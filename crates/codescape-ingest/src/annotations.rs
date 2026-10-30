//! `@ontology` annotations: code-side claims that a symbol implements a
//! knowledge-graph or ontology concept (VisionClaw ADR-2164 D1, D2).
//!
//! # Grammar (D1)
//!
//! An annotation is a whole comment line. After the comment leader (one of
//! `//`, `///`, `//!`, `#`, `--`, `;`, `*`, `/*` or `<!--`) and optional
//! whitespace, the line holds the token `@ontology` followed by one to eight
//! IRIs, separated by whitespace. A trailing `*/` or `-->` is stripped. Each
//! IRI must match `urn:ngm:class:[a-z0-9-]{1,128}` or
//! `urn:visionflow:page:[a-z0-9-]{1,128}`; any other token is skipped, a
//! repeated IRI counts once, and IRIs past the eighth are dropped. A code line
//! with a trailing comment (`x(); // @ontology …`) is not an annotation, and a
//! bare IRI without the marker never links.
//!
//! Codescape checks only the shape. It never resolves or validates an IRI:
//! to Codescape it is an opaque string, and the XR client joins it against
//! the knowledge stream.
//!
//! # Binding and roll-up (D2)
//!
//! - An annotation inside a contiguous comment block that sits directly above
//!   a symbol binds to that symbol. Attribute and decorator lines (`#[…]`,
//!   `#![…]`, `@decorator`) between the block and the symbol are allowed,
//!   because extractors start a symbol's span at its name line.
//! - Otherwise it binds to the innermost symbol whose span contains the line,
//!   and to the file node at worst.
//! - A file node's refs are the union of its own and its symbols'
//!   annotations. Directory nodes and containing symbols carry none of their
//!   children's.
//!
//! # Storage
//!
//! [`AnnotationTable`] is a side table: the parsed lines per file (re-read
//! only for changed paths) plus their binding against the current snapshot
//! (redone on every publish, because spans move). [`codescape_model::CodeNode`],
//! the persisted snapshot and the wire records are unchanged.
//!
//! # Example
//!
//! ```
//! use codescape_ingest::annotations::scan_text;
//!
//! let raw = scan_text("/// Ranks documents.\n/// @ontology urn:ngm:class:bm25\nfn rank() {}\n");
//! assert_eq!(raw.len(), 1);
//! assert_eq!(raw[0].line, 2);
//! assert_eq!(raw[0].iris, vec!["urn:ngm:class:bm25".to_string()]);
//! // The block ends on line 2, so a symbol starting on line 3 takes it.
//! assert_eq!((raw[0].block_end, raw[0].anchor_end), (2, 3));
//! ```

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use codescape_model::{GraphSnapshot, NodeKind, Span};
use rayon::prelude::*;

/// The marker token that turns a comment line into an annotation.
pub const MARKER: &str = "@ontology";
/// Most IRIs one annotation line may carry; later ones are dropped.
pub const MAX_IRIS_PER_LINE: usize = 8;
/// Most refs one node carries on the wire (`metadata.ontology_refs`).
pub const MAX_REFS_PER_NODE: usize = 32;
/// Longest IRI local part accepted.
pub const MAX_LOCAL_LEN: usize = 128;
/// Files larger than this are not scanned (generated or vendored blobs).
pub const MAX_SCAN_BYTES: u64 = 4 * 1024 * 1024;

/// The two IRI namespaces an annotation may name.
const IRI_PREFIXES: &[&str] = &["urn:ngm:class:", "urn:visionflow:page:"];
/// Comment leaders, longest first so `///` is not read as `//` plus `/`.
const LEADERS: &[&str] = &["<!--", "///", "//!", "//", "/*", "--", "#", ";", "*"];

/// One `@ontology` line parsed from a file, before binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAnnotation {
    /// 1-based line of the annotation.
    pub line: u32,
    /// The IRIs it names, in order, de-duplicated, at most [`MAX_IRIS_PER_LINE`].
    pub iris: Vec<String>,
    /// Last line of the contiguous comment block holding the annotation.
    pub block_end: u32,
    /// Last line a documented symbol may start on: `block_end + 1`, pushed
    /// down over attribute and decorator lines.
    pub anchor_end: u32,
}

