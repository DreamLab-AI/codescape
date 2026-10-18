//! Isolation from the knowledge-work system is a tested property
//! (docs/BASELINE-codescape.md, invariants I1, I1a, I8, I10, I11).
//!
//! The Codescape build graph must never contain the ontology, memory-cloud,
//! Oxigraph, RuVector/Postgres, Solid pod or VisionClaw server crates. This test
//! runs `cargo metadata --locked` on the Codescape workspace and fails on any of
//! them. The only estate crate allowed is `visionclaw-wire` (the
//! dependency-free position wire, from crates.io); the CUDA PTX is a runtime
//! artefact, so no other `visionclaw-*` crate may appear in either feature set,
//! and no package may come from the VisionClaw repository by git or by a path
//! outside this repository.

use std::path::PathBuf;
use std::process::Command;

const FORBIDDEN: &[&str] = &[
    "visionclaw-server",
    "visionclaw-ontology",
    "visionclaw-memory-cloud",
    "visionclaw-adapters",
    "visionclaw-actors",
    "visionclaw-contracts",
    "vault",
    "vault-core",
    "whelk",
    "whelk-rs",
    "oxigraph",
    "horned-owl",
    "tokio-postgres",
    "postgres",
    "sqlx",
    "deadpool-postgres",
    "solid-pod-rs",
    "solid-pod-rs-core",
];

const FORBIDDEN_PREFIXES: &[&str] = &["ruvector", "solid-pod-rs"];

/// The repository root: `crates/codescape-server` is two levels down.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .unwrap()
        .to_path_buf()
}

fn package_names(manifest_dir: &std::path::Path, extra_args: &[&str]) -> Vec<String> {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--offline")
        .current_dir(manifest_dir);
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("cargo metadata runs");
    if !out.status.success() {
        // Offline may fail on a cold cache; retry online once, but keep the
        // offline diagnostic visible so a network-dependent gate is noticed.
        eprintln!(
            "isolation: offline cargo metadata failed, retrying online:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut cmd = Command::new(env!("CARGO"));
        cmd.arg("metadata")
            .arg("--format-version")
            .arg("1")
            .current_dir(manifest_dir);
        for a in extra_args {
            cmd.arg(a);
        }
        let out = cmd.output().expect("cargo metadata runs");
        assert!(
            out.status.success(),
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return names_from(&out.stdout);
    }
    names_from(&out.stdout)
}

fn names_from(stdout: &[u8]) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_slice(stdout).expect("metadata is json");
    v["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect()
}

fn violations(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|n| {
            FORBIDDEN.contains(&n.as_str()) || FORBIDDEN_PREFIXES.iter().any(|p| n.starts_with(p))
        })
        .cloned()
        .collect()
}

#[test]
fn build_graph_contains_no_knowledge_work_crates() {
    let names = package_names(&workspace_root(), &["--locked"]);
    assert!(
        names.iter().any(|n| n == "codescape-server"),
        "metadata covered the Codescape workspace"
    );
    assert!(names.iter().any(|n| n == "graphicle"), "graphicle resolved");
    let bad = violations(&names);
    assert!(
        bad.is_empty(),
        "forbidden crates in the Codescape build graph: {bad:?}"
    );
}

#[test]
fn cuda_feature_graph_contains_no_knowledge_work_crates() {
    // Compose builds with `--features cuda`. Since ADR-0001 the CUDA PTX is a
    // runtime artefact (CODESCAPE_PTX_DIR), so the feature adds only `cust`: no
    // VisionClaw crate may enter the graph through it. `cargo metadata` resolves
    // features without compiling, so this runs on hosts without CUDA.
    let names = package_names(&workspace_root(), &["--locked", "--all-features"]);
    assert!(
        !names.iter().any(|n| n == "visionclaw-gpu"),
        "visionclaw-gpu must not be in the cuda build graph (PTX is loaded at runtime)"
    );
    let bad = violations(&names);
    assert!(
        bad.is_empty(),
        "forbidden crates in the Codescape cuda build graph: {bad:?}"
    );
}

#[test]
fn visionclaw_wire_is_the_only_visionclaw_crate() {
    // Codescape I1a (ADR-0001): the frozen position wire is the one estate
    // crate Codescape may depend on, in both feature sets. Anything else named
    // `visionclaw-*` would re-couple the extracted repository to VisionClaw.
    for extra in [&["--locked"][..], &["--locked", "--all-features"][..]] {
        let names = package_names(&workspace_root(), extra);
        let estate: Vec<&String> = names
            .iter()
            .filter(|n| n.starts_with("visionclaw-") && n.as_str() != "visionclaw-wire")
            .collect();
        assert!(
            estate.is_empty(),
            "only visionclaw-wire may be a dependency ({extra:?}), found {estate:?}"
        );
        assert!(
            names.iter().any(|n| n == "visionclaw-wire"),
            "visionclaw-wire resolved ({extra:?})"
        );
    }
}

#[test]
fn the_check_itself_catches_an_injected_dependency() {
    // Build a throwaway workspace that depends by path on a fixture package
    // named like the ontology crate, and prove the filter flags it. This guards
    // the test against silently passing because the ban list stopped matching.
    // The fixture lives in this repository (excluded from the workspace), so
    // the control always runs; a missing fixture is a failure, not a pass.
    let tmp = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/forbidden/visionclaw-ontology");
    assert!(
        fixture.join("Cargo.toml").exists(),
        "negative-control fixture missing: {}",
        fixture.display()
    );
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        format!(
            r#"[package]
name = "injected"
version = "0.0.0"
edition = "2021"
[dependencies]
visionclaw-ontology = {{ path = "{}" }}
[workspace]
"#,
            fixture.display()
        ),
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    let names = package_names(tmp.path(), &[]);
    let bad = violations(&names);
    assert!(
        bad.iter().any(|n| n == "visionclaw-ontology"),
        "injected dependency must be flagged, got {bad:?}"
    );
    let estate: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("visionclaw-") && n.as_str() != "visionclaw-wire")
        .collect();
    assert!(
        !estate.is_empty(),
        "the I1a prefix filter must flag the injected crate too"
    );
}

