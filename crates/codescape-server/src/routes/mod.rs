//! HTTP and WebSocket routes on the main port.
//!
//! | Route | |
//! |---|---|
//! | `GET /api/health` | engine, repo, revision, counts, settled, version |
//! | `GET /api/graph/snapshot?tier=l0\|l1\|l2&focus_files=1,2` | projected `GraphSnapshot` |
//! | `GET /api/graph/node/{id}` | node, position, heat, neighbours |
//! | `GET /api/graph/search?q=&limit=` | ranked matches |
//! | `POST /api/graph/query {text}` | `QueryResult` |
//! | `GET /api/settings/physics` | layout parameters (XR `PhysicsView` keys) |
//! | `GET /api/graph/settlement` | XR settlement body |
//! | `GET /api/graph/ontology-refs` | every `@ontology` annotation as `[{key, id, path, line, iri}]` (VisionClaw ADR-2164 D3) |
//! | `WS /ws/positions`, `/ws/events`, `/wss`, `/ws/voice` | see [`ws`] and [`voice`] |
//! | `WS /ws/presence` | XR avatar rooms, see [`presence`] |
//!
//! Every route takes an optional `?repo=<label>` (default: the first repo).

pub mod presence;
pub mod voice;
pub mod ws;

use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use codescape_collab::{command_from_llm, command_schema, parse_command, ParseError};
use codescape_ingest::annotations::AnnotationTable;
use codescape_model::events::QueryResult;
use codescape_model::{lod, GraphSnapshot, Tier};
use codescape_server::{now_ms, VERSION};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::json;

use crate::engine::RepoEngine;
use crate::queries::{execute, CommandCtx, Effect};
use crate::state::{parse_tier, AppState};

/// Common `?repo=` parameter.
#[derive(Debug, Deserialize, Default)]
pub struct RepoQuery {
    /// Repository label.
    pub repo: Option<String>,
}

fn repo_or_404(st: &AppState, label: Option<&str>) -> Result<Arc<RepoEngine>, Box<HttpResponse>> {
    st.repo(label).ok_or_else(|| {
        Box::new(HttpResponse::NotFound().json(json!({"error": "unknown repo", "repo": label})))
    })
}

async fn health(st: web::Data<AppState>, q: web::Query<RepoQuery>) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let p = repo.current();
    let l = repo.layout.read().clone();
    HttpResponse::Ok().json(json!({
        "engine": l.engine,
        "engine_reason": l.reason,
        "repo": repo.label,
        "repos": st.repos.iter().map(|r| r.label.clone()).collect::<Vec<_>>(),
        "epoch": p.snapshot.epoch,
        "revision": p.snapshot.revision,
        "nodes": p.snapshot.nodes.len(),
        "edges": p.snapshot.edges.len(),
        "tiers": p.snapshot.tier_counts(),
        "settled": l.settled,
        "layout_steps": l.steps,
        "energy": l.energy,
        "auth": st.verifier.name(),
        "dropped_change_batches": repo.changes.dropped(),
        "connections": st.connections(),
        "version": VERSION,
    }))
}

/// `?tier=&focus_files=` for snapshot projection.
#[derive(Debug, Deserialize)]
pub struct SnapshotQuery {
    repo: Option<String>,
    tier: Option<String>,
    focus_files: Option<String>,
}

/// The projected sub-snapshot: nodes at or above `tier` plus focus-file symbols, and the edges among them.
pub fn project_snapshot(s: &GraphSnapshot, tier: Tier, focus_files: &[u32]) -> GraphSnapshot {
    let ids: std::collections::HashSet<u32> =
        lod::project(s, tier, focus_files).into_iter().collect();
    GraphSnapshot {
        repo: s.repo.clone(),
        epoch: s.epoch,
        revision: s.revision,
        generated_at_ms: s.generated_at_ms,
        nodes: s
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .cloned()
            .collect(),
        edges: s
            .edges
            .iter()
            .filter(|e| ids.contains(&e.source) && ids.contains(&e.target))
            .cloned()
            .collect(),
    }
}

fn parse_ids(s: Option<&str>) -> Vec<u32> {
    s.unwrap_or("")
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect()
}

