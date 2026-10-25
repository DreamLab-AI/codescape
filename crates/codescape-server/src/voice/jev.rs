//! Jev option (VC-39-S1-01): a bounded System One Choice between the
//! deterministic grammar and the Loom fallback.
//!
//! When the grammar returns `NoMatch`, and only then, Jev may pick one of the
//! grammar's own **argument-free** commands, or `none`. Commands that need a
//! target, depth or name are never offered, so Jev can neither invent a
//! command nor supply arguments; those utterances still go to Loom. `none`,
//! every failure and any answer below the acceptance threshold return `None`
//! and the caller runs exactly today's Loom path.
//!
//! Off by default (`CODESCAPE_JEV=1` enables it), local-only by default
//! (a non-local endpoint needs `CODESCAPE_JEV_ALLOW_CLOUD=1`), and the
//! acceptance threshold defaults to 1.0 ("accept nothing automatically")
//! until an operator sets `CODESCAPE_JEV_MIN_CONFIDENCE` from labelled data.
//! The only thing sent is the normalised utterance, already capped at 512
//! characters; no repository content leaves.
//!
//! Endpoint resolution, first match wins: `CODESCAPE_JEV_ENDPOINT` (+
//! `CODESCAPE_JEV_MODEL`, key from the env var named by
//! `CODESCAPE_JEV_KEY_ENV`); the agentbox loopback proxy
//! (`AGENTBOX_SYSTEM_ONE_PROXY=1`, endpoint/model from `AGENTBOX_SYSTEM_ONE_*`
//! or `AGENTBOX_SKILL_ROUTE_*`, no Authorization header); the public API with
//! `TYPESAFE_API_KEY`. A local-only caller refuses the last two unless
//! `AGENTBOX_SYSTEM_ONE_BACKEND_LOCAL=1` or `CODESCAPE_JEV_LOCAL=1`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use std::future::Future;
use std::pin::Pin;
use codescape_collab::Command;
use serde_json::{json, Value};

use crate::egress::EgressAllowlist;

/// Public TypeSafe endpoint (resolution step 3).
pub const DEFAULT_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
/// Default model alias.
pub const DEFAULT_MODEL: &str = "jev-latest";
/// Hard deadline for the one interactive call; zero retries.
pub const DEFAULT_TIMEOUT_MS: u64 = 3_000;
/// Deadline ceiling for interactive paths.
pub const MAX_TIMEOUT_MS: u64 = 4_000;
/// Question ID in the request and response.
pub const QUESTION_ID: &str = "command";
/// The Choice key meaning "not one of these; use Loom".
pub const NONE_KEY: &str = "none";

/// Why Jev produced no command. Logged as one line, never with request text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// `CODESCAPE_JEV` not set.
    FlagOff,
    /// No usable endpoint.
    NoEndpoint,
    /// Deadline passed.
    Timeout,
    /// Transport failure.
    Transport,
    /// Non-2xx status.
    Http(u16),
    /// Unparsable or out-of-set answer.
    Malformed,
    /// Below the acceptance threshold.
    LowConfidence,
    /// Jev chose `none`.
    None,
}

impl Fallback {
    /// Stable log label.
    pub fn label(self) -> String {
        match self {
            Self::FlagOff => "flag-off".into(),
            Self::NoEndpoint => "no-endpoint".into(),
            Self::Timeout => "timeout".into(),
            Self::Transport => "transport".into(),
            Self::Http(c) => format!("http-{c}"),
            Self::Malformed => "malformed".into(),
            Self::LowConfidence => "low-confidence".into(),
            Self::None => "none".into(),
        }
    }
}

/// The argument-free commands Jev may select, as `(choice key, description, command)`.
/// Derived from the grammar's own `Command` variants; nothing here takes an argument.
pub fn offered() -> Vec<(&'static str, &'static str, Command)> {
    vec![
        ("show_all", "Clear every filter and show the whole graph again.", Command::ShowAll),
        ("hide_tests", "Hide test files and test symbols.", Command::HideTests),
        ("zoom_in", "Move the camera closer.", Command::ZoomIn),
        ("zoom_out", "Move the camera away.", Command::ZoomOut),
        ("overview", "Frame the whole graph.", Command::Overview),
        ("back", "Go back to the previous focus.", Command::Back),
        ("replay", "Replay the recorded timeline.", Command::Replay),
        ("live", "Return to the live view.", Command::Live),
        ("stop_following", "Stop following an actor.", Command::StopFollowing),
        ("help", "List what can be said.", Command::Help),
    ]
}