/// One annotation bound to a node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Annotation {
    /// Repository-relative path of the file holding the line.
    pub path: Arc<str>,
    /// 1-based line of the annotation.
    pub line: u32,
    /// The IRI.
    pub iri: Arc<str>,
    /// Interned id of the node it binds to (a symbol, or the file).
    pub node: u32,
}

/// Whether `tok` is an IRI an annotation may name.
pub fn is_annotation_iri(tok: &str) -> bool {
    IRI_PREFIXES.iter().any(|p| {
        tok.strip_prefix(p).is_some_and(|local| {
            (1..=MAX_LOCAL_LEN).contains(&local.len())
                && local
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    })
}

/// An attribute or decorator line that may sit between a doc block and its symbol.
fn is_attribute(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("#[")
        || t.starts_with("#![")
        || t.strip_prefix('@')
            .and_then(|r| r.chars().next())
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// The text after the comment leader when `line` is a whole-line comment.
fn comment_body(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with("#[") || t.starts_with("#![") {
        return None;
    }
    LEADERS.iter().find_map(|l| t.strip_prefix(l))
}

/// The IRIs of an annotation line, or `None` when the line is not one.
fn parse_line(line: &str) -> Option<Vec<String>> {
    let body = comment_body(line)?.trim();
    let rest = body.strip_prefix(MARKER)?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None; // `@ontologyX` is another token
    }
    let mut rest = rest.trim_end();
    for close in ["*/", "-->"] {
        if let Some(r) = rest.strip_suffix(close) {
            rest = r.trim_end();
        }
    }
    let mut iris: Vec<String> = Vec::new();
    for tok in rest.split_whitespace() {
        if iris.len() == MAX_IRIS_PER_LINE {
            break;
        }
        if is_annotation_iri(tok) && !iris.iter().any(|i| i == tok) {
            iris.push(tok.to_string());
        }
    }
    (!iris.is_empty()).then_some(iris)
}

/// Every annotation line in `text`, with the block geometry binding needs.
pub fn scan_text(text: &str) -> Vec<RawAnnotation> {
    if !text.contains(MARKER) {
        return Vec::new();
    }
    let lines: Vec<&str> = text.lines().collect();
    let is_comment: Vec<bool> = lines.iter().map(|l| comment_body(l).is_some()).collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(iris) = parse_line(line) else {
            continue;
        };
        let mut end = i;
        while end + 1 < lines.len() && is_comment[end + 1] {
            end += 1;
        }
        let mut anchor = end + 1;
        while anchor < lines.len() && is_attribute(lines[anchor]) {
            anchor += 1;
        }
        out.push(RawAnnotation {
            line: i as u32 + 1,
            iris,
            block_end: end as u32 + 1,
            anchor_end: anchor as u32 + 1,
        });
    }
    out
}

/// Bind the annotations of one file. `symbols` are the file's symbol nodes
/// with their spans; `file` is the file node, if the snapshot has one. An
/// annotation that finds neither a symbol nor a file node is dropped.
pub fn bind(
    path: &Arc<str>,
    raw: &[RawAnnotation],
    file: Option<u32>,
    symbols: &[(u32, Span)],
) -> Vec<Annotation> {
    // Smallest span first, then lowest id, so "innermost" is the first match.
    let narrowest = |cands: &mut dyn Iterator<Item = &(u32, Span)>| -> Option<u32> {
        cands
            .min_by_key(|(id, s)| (s.end_line.saturating_sub(s.start_line), *id))
            .map(|(id, _)| *id)
    };
    let mut out = Vec::new();
    for r in raw {
        let documented = narrowest(
            &mut symbols
                .iter()
                .filter(|(_, s)| s.start_line > r.block_end && s.start_line <= r.anchor_end),
        );
        let node = documented
            .or_else(|| {
                narrowest(
                    &mut symbols
                        .iter()
                        .filter(|(_, s)| s.start_line <= r.line && r.line <= s.end_line),
                )
            })
            .or(file);
        let Some(node) = node else {
            continue;
        };
        for iri in &r.iris {
            out.push(Annotation {
                path: path.clone(),
                line: r.line,
                iri: Arc::from(iri.as_str()),
                node,
            });
        }
    }
    out
}

/// The wire value of `metadata.ontology_refs`: `<iri>#L<line>` tokens sorted by
/// IRI then line, de-duplicated, at most [`MAX_REFS_PER_NODE`], space-separated.
pub fn format_refs<'a>(refs: impl IntoIterator<Item = (&'a str, u32)>) -> String {
    let set: BTreeSet<(&str, u32)> = refs.into_iter().collect();
    set.into_iter()
        .take(MAX_REFS_PER_NODE)
        .map(|(iri, line)| format!("{iri}#L{line}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Read and scan one repository file. Unreadable, oversized or non-UTF-8
/// files have no annotations.
fn scan_file(root: &Path, rel: &str) -> Vec<RawAnnotation> {
    let abs = root.join(rel);
    match std::fs::metadata(&abs) {
        Ok(m) if m.is_file() && m.len() <= MAX_SCAN_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read(&abs)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|t| scan_text(&t))
        .unwrap_or_default()
}

/// The annotation side table of one repository.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotationTable {
    raw: HashMap<Arc<str>, Vec<RawAnnotation>>,
    bound: Vec<Annotation>,
    refs: HashMap<u32, String>,
}

impl AnnotationTable {
    /// Scan every file node of `snapshot` under `root` and bind the result.
    pub fn scan(root: &Path, snapshot: &GraphSnapshot) -> Self {
        let files: Vec<Arc<str>> = snapshot
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::File)
            .filter_map(|n| n.path.clone())
            .collect();
        let raw: HashMap<Arc<str>, Vec<RawAnnotation>> = files
            .into_par_iter()
            .filter_map(|p| {
                let r = scan_file(root, &p);
                (!r.is_empty()).then_some((p, r))
            })
            .collect();
        let mut t = Self {
            raw,
            ..Self::default()
        };
        t.rebind(snapshot);
        t
    }

    /// Re-read `paths` (repository-relative) under `root`, then rebind every
    /// file against `snapshot`. Returns whether the bound refs changed.
    pub fn refresh<S: AsRef<str>>(
        &mut self,
        root: &Path,
        snapshot: &GraphSnapshot,
        paths: &[S],
    ) -> bool {
        for p in paths {
            let p = p.as_ref();
            let r = scan_file(root, p);
            if r.is_empty() {
                self.raw.remove(p);
            } else {
                self.raw.insert(Arc::from(p), r);
            }
        }
        self.rebind(snapshot)
    }

    /// Bind the parsed lines against `snapshot` (spans move with every edit).
    /// Returns whether the bound refs changed.
    pub fn rebind(&mut self, snapshot: &GraphSnapshot) -> bool {
        let mut files: HashMap<&str, u32> = HashMap::new();
        let mut symbols: HashMap<&str, Vec<(u32, Span)>> = HashMap::new();
        for n in &snapshot.nodes {
            let Some(p) = n.path.as_deref() else {
                continue;
            };
            if !self.raw.contains_key(p) {
                continue;
            }
            match (&n.kind, n.span) {
                (NodeKind::File, _) => {
                    files.insert(p, n.id);
                }
                (NodeKind::Directory | NodeKind::External | NodeKind::Document, _) => {}
                (_, Some(s)) => symbols.entry(p).or_default().push((n.id, s)),
                (_, None) => {}
            }
        }
        let mut bound: Vec<Annotation> = self
            .raw
            .iter()
            .flat_map(|(path, raw)| {
                bind(
                    path,
                    raw,
                    files.get(&**path).copied(),
                    symbols.get(&**path).map_or(&[][..], Vec::as_slice),
                )
            })
            .collect();
        bound.sort();
        bound.dedup();

        let mut per_node: HashMap<u32, Vec<(&str, u32)>> = HashMap::new();
        for a in &bound {
            per_node.entry(a.node).or_default().push((&a.iri, a.line));
            // Roll-up: the file node carries its symbols' annotations too.
            if let Some(&f) = files.get(&*a.path) {
                if f != a.node {
                    per_node.entry(f).or_default().push((&a.iri, a.line));
                }
            }
        }
        let refs: HashMap<u32, String> = per_node
            .into_iter()
            .map(|(id, r)| (id, format_refs(r)))
            .collect();
        let changed = refs != self.refs || bound != self.bound;
        self.bound = bound;
        self.refs = refs;
        changed
    }

    /// Every bound annotation, sorted by path, line, IRI and node.
    pub fn annotations(&self) -> &[Annotation] {
        &self.bound
    }

    /// `metadata.ontology_refs` per node id; nodes without refs are absent.
    pub fn ontology_refs(&self) -> &HashMap<u32, String> {
        &self.refs
    }

    /// `metadata.ontology_refs` of one node.
    pub fn refs_for(&self, id: u32) -> Option<&str> {
        self.refs.get(&id).map(String::as_str)
    }

    /// No annotation is bound.
    pub fn is_empty(&self) -> bool {
        self.bound.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use codescape_model::{CodeNode, Lang, Tier};

    use super::*;

    // Test texts are single-line strings with `\n` escapes, so Codescape
    // ingesting its own repository never reads them as real annotations.
    const A: &str = "urn:ngm:class:bm25";
    const B: &str = "urn:visionflow:page:did-nostr";

    #[test]
    fn iri_shape_is_checked_and_nothing_else() {
        assert!(is_annotation_iri(A));
        assert!(is_annotation_iri(B));
        assert!(is_annotation_iri(&format!(
            "urn:ngm:class:{}",
            "a".repeat(128)
        )));
        assert!(!is_annotation_iri(&format!(
            "urn:ngm:class:{}",
            "a".repeat(129)
        )));
        assert!(!is_annotation_iri("urn:ngm:class:"));
        assert!(!is_annotation_iri("urn:ngm:class:BM25"));
        assert!(!is_annotation_iri("urn:ngm:class:bm_25"));
        assert!(!is_annotation_iri("urn:ngm:property:bm25"));
        assert!(!is_annotation_iri("https://example.org/bm25"));
    }

    #[test]
    fn every_comment_leader_is_accepted() {
        for leader in ["//", "///", "//!", "#", "--", ";", "*", "/*", "<!--"] {
            let text = format!("  {leader} {MARKER} {A}\n");
            let raw = scan_text(&text);
            assert_eq!(raw.len(), 1, "leader {leader}");
            assert_eq!(raw[0].iris, vec![A.to_string()], "leader {leader}");
        }
        assert_eq!(scan_text(&format!("/* {MARKER} {A} */\n"))[0].iris, vec![A]);
        assert_eq!(
            scan_text(&format!("<!-- {MARKER} {A} -->\n"))[0].iris,
            vec![A]
        );
        assert_eq!(
            scan_text(&format!("//{MARKER} {A}\n")).len(),
            1,
            "no space after leader"
        );
    }

    #[test]
    fn malformed_bare_trailing_and_ninth_iris_are_ignored() {
        // A bare IRI never links.
        assert!(scan_text(&format!("// see {A}\n")).is_empty());
        assert!(scan_text(&format!("let s = \"{A}\";\n")).is_empty());
        // A code line with a trailing comment is not an annotation.
        assert!(scan_text(&format!("call(); // {MARKER} {A}\n")).is_empty());
        // `@ontologyX` is not the marker; the marker alone carries nothing.
        assert!(scan_text(&format!("// {MARKER}x {A}\n")).is_empty());
        assert!(scan_text(&format!("// {MARKER}\n")).is_empty());
        // `#[…]` is an attribute, not a `#` comment.
        assert!(scan_text(&format!("#[doc = \"{MARKER} {A}\"]\n")).is_empty());
        // A malformed token is skipped and the valid ones on the line kept.
        let raw = scan_text(&format!("// {MARKER} urn:ngm:class:Bad {A} prose {B}\n"));
        assert_eq!(raw[0].iris, vec![A.to_string(), B.to_string()]);
        // Repeats count once; the ninth distinct IRI is dropped.
        let nine: Vec<String> = (1..=9).map(|i| format!("urn:ngm:class:c{i}")).collect();
        let raw = scan_text(&format!("// {MARKER} {A} {A} {}\n", nine.join(" ")));
        let mut want = vec![A.to_string()];
        want.extend(nine[..7].iter().cloned());
        assert_eq!(raw[0].iris, want);
        assert_eq!(raw[0].iris.len(), MAX_IRIS_PER_LINE);
    }

    #[test]
    fn block_geometry_skips_attributes_and_stops_at_blank_lines() {
        // The visionclaw-xr-presence `Did` shape: doc block, derive, struct.
        let text = format!("use x;\n\n/// Nostr DID.\n/// {MARKER} {B}\n#[derive(Debug)]\n#[serde(transparent)]\npub struct Did(String);\n");
        let raw = scan_text(&text);
        assert_eq!(
            (raw[0].line, raw[0].block_end, raw[0].anchor_end),
            (4, 4, 7)
        );
        // Python decorator.
        let raw = scan_text(&format!("# {MARKER} {A}\n@cache\ndef rank():\n    pass\n"));
        assert_eq!((raw[0].block_end, raw[0].anchor_end), (1, 3));
        // A blank line ends the block: nothing is directly below.
        let raw = scan_text(&format!("// {MARKER} {A}\n\nfn f() {{}}\n"));
        assert_eq!((raw[0].block_end, raw[0].anchor_end), (1, 2));
    }

    fn span(a: u32, b: u32) -> Span {
        Span {
            start_line: a,
            end_line: b,
        }
    }

    #[test]
    fn binding_prefers_the_documented_symbol_then_the_innermost_container() {
        let path: Arc<str> = Arc::from("src/a.rs");
        // 1 impl Engine {        (symbol 10, lines 1-12)
        // 2   /// doc
        // 3   /// @ontology A    → documented method 11 (starts line 5, after an attribute)
        // 4   #[inline]
        // 5   fn run() {         (symbol 11, lines 5-9)
        // 6     // @ontology B   → innermost container 11
        // 9   }
        // 11  // @ontology A     → container 10 (nothing starts at 12)
        // 20 // @ontology B      → file 1
        let text = format!(
            "impl Engine {{\n  /// doc\n  /// {MARKER} {A}\n  #[inline]\n  fn run() {{\n    // {MARKER} {B}\n\n\n  }}\n\n  // {MARKER} {A}\n}}\n\n\n\n\n\n\n\n// {MARKER} {B}\n"
        );
        let raw = scan_text(&text);
        assert_eq!(
            raw.iter().map(|r| r.line).collect::<Vec<_>>(),
            vec![3, 6, 11, 20]
        );
        let symbols = [(10, span(1, 12)), (11, span(5, 9))];
        let bound = bind(&path, &raw, Some(1), &symbols);
        let got: Vec<(u32, &str, u32)> = bound.iter().map(|a| (a.line, &*a.iri, a.node)).collect();
        assert_eq!(got, vec![(3, A, 11), (6, B, 11), (11, A, 10), (20, B, 1)]);
        // Without a file node, the file-level annotation is dropped.
        let bound = bind(&path, &raw, None, &symbols);
        assert_eq!(bound.len(), 3);
    }

    #[test]
    fn refs_are_sorted_deduplicated_and_capped() {
        assert_eq!(
            format_refs([(B, 9), (A, 30), (A, 4), (A, 4)]),
            format!("{A}#L4 {A}#L30 {B}#L9")
        );
        let many: Vec<String> = (0..40).map(|i| format!("urn:ngm:class:c{i:02}")).collect();
        let s = format_refs(many.iter().map(|i| (i.as_str(), 1)));
        assert_eq!(s.split(' ').count(), MAX_REFS_PER_NODE);
    }

    fn node(id: u32, kind: NodeKind, path: &str, span: Option<Span>) -> CodeNode {
        CodeNode {
            id,
            key: Arc::from(format!("k{id}")),
            kind,
            lang: Lang::Rust,
            label: Arc::from(format!("n{id}")),
            path: Some(Arc::from(path)),
            span,
            parent: 0,
            community: 0,
            cluster: 0,
            importance: 0.0,
            tier: Tier::L1,
            size: 0,
        }
    }

    fn snapshot(nodes: Vec<CodeNode>) -> GraphSnapshot {
        GraphSnapshot {
            repo: Arc::from("t"),
            epoch: 1,
            revision: 1,
            generated_at_ms: 0,
            nodes,
            edges: vec![],
        }
    }

    #[test]
    fn table_rolls_up_to_the_file_and_never_to_directories_or_containers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/a.rs"),
            format!("// {MARKER} {B}\n\nimpl E {{\n  /// {MARKER} {A}\n  fn f() {{}}\n}}\n"),
        )
        .unwrap();
        std::fs::write(dir.path().join("src/b.rs"), "fn g() {}\n").unwrap();
        let snap = snapshot(vec![
            node(1, NodeKind::Directory, "src", None),
            node(2, NodeKind::File, "src/a.rs", None),
            node(3, NodeKind::Struct, "src/a.rs", Some(span(3, 6))),
            node(4, NodeKind::Method, "src/a.rs", Some(span(5, 5))),
            node(5, NodeKind::File, "src/b.rs", None),
        ]);
        let t = AnnotationTable::scan(dir.path(), &snap);
        assert_eq!(
            t.refs_for(2),
            Some(format!("{A}#L4 {B}#L1").as_str()),
            "file roll-up"
        );
        assert_eq!(
            t.refs_for(4),
            Some(format!("{A}#L4").as_str()),
            "documented method"
        );
        assert_eq!(t.refs_for(3), None, "a container does not roll up");
        assert_eq!(t.refs_for(1), None, "directories carry none");
        assert_eq!(t.refs_for(5), None, "unannotated file");
        assert_eq!(t.ontology_refs().len(), 2);
        assert_eq!(t.annotations().len(), 2);

        // Editing the file and refreshing only that path updates the table.
        let mut t2 = t.clone();
        std::fs::write(
            dir.path().join("src/b.rs"),
            format!("// {MARKER} {A}\nfn g() {{}}\n"),
        )
        .unwrap();
        assert!(t2.refresh(dir.path(), &snap, &["src/b.rs"]));
        assert_eq!(t2.refs_for(5), Some(format!("{A}#L1").as_str()));
        assert!(
            !t2.clone().refresh(dir.path(), &snap, &["src/b.rs"]),
            "unchanged"
        );
        // A deleted file loses its annotations.
        std::fs::remove_file(dir.path().join("src/a.rs")).unwrap();
        assert!(t2.refresh(dir.path(), &snap, &["src/a.rs"]));
        assert_eq!(t2.refs_for(2), None);
        assert_eq!(t2.refs_for(4), None);
    }

    /// L1 against real extractor spans: graphicle starts `Did` on its name
    /// line, after the derive, and the doc-block annotation still binds to it.
    #[test]
    fn binds_against_graphicle_spans() {
        let root = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("src/lib.rs"),
            format!("//! Crate doc.\n//! {MARKER} {A}\n\n/// Nostr DID.\n/// {MARKER} {B}\n#[derive(Debug, Clone)]\npub struct Did(String);\n\nimpl Did {{\n    pub fn hex(&self) -> &str {{\n        // {MARKER} urn:ngm:class:sha-256\n        &self.0\n    }}\n}}\n"),
        )
        .unwrap();
        let cfg = crate::IngestConfig::new("t", root.path(), out.path());
        let (ing, _) = crate::Ingestor::open(cfg, codescape_model::IdInterner::new(1)).unwrap();
        let snap = ing.snapshot();
        let id_of = |kind: NodeKind, label: &str| {
            snap.nodes
                .iter()
                .find(|n| n.kind == kind && &*n.label == label)
                .unwrap_or_else(|| panic!("{label} extracted"))
                .id
        };
        let did = id_of(NodeKind::Struct, "Did");
        let hex = snap
            .nodes
            .iter()
            .find(|n| &*n.label == "hex" && n.span.is_some())
            .expect("hex extracted")
            .id;
        let file = snap
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::File)
            .unwrap()
            .id;
        let t = AnnotationTable::scan(root.path(), snap);
        assert_eq!(
            t.refs_for(did),
            Some(format!("{B}#L5").as_str()),
            "documented struct"
        );
        assert_eq!(
            t.refs_for(hex),
            Some("urn:ngm:class:sha-256#L11"),
            "innermost container"
        );
        assert_eq!(
            t.refs_for(file),
            Some(format!("{A}#L2 urn:ngm:class:sha-256#L11 {B}#L5").as_str()),
            "module doc binds to the file, which rolls up the rest"
        );
    }

    #[test]
    fn rebind_follows_moved_spans_without_rereading() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.rs"),
            format!("/// {MARKER} {A}\nfn f() {{}}\n"),
        )
        .unwrap();
        let mut t = AnnotationTable::scan(
            dir.path(),
            &snapshot(vec![
                node(1, NodeKind::File, "a.rs", None),
                node(2, NodeKind::Function, "a.rs", Some(span(2, 2))),
            ]),
        );
        assert_eq!(t.refs_for(2), Some(format!("{A}#L1").as_str()));
        // The symbol was re-interned (new id) by a later snapshot.
        assert!(t.rebind(&snapshot(vec![
            node(1, NodeKind::File, "a.rs", None),
            node(7, NodeKind::Function, "a.rs", Some(span(2, 2))),
        ])));
        assert_eq!(t.refs_for(2), None);
        assert_eq!(t.refs_for(7), Some(format!("{A}#L1").as_str()));
    }
}