async fn snapshot(st: web::Data<AppState>, q: web::Query<SnapshotQuery>) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let tier = match q.tier.as_deref() {
        None => Tier::L1,
        Some(t) => match parse_tier(t) {
            Some(t) => t,
            None => {
                return HttpResponse::BadRequest()
                    .json(json!({"error": "tier must be l0, l1 or l2"}))
            }
        },
    };
    let focus = parse_ids(q.focus_files.as_deref());
    // Cached per (revision, tier, focus set): repeated l2 requests reuse one body.
    match web::block(move || repo.snapshot_body(tier, &focus)).await {
        Ok(Ok(body)) => HttpResponse::Ok()
            .content_type("application/json")
            .body(body),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

async fn node(
    st: web::Data<AppState>,
    path: web::Path<u32>,
    q: web::Query<RepoQuery>,
) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let id = path.into_inner();
    let p = repo.current();
    let Some(n) = p.index.get(&id).map(|&i| p.snapshot.nodes[i].clone()) else {
        return HttpResponse::NotFound().json(json!({"error": "unknown node", "id": id}));
    };
    let pos = {
        let f = repo.positions.read();
        f.index.get(&id).map(|&i| f.pos[i])
    };
    let heat = repo
        .collab
        .lock()
        .registry
        .heat(now_ms())
        .get(&id)
        .copied()
        .unwrap_or(0.0);
    let r2 = repo.clone();
    let neighbours = web::block(move || {
        let ing = r2.ingest.read();
        let mut v: Vec<_> = ing
            .neighbors(id, None, false, true)
            .into_iter()
            .map(|(m, rel)| json!({"id": m, "relation": rel.as_str(), "direction": "out"}))
            .collect();
        v.extend(
            ing.neighbors(id, None, true, false)
                .into_iter()
                .map(|(m, rel)| json!({"id": m, "relation": rel.as_str(), "direction": "in"})),
        );
        v.truncate(200);
        v
    })
    .await
    .unwrap_or_default();
    HttpResponse::Ok()
        .json(json!({ "node": n, "position": pos, "heat": heat, "neighbours": neighbours }))
}

/// `?q=&limit=`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    repo: Option<String>,
    q: String,
    limit: Option<usize>,
}

async fn search(st: web::Data<AppState>, q: web::Query<SearchQuery>) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    let query = q.q.clone();
    let r2 = repo.clone();
    let hits = web::block(move || r2.ingest.read().search(&query, limit))
        .await
        .unwrap_or_default();
    let p = repo.current();
    let out: Vec<_> = hits
        .into_iter()
        .filter_map(|(id, score)| {
            let n = &p.snapshot.nodes[*p.index.get(&id)?];
            Some(json!({"id": id, "label": n.label.as_ref(), "kind": n.kind.as_str(), "path": n.path.as_deref(), "score": score}))
        })
        .collect();
    HttpResponse::Ok().json(out)
}

/// Body of `POST /api/graph/query`.
#[derive(Debug, Deserialize)]
pub struct QueryBody {
    text: String,
    #[serde(default)]
    focus: Option<u32>,
}

async fn query(
    st: web::Data<AppState>,
    q: web::Query<RepoQuery>,
    body: web::Json<QueryBody>,
) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let ctx = Arc::new(Mutex::new(CommandCtx {
        focus: body.focus,
        ..Default::default()
    }));
    match run_command(&st, &repo, &body.text, ctx).await {
        Ok((r, _)) => HttpResponse::Ok().json(r),
        Err(e) => HttpResponse::UnprocessableEntity().json(json!({"error": e})),
    }
}

const LOOM_SYSTEM: &str =
    "You map a spoken request about a source-code graph onto one view command. \
Reply with JSON only: {\"command\": <command or null>, \"answer\": <one short spoken sentence>}. \
Commands are externally tagged, for example: {\"focus\":{\"name\":\"stable_match\"}}, \
{\"explain\":\"this\"}, {\"callers\":{\"target\":{\"name\":\"parse_command\"},\"depth\":1}}, \
{\"affected\":{\"target\":{\"name\":\"GraphDelta\"},\"depth\":3}}, \
{\"path\":{\"from\":{\"name\":\"a\"},\"to\":{\"name\":\"b\"}}}, \
{\"show_only\":{\"lang\":\"rust\",\"community\":null}}, \"zoom_out\", \"overview\". \
\"Bring up\", \"show me\" or \"open\" a named thing means focus. Use the identifier the speaker \
described (snake_case or CamelCase) as the name. Use null when the request is not about the code \
graph, and answer briefly; never invent a target that was not described.";