/// A resolved endpoint.
#[derive(Clone, PartialEq)]
pub struct Resolved {
    /// Full `…/v1/systemone` URL.
    pub url: String,
    /// Model name or alias.
    pub model: String,
    /// Bearer token; `None` for the proxy and keyless local endpoints.
    pub bearer: Option<String>,
    /// Hard deadline.
    pub timeout: Duration,
    /// Minimum Choice confidence to accept.
    pub min_confidence: f64,
}

impl std::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolved")
            .field("url", &self.url)
            .field("model", &self.model)
            .field("bearer", &self.bearer.as_ref().map(|_| "<set>"))
            .field("timeout", &self.timeout)
            .field("min_confidence", &self.min_confidence)
            .finish()
    }
}

type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

fn val(l: Lookup<'_>, k: &str) -> Option<String> {
    l(k).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn truthy(l: Lookup<'_>, k: &str) -> bool {
    matches!(val(l, k).map(|s| s.to_ascii_lowercase()).as_deref(), Some("1") | Some("true"))
}

fn systemone_url(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.ends_with("/v1/systemone") {
        b.to_string()
    } else {
        format!("{b}/v1/systemone")
    }
}

/// Resolve the option from `lookup`.
///
/// # Errors
/// [`Fallback::FlagOff`] when `CODESCAPE_JEV` is not `1`/`true`;
/// [`Fallback::NoEndpoint`] when nothing resolves (including a non-local
/// endpoint without `CODESCAPE_JEV_ALLOW_CLOUD`).
pub fn resolve(l: Lookup<'_>) -> Result<Resolved, Fallback> {
    if !truthy(l, "CODESCAPE_JEV") {
        return Err(Fallback::FlagOff);
    }
    let timeout = Duration::from_millis(
        val(l, "CODESCAPE_JEV_TIMEOUT_MS")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(100, MAX_TIMEOUT_MS),
    );
    // Accept nothing automatically until the operator fits a threshold.
    let min_confidence = val(l, "CODESCAPE_JEV_MIN_CONFIDENCE")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(1.0);
    let local = truthy(l, "AGENTBOX_SYSTEM_ONE_BACKEND_LOCAL")
        || truthy(l, "CODESCAPE_JEV_LOCAL")
        || truthy(l, "CODESCAPE_JEV_ALLOW_CLOUD");
    if let Some(ep) = val(l, "CODESCAPE_JEV_ENDPOINT") {
        let bearer = val(l, "CODESCAPE_JEV_KEY_ENV").and_then(|name| val(l, &name));
        return Ok(Resolved {
            url: systemone_url(&ep),
            model: val(l, "CODESCAPE_JEV_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
            bearer,
            timeout,
            min_confidence,
        });
    }
    if !local {
        return Err(Fallback::NoEndpoint);
    }
    if truthy(l, "AGENTBOX_SYSTEM_ONE_PROXY") {
        let ep = val(l, "AGENTBOX_SYSTEM_ONE_ENDPOINT").or_else(|| val(l, "AGENTBOX_SKILL_ROUTE_API"));
        let model = val(l, "AGENTBOX_SYSTEM_ONE_MODEL").or_else(|| val(l, "AGENTBOX_SKILL_ROUTE_MODEL"));
        if let (Some(ep), Some(model)) = (ep, model) {
            return Ok(Resolved { url: systemone_url(&ep), model, bearer: None, timeout, min_confidence });
        }
    }
    if let Some(key) = val(l, "TYPESAFE_API_KEY") {
        return Ok(Resolved {
            url: DEFAULT_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            bearer: Some(key),
            timeout,
            min_confidence,
        });
    }
    Err(Fallback::NoEndpoint)
}

/// The URL to add to the egress allowlist, when the option is enabled and resolves.
pub fn egress_url(l: Lookup<'_>) -> Option<String> {
    resolve(l).ok().map(|r| r.url)
}

/// The `POST /v1/systemone` body for `utterance`.
pub fn build_request(model: &str, utterance: &str) -> Value {
    let criteria: BTreeMap<String, String> = offered()
        .into_iter()
        .map(|(k, d, _)| (k.to_string(), d.to_string()))
        .chain(std::iter::once((
            NONE_KEY.to_string(),
            "None of the other choices: the utterance asks for something else, names a target, or is not a view command.".to_string(),
        )))
        .collect();
    json!({
        "model": model,
        "state": { "utterance": utterance },
        "questions": {
            QUESTION_ID: {
                "type": "choice",
                "instructions": "Which view command does the `utterance` ask for? Choose none unless one of the listed commands is exactly what was asked.",
                "criteria": criteria,
            }
        }
    })
}

/// A transport failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    /// Deadline passed.
    Timeout,
    /// Anything else.
    Other,
}

/// A raw reply.
pub struct Raw {
    /// HTTP status.
    pub status: u16,
    /// Body text.
    pub body: String,
}

/// A boxed transport future.
pub type PostFuture<'a> = Pin<Box<dyn Future<Output = Result<Raw, TransportError>> + Send + 'a>>;

/// HTTP seam, mockable in tests.
pub trait Transport: Send + Sync {
    /// POST `body` as JSON.
    fn post<'a>(&'a self, url: &'a str, bearer: Option<&'a str>, body: &'a Value, deadline: Duration)
        -> PostFuture<'a>;
}

/// reqwest transport through the egress allowlist.
pub struct HttpTransport {
    http: reqwest::Client,
    allow: EgressAllowlist,
}

impl HttpTransport {
    /// Build over `allow`.
    pub fn new(allow: &EgressAllowlist) -> Self {
        Self { http: allow.http_client(Duration::from_millis(MAX_TIMEOUT_MS)), allow: allow.clone() }
    }
}

impl Transport for HttpTransport {
    fn post<'a>(&'a self, url: &'a str, bearer: Option<&'a str>, body: &'a Value, deadline: Duration)
        -> PostFuture<'a> {
        Box::pin(async move {
            self.allow.check(url).map_err(|_| TransportError::Other)?;
            let mut rb = self.http.post(url).timeout(deadline).json(body);
            if let Some(b) = bearer {
                rb = rb.bearer_auth(b);
            }
            let r = rb.send().await.map_err(|e| {
                if e.is_timeout() { TransportError::Timeout } else { TransportError::Other }
            })?;
            let status = r.status().as_u16();
            let body = r.text().await.map_err(|_| TransportError::Other)?;
            Ok(Raw { status, body })
        })
    }
}