/// The VisionClaw repository under any of its names (I10).
fn names_visionclaw_repo(source: &str) -> bool {
    let s = source.to_ascii_lowercase();
    s.contains("visionclaw") || s.contains("ar-ai-knowledge-graph")
}

#[test]
fn no_dependency_comes_from_the_visionclaw_repository() {
    // Codescape I10: no package is fetched from the VisionClaw repository by
    // git (its history is about 72 GB and its workspace would leak into this
    // lock), and no path package lives outside this repository (a regression
    // to `path = "../visionclaw-wire"` or a sibling checkout).
    let root = workspace_root()
        .canonicalize()
        .expect("workspace root resolves");
    for extra in [&["--locked"][..], &["--locked", "--all-features"][..]] {
        let m = metadata(extra);
        let mut path_packages = 0;
        for p in m["packages"].as_array().unwrap() {
            let name = p["name"].as_str().unwrap();
            match p["source"].as_str() {
                Some(src) => assert!(
                    !(src.starts_with("git+") && names_visionclaw_repo(src)),
                    "{name} is a git dependency on the VisionClaw repository: {src} [{extra:?}]"
                ),
                None => {
                    path_packages += 1;
                    let manifest = PathBuf::from(p["manifest_path"].as_str().unwrap())
                        .canonicalize()
                        .expect("path package manifest resolves");
                    assert!(
                        manifest.starts_with(&root),
                        "{name} is a path dependency outside this repository: {} [{extra:?}]",
                        manifest.display()
                    );
                }
            }
        }
        assert_eq!(
            path_packages, 5,
            "exactly the five workspace members are path packages [{extra:?}]"
        );
    }
}