/// Parse with the grammar; on no match, ask Loom (when configured) and validate its reply.
/// Returns the result and the focus effect for the caller to apply.
pub async fn run_command(
    st: &AppState,
    repo: &Arc<RepoEngine>,
    text: &str,
    ctx: Arc<Mutex<CommandCtx>>,
) -> Result<(QueryResult, Effect), String> {
    // Bound the work before the grammar (and Loom) sees it.
    let truncated = text.chars().count() > MAX_COMMAND_CHARS;
    let owned: String;
    let text = if truncated {
        owned = text.chars().take(MAX_COMMAND_CHARS).collect();
        owned.as_str()
    } else {
        text
    };
    let mut out = run_command_inner(st, repo, text, ctx).await;
    if truncated {
        let note = format!(" (Your command was cut to {MAX_COMMAND_CHARS} characters.)");
        match &mut out {
            Ok((r, _)) => r.answer.push_str(&note),
            Err(e) => e.push_str(&note),
        }
    }
    out
}

/// Text spoken when the model's reply fails validation. The reply is
/// untrusted and unvalidated, so none of it is echoed (W4COVC-F2).
fn invalid_reply_answer(_reply: &serde_json::Value) -> String {
    "I could not interpret that.".to_owned()
}

/// Longest command text parsed (characters); longer input is truncated and the answer says so.
pub const MAX_COMMAND_CHARS: usize = 512;

async fn run_command_inner(
    st: &AppState,
    repo: &Arc<RepoEngine>,
    text: &str,
    ctx: Arc<Mutex<CommandCtx>>,
) -> Result<(QueryResult, Effect), String> {
    // Jev option (VC-39-S1-01): consulted only on a grammar NoMatch; any miss leaves
    // the grammar result untouched so the Loom path below is exactly as before.
    let parsed =
        codescape_server::voice::jev::after_grammar(parse_command(text), st.jev.as_ref()).await;
    let cmd = match parsed {
        Ok(c) => Some((c, None)),
        Err(ParseError::Empty) => return Err("Say or type a command.".into()),
        Err(ParseError::NoMatch { normalised })
        | Err(ParseError::Invalid { reason: normalised }) => {
            let Some(loom) = st.loom.as_ref() else {
                return Err(format!(
                    "I did not understand \"{normalised}\". Say help for examples."
                ));
            };
            match loom.ask(LOOM_SYSTEM, text, &command_schema()).await {
                Ok(v) => match command_from_llm(&v) {
                    Ok((Some(c), answer)) => Some((c, Some(answer))),
                    Ok((None, answer)) => {
                        return Ok((
                            QueryResult {
                                answer,
                                ..Default::default()
                            },
                            Effect::None,
                        ))
                    }
                    Err(e) => {
                        // Spoken as text, never executed.
                        tracing::info!(error = %e, "Loom reply failed validation");
                        let answer = invalid_reply_answer(&v);
                        return Ok((
                            QueryResult {
                                answer,
                                ..Default::default()
                            },
                            Effect::None,
                        ));
                    }
                },
                Err(codescape_server::voice::loom::LoomError::Busy) => {
                    return Err("The language model is busy; try again in a moment.".into());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Loom fallback failed");
                    return Err(format!(
                        "I did not understand \"{text}\", and the language model is unavailable."
                    ));
                }
            }
        }
    };
    let (cmd, llm_answer) = cmd.expect("set above");
    let repo = repo.clone();
    let res = web::block(move || {
        let mut c = ctx.lock();
        execute(&repo, &cmd, &mut c, now_ms())
    })
    .await
    .map_err(|e| e.to_string())?;
    let (mut r, eff) = res;
    if let Some(a) = llm_answer.filter(|a| !a.is_empty()) {
        if r.answer.is_empty() {
            r.answer = a;
        }
    }
    Ok((r, eff))
}

async fn physics(st: web::Data<AppState>) -> HttpResponse {
    let p = codescape_layout::LayoutParams::default();
    let engine = st
        .repos
        .first()
        .map(|r| r.layout.read().engine.clone())
        .unwrap_or_default();
    HttpResponse::Ok().json(json!({
        "repelK": p.repulsion,
        "springK": p.spring_k,
        "damping": p.damping,
        "gravity": p.gravity,
        "dt": p.dt,
        "maxVelocity": p.max_velocity,
        "theta": p.theta,
        "structuralWeight": p.structural_weight,
        "intraCommunityWeight": p.intra_community_weight,
        "interCommunityWeight": p.inter_community_weight,
        "dagBiasK": 0.0,
        "engine": engine,
        "source": "codescape",
    }))
}