/// The Jev client held in server state.
pub struct JevClient {
    resolved: Resolved,
    transport: Arc<dyn Transport>,
}

impl JevClient {
    /// `Some` when the option is enabled and an endpoint resolves.
    pub fn from_lookup(l: Lookup<'_>, allow: &EgressAllowlist) -> Option<Self> {
        match resolve(l) {
            Ok(resolved) => Some(Self { resolved, transport: Arc::new(HttpTransport::new(allow)) }),
            Err(Fallback::FlagOff) => None,
            Err(e) => {
                tracing::info!(reason = %e.label(), "Jev option unavailable");
                None
            }
        }
    }

    /// With an explicit transport (tests).
    pub fn with_transport(resolved: Resolved, transport: Arc<dyn Transport>) -> Self {
        Self { resolved, transport }
    }

    /// Pick one offered command for `utterance`, or say why not.
    ///
    /// # Errors
    /// Any [`Fallback`]; the caller must then run today's Loom path unchanged.
    pub async fn pick(&self, utterance: &str) -> Result<Command, Fallback> {
        let body = build_request(&self.resolved.model, utterance);
        let deadline = self.resolved.timeout;
        let call = self.transport.post(&self.resolved.url, self.resolved.bearer.as_deref(), &body, deadline);
        let raw = match tokio::time::timeout(deadline, call).await {
            Err(_) | Ok(Err(TransportError::Timeout)) => return Err(Fallback::Timeout),
            Ok(Err(TransportError::Other)) => return Err(Fallback::Transport),
            Ok(Ok(r)) => r,
        };
        if !(200..300).contains(&raw.status) {
            return Err(Fallback::Http(raw.status));
        }
        let v: Value = serde_json::from_str(&raw.body).map_err(|_| Fallback::Malformed)?;
        let a = v.pointer(&format!("/answers/{QUESTION_ID}")).ok_or(Fallback::Malformed)?;
        if a.get("type").and_then(Value::as_str) != Some("choice") {
            return Err(Fallback::Malformed);
        }
        let key = a.get("choice").and_then(Value::as_str).ok_or(Fallback::Malformed)?;
        let confidence = a
            .get("confidence")
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && (0.0..=1.0).contains(c))
            .ok_or(Fallback::Malformed)?;
        if key == NONE_KEY {
            return Err(Fallback::None);
        }
        let cmd = offered().into_iter().find(|(k, _, _)| *k == key).map(|(_, _, c)| c).ok_or(Fallback::Malformed)?;
        if confidence < self.resolved.min_confidence {
            return Err(Fallback::LowConfidence);
        }
        // The grammar's own validation still has the last word.
        cmd.validate().map_err(|_| Fallback::Malformed)?;
        Ok(cmd)
    }

    /// [`Self::pick`] with the fallback logged as one line and no request text.
    pub async fn pick_logged(&self, utterance: &str) -> Option<Command> {
        match self.pick(utterance).await {
            Ok(c) => Some(c),
            Err(f) => {
                tracing::info!(reason = %f.label(), "Jev command pick fell back to Loom");
                None
            }
        }
    }
}