#[test]
fn the_visionclaw_repo_matcher_catches_known_urls() {
    // Negative control for I10's matcher, over the URL forms cargo records.
    for bad in [
        "git+https://github.com/DreamLab-AI/VisionClaw?rev=abc#abc",
        "git+https://github.com/DreamLab-AI/AR-AI-Knowledge-Graph#0123",
        "git+ssh://git@github.com/dreamlab-ai/visionclaw.git?branch=main#0",
    ] {
        assert!(names_visionclaw_repo(bad), "{bad}");
    }
    assert!(!names_visionclaw_repo(
        "git+https://github.com/Graphicle-AI/graphicle?rev=003f93605d895ed69f0cf06690dd296474385b99#003f9360"
    ));
    assert!(!names_visionclaw_repo(
        "registry+https://github.com/rust-lang/crates.io-index"
    ));
}

#[test]
fn server_refuses_to_boot_with_a_ruvector_connection_string() {
    // The agentbox environment sets RUVECTOR_PG_CONNINFO for every process, so
    // the property under test is the binary's refusal, not the test's own env.
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_codescape-server"))
        .env(
            "RUVECTOR_PG_CONNINFO",
            "host=ruvector-postgres dbname=ruvector",
        )
        .env("CODESCAPE_DATA_DIR", tmp.path())
        .env("CODESCAPE_PORT", "0")
        .env("CODESCAPE_INGEST_PORT", "0")
        .arg("--repo")
        .arg(format!(
            "model={}",
            workspace_root().join("crates/codescape-model").display()
        ))
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "exit 2 expected; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("RUVECTOR_PG_CONNINFO"),
        "refusal must name the variable: {stderr}"
    );
}

/// Full `cargo metadata` JSON for the workspace (with the resolve graph).
fn metadata(extra_args: &[&str]) -> serde_json::Value {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("metadata")
        .arg("--format-version")
        .arg("1")
        .current_dir(workspace_root());
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("metadata is json")
}

#[test]
fn no_permissive_crate_depends_on_an_agpl_crate() {
    // Codescape I11 (ADR-0001). codescape-model, codescape-collab and
    // visionclaw-wire are MIT OR Apache-2.0 and published; nothing they ship
    // with (normal and build dependencies, transitively) may be AGPL, or the
    // permissive licence would be a fiction. Dev-dependencies do not ship and
    // are excluded.
    for extra in [&["--locked"][..], &["--locked", "--all-features"][..]] {
        let m = metadata(extra);
        let packages = m["packages"].as_array().unwrap();
        let license_of = |id: &str| -> String {
            packages
                .iter()
                .find(|p| p["id"] == id)
                .and_then(|p| p["license"].as_str())
                .unwrap_or("")
                .to_string()
        };
        let name_of = |id: &str| -> String {
            packages
                .iter()
                .find(|p| p["id"] == id)
                .and_then(|p| p["name"].as_str())
                .unwrap_or(id)
                .to_string()
        };
        let nodes = m["resolve"]["nodes"].as_array().unwrap();
        let shipped_deps = |id: &str| -> Vec<String> {
            nodes
                .iter()
                .find(|n| n["id"] == id)
                .map(|n| {
                    n["deps"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|d| {
                            d["dep_kinds"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|k| k["kind"].is_null() || k["kind"] == "build")
                        })
                        .map(|d| d["pkg"].as_str().unwrap().to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        let members: Vec<String> = m["workspace_members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let mut checked = 0;
        for member in &members {
            let lic = license_of(member);
            if lic.contains("AGPL") {
                continue;
            }
            checked += 1;
            let mut stack = vec![member.clone()];
            let mut seen = std::collections::HashSet::new();
            while let Some(id) = stack.pop() {
                if !seen.insert(id.clone()) {
                    continue;
                }
                for dep in shipped_deps(&id) {
                    let dl = license_of(&dep);
                    assert!(
                        !dl.contains("AGPL") && !dl.contains("GPL-3.0"),
                        "{} ({lic}) ships with {} ({dl}) [{extra:?}]",
                        name_of(member),
                        name_of(&dep)
                    );
                    stack.push(dep);
                }
            }
        }
        assert!(
            checked >= 2,
            "codescape-model and codescape-collab are permissive members, saw {checked}"
        );
    }
}
