# Changelog

All notable changes to this repository are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); crates follow semantic versioning once published.

## [Unreleased]

### Added
- Repository skeleton: README, AGENTS.md and CLAUDE.md, the governing document `docs/BASELINE-codescape.md`, the ADR ledger with ADR-0001 (extraction from VisionClaw and the licence split), the deployment contract, licence texts and a workspace stub.

### Planned
- Import of the five crates and the desktop client from VisionClaw `feat/codescape` (from `b63068e3d`).
- `codescape-model` depends on `visionclaw-wire` from crates.io instead of a `#[path]` include.
- `codescape-layout` loads PTX from `CODESCAPE_PTX_DIR` at runtime instead of linking `visionclaw-gpu`.
- `codescape-model` and `codescape-collab` relicensed `MIT OR Apache-2.0` and prepared for crates.io.