/// The route decision: given the grammar's result, let Jev replace a `NoMatch`
/// (and nothing else) with one of its offered commands.
///
/// `Ok(_)` (a grammar hit), `Empty` and `Invalid` are returned untouched and
/// Jev is not called. A `NoMatch` is also returned untouched whenever Jev is
/// absent or yields no command, so the caller's Loom path is exactly today's.
pub async fn after_grammar(
    parsed: Result<Command, codescape_collab::ParseError>,
    jev: Option<&JevClient>,
) -> Result<Command, codescape_collab::ParseError> {
    use codescape_collab::ParseError;
    match (&parsed, jev) {
        (Err(ParseError::NoMatch { normalised }), Some(j)) => match j.pick_logged(normalised).await {
            Some(c) => Ok(c),
            None => parsed,
        },
        _ => parsed,
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Mutex;

    pub enum Script {
        Reply(u16, String),
        Fail(TransportError),
        Hang,
    }

    pub struct Mock {
        pub script: Script,
        pub seen: Mutex<Vec<Value>>,
        pub bearers: Mutex<Vec<Option<String>>>,
    }

    impl Mock {
        pub fn new(script: Script) -> Arc<Self> {
            Arc::new(Self { script, seen: Mutex::new(vec![]), bearers: Mutex::new(vec![]) })
        }
    }

    impl Transport for Mock {
        fn post<'a>(&'a self, _u: &'a str, bearer: Option<&'a str>, body: &'a Value, _d: Duration) -> PostFuture<'a> {
            self.seen.lock().unwrap().push(body.clone());
            self.bearers.lock().unwrap().push(bearer.map(String::from));
            Box::pin(async move {
                match &self.script {
                    Script::Reply(s, b) => Ok(Raw { status: *s, body: b.clone() }),
                    Script::Fail(e) => Err(*e),
                    Script::Hang => {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        Err(TransportError::Timeout)
                    }
                }
            })
        }
    }

    pub fn choice(key: &str, conf: f64) -> String {
        json!({"answers": {QUESTION_ID: {"type":"choice","choice":key,"confidence":conf,"legend":{},"probabilities":{}}}}).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use std::collections::HashMap;

    fn env(p: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = p.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    fn client(script: Script, min: f64) -> (JevClient, Arc<Mock>) {
        let m = Mock::new(script);
        let r = Resolved {
            url: "http://127.0.0.1:1/v1/systemone".into(),
            model: "m".into(),
            bearer: None,
            timeout: Duration::from_millis(150),
            min_confidence: min,
        };
        (JevClient::with_transport(r, m.clone()), m)
    }

    #[test]
    fn off_by_default_even_with_every_endpoint() {
        let e = env(&[("TYPESAFE_API_KEY", "k"), ("CODESCAPE_JEV_ENDPOINT", "http://x"), ("AGENTBOX_SYSTEM_ONE_PROXY", "1")]);
        assert_eq!(resolve(&e), Err(Fallback::FlagOff));
        assert_eq!(egress_url(&e), None);
    }

    #[test]
    fn local_only_by_default_and_allow_cloud_overrides() {
        let e = env(&[("CODESCAPE_JEV", "1"), ("TYPESAFE_API_KEY", "k")]);
        assert_eq!(resolve(&e), Err(Fallback::NoEndpoint));
        let e = env(&[("CODESCAPE_JEV", "1"), ("TYPESAFE_API_KEY", "k"), ("CODESCAPE_JEV_ALLOW_CLOUD", "1")]);
        let r = resolve(&e).unwrap();
        assert_eq!(r.url, DEFAULT_ENDPOINT);
        assert_eq!(r.bearer.as_deref(), Some("k"));
        assert_eq!(r.min_confidence, 1.0, "accept nothing automatically by default");
    }

    #[test]
    fn proxy_sends_no_bearer_and_accepts_the_skill_route_pair() {
        let e = env(&[
            ("CODESCAPE_JEV", "1"),
            ("AGENTBOX_SYSTEM_ONE_BACKEND_LOCAL", "1"),
            ("AGENTBOX_SYSTEM_ONE_PROXY", "1"),
            ("AGENTBOX_SKILL_ROUTE_API", "http://127.0.0.1:8097"),
            ("AGENTBOX_SKILL_ROUTE_MODEL", "jev-x"),
            ("TYPESAFE_API_KEY", "k"),
        ]);
        let r = resolve(&e).unwrap();
        assert_eq!((r.url.as_str(), r.model.as_str(), r.bearer), ("http://127.0.0.1:8097/v1/systemone", "jev-x", None));
    }

    #[test]
    fn explicit_config_wins_and_timeout_is_clamped() {
        let e = env(&[
            ("CODESCAPE_JEV", "true"),
            ("CODESCAPE_JEV_ENDPOINT", "http://127.0.0.1:9/v1/systemone"),
            ("CODESCAPE_JEV_KEY_ENV", "MY_KEY"),
            ("MY_KEY", "s"),
            ("CODESCAPE_JEV_TIMEOUT_MS", "999999"),
            ("CODESCAPE_JEV_MIN_CONFIDENCE", "0.8"),
        ]);
        let r = resolve(&e).unwrap();
        assert_eq!(r.bearer.as_deref(), Some("s"));
        assert_eq!(r.timeout, Duration::from_millis(MAX_TIMEOUT_MS));
        assert_eq!(r.min_confidence, 0.8);
        assert!(!format!("{r:?}").contains('s') || !format!("{r:?}").contains("\"s\""));
    }

    #[test]
    fn wire_shape_offers_only_argument_free_commands_plus_none() {
        let v = build_request("jev-latest", "put it all back");
        assert_eq!(v["state"]["utterance"], "put it all back");
        let q = &v["questions"][QUESTION_ID];
        assert_eq!(q["type"], "choice");
        let keys: Vec<&str> = q["criteria"].as_object().unwrap().keys().map(String::as_str).collect();
        assert!(keys.contains(&NONE_KEY) && keys.contains(&"show_all"));
        assert_eq!(keys.len(), offered().len() + 1);
        for (_, _, c) in offered() {
            assert!(c.validate().is_ok());
            // Round-trips through the grammar's own wire form with no arguments.
            let j = serde_json::to_value(&c).unwrap();
            assert!(j.is_string(), "{j} carries arguments");
        }
    }

    #[tokio::test]
    async fn accept_path_selects_the_offered_command() {
        let (c, m) = client(Script::Reply(200, choice("show_all", 0.95)), 0.8);
        assert_eq!(c.pick("put everything back").await, Ok(Command::ShowAll));
        assert_eq!(m.seen.lock().unwrap().len(), 1);
        assert_eq!(c.pick_logged("x").await, Some(Command::ShowAll));
    }

    #[tokio::test]
    async fn every_failure_returns_no_command() {
        for (script, want) in [
            (Script::Reply(200, choice("none", 0.99)), Fallback::None),
            (Script::Reply(200, choice("zoom_in", 0.5)), Fallback::LowConfidence),
            (Script::Reply(200, choice("delete_everything", 0.99)), Fallback::Malformed),
            (Script::Reply(200, choice("focus", 0.99)), Fallback::Malformed),
            (Script::Reply(200, "not json".into()), Fallback::Malformed),
            (Script::Reply(200, "{}".into()), Fallback::Malformed),
            (Script::Reply(429, "{}".into()), Fallback::Http(429)),
            (Script::Reply(500, "{}".into()), Fallback::Http(500)),
            (Script::Fail(TransportError::Other), Fallback::Transport),
            (Script::Fail(TransportError::Timeout), Fallback::Timeout),
            (Script::Hang, Fallback::Timeout),
        ] {
            let (c, _) = client(script, 0.8);
            assert_eq!(c.pick("x").await, Err(want));
            assert_eq!(c.pick_logged("x").await, None);
        }
    }

    use codescape_collab::{parse_command, ParseError};

    /// Every utterance the grammar's own golden table parses, plus Empty/Invalid,
    /// must come back identical and must never reach the transport.
    #[tokio::test]
    async fn grammar_hits_empty_and_invalid_never_reach_jev() {
        let (c, m) = client(Script::Reply(200, choice("help", 1.0)), 0.0);
        let (mut ok, mut empty) = (0, 0);
        for u in [
            "zoom in", "zoom out", "overview", "show all", "hide tests", "go back", "help",
            "callers of main", "explain this", "", "   ", "show only", "rewind 0 seconds",
            "callers of main depth 99",
        ] {
            let before = parse_command(u);
            match &before {
                Ok(_) => ok += 1,
                Err(ParseError::Empty) => empty += 1,
                Err(ParseError::Invalid { .. }) => unreachable!("parse_command never yields Invalid"),
                Err(ParseError::NoMatch { .. }) => continue, // Jev may legitimately act here
            }
            let after = after_grammar(parse_command(u), Some(&c)).await;
            assert_eq!(before, after, "{u:?}");
        }
        assert!(ok > 0 && empty > 0, "ok={ok} empty={empty}");
        // `Invalid` only comes from validated model replies, never from the grammar:
        // built by hand, it must pass through untouched.
        let inv = Err(ParseError::Invalid { reason: "depth".into() });
        assert_eq!(after_grammar(inv.clone(), Some(&c)).await, inv);
        assert!(m.seen.lock().unwrap().is_empty(), "Jev was consulted off the NoMatch path");
    }

    #[tokio::test]
    async fn no_match_is_identical_when_jev_is_absent_or_fails() {
        let u = "please make the picture bigger somehow";
        let before = parse_command(u);
        assert!(matches!(before, Err(ParseError::NoMatch { .. })));
        assert_eq!(after_grammar(parse_command(u), None).await, before, "flag off / no endpoint");
        for script in [
            Script::Reply(200, choice("none", 0.99)),
            Script::Reply(200, choice("zoom_in", 0.1)),
            Script::Reply(200, choice("not_offered", 0.99)),
            Script::Reply(200, "garbage".into()),
            Script::Reply(503, "{}".into()),
            Script::Fail(TransportError::Other),
            Script::Hang,
        ] {
            let (c, _) = client(script, 0.8);
            assert_eq!(after_grammar(parse_command(u), Some(&c)).await, before);
        }
    }

    #[tokio::test]
    async fn no_match_accept_path_substitutes_only_an_offered_command() {
        let (c, m) = client(Script::Reply(200, choice("zoom_in", 0.95)), 0.8);
        let r = after_grammar(parse_command("make the picture bigger somehow"), Some(&c)).await;
        assert_eq!(r, Ok(Command::ZoomIn));
        assert_eq!(m.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn default_threshold_accepts_nothing_below_certainty() {
        let (c, _) = client(Script::Reply(200, choice("help", 0.99)), 1.0);
        assert_eq!(c.pick("x").await, Err(Fallback::LowConfidence));
    }
}