async fn settlement(st: web::Data<AppState>, q: web::Query<RepoQuery>) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let l = repo.layout.read().clone();
    HttpResponse::Ok().json(json!({
        "settlementState": { "isSettled": l.settled, "kineticEnergy": l.energy, "steps": l.steps },
        "telemetryLive": !l.engine.is_empty(),
        "physicsRunning": !l.engine.is_empty(),
        "engine": l.engine,
        "revision": repo.current().snapshot.revision,
    }))
}

/// `[{key, id, path, line, iri}]` for every bound annotation, sorted by path,
/// line, IRI and node. `key` and `id` name the node the annotation binds to (a
/// symbol, or the file); file roll-ups are not repeated here.
pub fn ontology_refs_body(snapshot: &GraphSnapshot, table: &AnnotationTable) -> serde_json::Value {
    let index = snapshot.index();
    serde_json::Value::Array(
        table
            .annotations()
            .iter()
            .filter_map(|a| {
                let n = &snapshot.nodes[*index.get(&a.node)?];
                Some(json!({
                    "key": n.key.as_ref(),
                    "id": a.node,
                    "path": a.path.as_ref(),
                    "line": a.line,
                    "iri": a.iri.as_ref(),
                }))
            })
            .collect(),
    )
}

async fn ontology_refs(st: web::Data<AppState>, q: web::Query<RepoQuery>) -> HttpResponse {
    let repo = match repo_or_404(&st, q.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let p = repo.current();
    HttpResponse::Ok().json(ontology_refs_body(&p.snapshot, &p.annotations))
}

/// Register every main-port route except static files.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/health", web::get().to(health))
        .route("/api/graph/snapshot", web::get().to(snapshot))
        .route("/api/graph/node/{id}", web::get().to(node))
        .route("/api/graph/search", web::get().to(search))
        .route("/api/graph/query", web::post().to(query))
        .route("/api/settings/physics", web::get().to(physics))
        .route("/api/graph/settlement", web::get().to(settlement))
        .route("/api/graph/ontology-refs", web::get().to(ontology_refs))
        .route("/ws/positions", web::get().to(ws::positions))
        .route("/ws/events", web::get().to(ws::events))
        .route("/wss", web::get().to(ws::wss))
        .route("/ws/voice", web::get().to(voice::voice))
        .route("/ws/presence", web::get().to(presence::presence));
}

