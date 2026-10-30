# Changelog

All notable changes to this repository are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); crates follow semantic versioning once published.

## [Unreleased]

### Added
- `@ontology <iri>` code annotations (VisionClaw ADR-2164 D1-D3): `codescape-ingest::annotations` parses whole-line comment annotations (one to eight `urn:ngm:class:` or `urn:visionflow:page:` IRIs, shape-checked only), binds each to the symbol its comment block documents (attribute and decorator lines allowed between), else to the innermost containing symbol, else to the file, and rolls them up to the file node. The server keeps the table inside each publication, re-reads the delta's changed paths (a comment edit arrives as a delta: verified live, the refs updated within 2 s and bound to the documented method), defensively re-reads a batch's paths when it yields no delta, and rebinds on every publish. The XR `initialGraphLoad` adds `metadata.ontology_refs` (`<iri>#L<line>` sorted by IRI then line, at most 32, omitted when empty) on annotated nodes, and `GET /api/graph/ontology-refs?repo=` lists `[{key, id, path, line, iri}]`. No new dependency; `Cargo.lock` unchanged.
- Repository skeleton: README, AGENTS.md and CLAUDE.md, the governing document `docs/BASELINE-codescape.md`, the ADR ledger with ADR-0001 (extraction from VisionClaw and the licence split), the deployment contract, licence texts and a workspace stub.
- Import of the five crates, the desktop client and the deploy entrypoint from VisionClaw `cac36a028` (on `main`, source range `b63068e3d^..cac36a028`; ADR-0001). Paths map `crates/codescape/<crate>/` to `crates/<crate>/`, `client/src/codescape/` to `client/` and `scripts/codescape-entrypoint.sh` to `deploy/codescape-entrypoint.sh`.
- `codescape-model` depends on `visionclaw-wire` 0.1.0 from crates.io; the committed `Cargo.lock` carries its registry checksum.
- Isolation test I10, `no_dependency_comes_from_the_visionclaw_repository`: no `git+` source naming VisionClaw and no path package outside this repository, in both feature sets. I1a and I11 arrive tested.
- The I1 negative control uses an in-repo fixture package (`crates/codescape-server/tests/fixtures/forbidden/visionclaw-ontology`, excluded from the workspace), so it always runs.
- `client/src/__tests__/fixtures/v5_frame.bin`, written by `visionclaw_wire::v5_frame` and pinned by `crates/codescape-server/tests/wire_fixture.rs`; `protocol.test.ts` decodes every field of it. This replaces the cross-check against VisionClaw's `binaryProtocol.ts`, and the client isolation test has no exception left.
- `client/package.json` and `package-lock.json`: the client stands alone, with direct dependencies pinned to the exact versions VisionClaw's client resolved at import.
- CI: fmt, clippy, tests and rustdoc with and without the `cuda` feature (the CUDA job compiles and links in a CUDA devel container, no device), the publish dry-run of the permissive crates, and the client typecheck, tests and build.
- `docs/evidence/import-gate.txt`: raw output of the import gate.

### Changed
- `deploy/codescape-entrypoint.sh` builds from a read-only checkout at `${CODESCAPE_SRC:-/app/codescape}`, takes the GPU sources from `${CODESCAPE_GPU_SRC:-/app/crates/visionclaw-gpu}`, and builds the client from a writable copy whose `node_modules` falls back to `${CODESCAPE_NODE_MODULES:-/app/client/node_modules}`. Vite loads its config with `--configLoader runner` and caches under the copy, so nothing is written into a shared `node_modules`.
- The client's Vite cache moves from `node_modules/.vite` to `client/.vite` (override `CODESCAPE_VITE_CACHE`); the bundle is written to `target/codescape-dist` (override `CODESCAPE_DIST`).
- The `dogfood` ingest smoke test reads this repository's own `crates/`.