/// Verify a request with the configured verifier (open access until P6).
pub fn verify(st: &AppState, req: &HttpRequest) -> Result<(), Box<HttpResponse>> {
    let auth = req
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());
    // Rebuilt from the public origin, never from the client-controlled Host header.
    let url = st
        .public
        .url_for(req.headers(), req.peer_addr().map(|a| a.ip()), req.uri());
    st.verifier
        .verify_http(req.method().as_str(), &url, auth)
        .map(|_| ())
        .map_err(|e| Box::new(HttpResponse::Unauthorized().json(json!({"error": e.to_string()}))))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use codescape_model::{CodeEdge, CodeNode, Confidence, Lang, NodeKind, Relation};

    use super::*;

    fn n(id: u32, kind: NodeKind, tier: Tier, parent: u32) -> CodeNode {
        CodeNode {
            id,
            key: Arc::from(format!("k{id}")),
            kind,
            lang: Lang::Rust,
            label: Arc::from(format!("n{id}")),
            path: Some(Arc::from("src/a.rs")),
            span: None,
            parent,
            community: 1,
            cluster: 1,
            importance: 0.0,
            tier,
            size: 0,
        }
    }

    /// A Loom backend that is a local stub: it answers every request with a chat
    /// completion whose message content is `reply` (the model's own JSON text),
    /// and counts the requests it served. No network beyond the loopback port.
    async fn stub_loom(reply: serde_json::Value) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().unwrap().port()
        );
        let served = Arc::new(AtomicUsize::new(0));
        let counter = served.clone();
        let body =
            json!({ "choices": [{ "message": { "content": reply.to_string() } }] }).to_string();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                // Read the whole request (headers, then Content-Length bytes).
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
                    let want = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + want {
                        break;
                    }
                }
                counter.fetch_add(1, Ordering::SeqCst);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        (base, served)
    }

    /// A served repository and an `AppState` whose Loom client points at `loom_url`.
    fn state_with_loom(dir: &std::path::Path, loom_url: &str) -> (AppState, Arc<RepoEngine>) {
        use clap::Parser;
        use codescape_server::auth::build_verifier;
        use codescape_server::egress::EgressAllowlist;
        use codescape_server::origin::OriginPolicy;
        use codescape_server::persist::ActorIds;
        use codescape_server::proxy::PublicOrigin;

        std::fs::create_dir_all(dir.join("repo/src")).unwrap();
        std::fs::write(dir.join("repo/src/a.rs"), "fn alpha() {}\n").unwrap();
        let repo_arg = format!("t={}", dir.join("repo").display());
        let data = dir.join("data");
        let cfg = codescape_server::config::Config::try_parse_from([
            "codescape-server",
            "--repo",
            repo_arg.as_str(),
            "--data-dir",
            data.to_str().unwrap(),
            "--loom-url",
            loom_url,
            "--loom-model",
            "stub-model",
            "--port",
            "0",
            "--ingest-port",
            "0",
        ])
        .unwrap();
        let (engine, _threads) = RepoEngine::open(&cfg, &cfg.repos[0]).unwrap();
        let egress = EgressAllowlist::from_urls(cfg.egress_urls()).unwrap();
        let public = PublicOrigin::from_config(&cfg).unwrap();
        let st = AppState::new(
            Arc::new(cfg),
            vec![engine.clone()],
            egress,
            build_verifier(false).unwrap(),
            codescape_collab::PathMap::new(),
            ActorIds::new(0),
            OriginPolicy::new(&[]).unwrap(),
            public,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        (st, engine)
    }

    /// Drive `run_command_inner` with text the grammar cannot match, so the
    /// request reaches the (stub) Loom, and return what the handler answers.
    fn ask_stub_loom(reply: serde_json::Value) -> (Result<(QueryResult, Effect), String>, usize) {
        let dir = tempfile::tempdir().unwrap();
        actix_web::rt::System::new().block_on(async move {
            let (base, served) = stub_loom(reply).await;
            let (st, repo) = state_with_loom(dir.path(), &base);
            let ctx = Arc::new(Mutex::new(CommandCtx::default()));
            let out = run_command_inner(&st, &repo, "frobnicate the quuxwidget", ctx).await;
            (out, served.load(std::sync::atomic::Ordering::SeqCst))
        })
    }

    /// W4COVC-F2 at handler level: an over-long model answer reaches the caller
    /// capped at `MAX_ANSWER_LEN`, not as the model wrote it.
    #[test]
    fn handler_caps_an_over_long_model_answer() {
        let long = "x".repeat(100_000);
        let (out, served) = ask_stub_loom(json!({"command": null, "answer": long}));
        assert_eq!(served, 1, "the stub Loom was consulted exactly once");
        let (result, effect) = out.expect("a model that only answers is not an error");
        assert!(matches!(effect, Effect::None));
        assert_eq!(
            result.answer.chars().count(),
            codescape_collab::llm::MAX_ANSWER_LEN
        );
        assert!(
            result.answer.ends_with('\u{2026}'),
            "a cut answer ends with an ellipsis"
        );
        assert!(result.answer.starts_with("xxx"));
    }

    /// W4COVC-F2 at handler level: a reply that fails validation is spoken as the
    /// fixed string, with none of the model's text, and executes nothing.
    #[test]
    fn handler_answers_a_failed_validation_with_the_fixed_string() {
        let long = "x".repeat(100_000);
        let (out, served) = ask_stub_loom(json!({"command": "rm_rf", "answer": long}));
        assert_eq!(served, 1, "the stub Loom was consulted exactly once");
        let (result, effect) = out.expect("a reply that fails validation is spoken, not an error");
        assert!(matches!(effect, Effect::None));
        assert_eq!(result.answer, "I could not interpret that.");
        assert!(!result.answer.contains('x'));
    }

    #[test]
    fn invalid_loom_reply_never_echoes_model_text() {
        let long = "x".repeat(100_000);
        let v = json!({"command": "rm_rf", "answer": long});
        assert!(command_from_llm(&v).is_err());
        let spoken = invalid_reply_answer(&v);
        assert_eq!(spoken, "I could not interpret that.");
    }

    /// Every example the Loom prompt teaches must validate into a `Command`.
    #[test]
    fn loom_prompt_examples_are_valid_commands() {
        let examples = [
            r#"{"focus":{"name":"stable_match"}}"#,
            r#"{"explain":"this"}"#,
            r#"{"callers":{"target":{"name":"parse_command"},"depth":1}}"#,
            r#"{"affected":{"target":{"name":"GraphDelta"},"depth":3}}"#,
            r#"{"path":{"from":{"name":"a"},"to":{"name":"b"}}}"#,
            r#"{"show_only":{"lang":"rust","community":null}}"#,
            r#""zoom_out""#,
            r#""overview""#,
        ];
        for ex in examples {
            assert!(LOOM_SYSTEM.contains(ex), "prompt lists {ex}");
            let v = json!({ "command": serde_json::from_str::<serde_json::Value>(ex).unwrap(), "answer": "" });
            let (cmd, _) = command_from_llm(&v).unwrap_or_else(|e| panic!("{ex}: {e}"));
            assert!(cmd.is_some(), "{ex}");
        }
    }

    #[test]
    fn ontology_refs_route_lists_every_bound_annotation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        // Single-line text, so Codescape never reads this test as an annotation.
        std::fs::write(
            dir.path().join("src/a.rs"),
            "// @ontology urn:ngm:class:sparql\n\n/// @ontology urn:ngm:class:bm25 urn:ngm:class:embedding\nfn f() {}\n",
        )
        .unwrap();
        let mut f = n(2, NodeKind::File, Tier::L1, 1);
        f.span = None;
        let mut sym = n(3, NodeKind::Function, Tier::L2, 2);
        sym.span = Some(codescape_model::Span {
            start_line: 4,
            end_line: 4,
        });
        let s = GraphSnapshot {
            repo: Arc::from("t"),
            epoch: 1,
            revision: 1,
            generated_at_ms: 0,
            nodes: vec![n(1, NodeKind::Directory, Tier::L0, 0), f, sym],
            edges: vec![],
        };
        let table = AnnotationTable::scan(dir.path(), &s);
        let body = ontology_refs_body(&s, &table);
        assert_eq!(
            body,
            json!([
                {"key": "k2", "id": 2, "path": "src/a.rs", "line": 1, "iri": "urn:ngm:class:sparql"},
                {"key": "k3", "id": 3, "path": "src/a.rs", "line": 3, "iri": "urn:ngm:class:bm25"},
                {"key": "k3", "id": 3, "path": "src/a.rs", "line": 3, "iri": "urn:ngm:class:embedding"},
            ])
        );
        // The stream carries the roll-up on the file and the symbol's own refs.
        assert_eq!(
            table.refs_for(2),
            Some("urn:ngm:class:bm25#L3 urn:ngm:class:embedding#L3 urn:ngm:class:sparql#L1")
        );
        assert_eq!(
            table.refs_for(3),
            Some("urn:ngm:class:bm25#L3 urn:ngm:class:embedding#L3")
        );
        assert_eq!(table.refs_for(1), None);
    }

    #[test]
    fn snapshot_projection_keeps_tier_and_focus_symbols_only() {
        let s = GraphSnapshot {
            repo: Arc::from("t"),
            epoch: 1,
            revision: 3,
            generated_at_ms: 0,
            nodes: vec![
                n(1, NodeKind::Directory, Tier::L0, 0),
                n(2, NodeKind::File, Tier::L1, 1),
                n(3, NodeKind::Function, Tier::L2, 2),
                n(4, NodeKind::File, Tier::L1, 1),
                n(5, NodeKind::Function, Tier::L2, 4),
            ],
            edges: vec![
                CodeEdge {
                    source: 2,
                    target: 3,
                    relation: Relation::Contains,
                    weight: 1.0,
                    confidence: Confidence::Extracted,
                },
                CodeEdge {
                    source: 2,
                    target: 4,
                    relation: Relation::Imports,
                    weight: 1.0,
                    confidence: Confidence::Extracted,
                },
            ],
        };
        let l1 = project_snapshot(&s, Tier::L1, &[]);
        assert_eq!(
            l1.nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![1, 2, 4]
        );
        assert_eq!(l1.edges.len(), 1);
        let focus = project_snapshot(&s, Tier::L1, &[2]);
        assert_eq!(
            focus.nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(focus.edges.len(), 2);
        assert_eq!(project_snapshot(&s, Tier::L0, &[]).nodes.len(), 1);
        assert_eq!(parse_ids(Some("1, 2,x,3")), vec![1, 2, 3]);
    }
}
