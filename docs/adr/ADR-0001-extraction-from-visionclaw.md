---
id: ADR-0001
title: Codescape is extracted from VisionClaw into its own repository with a per-crate licence split
date: 2026-10-18
decision_status: accepted
implementation_status: partial
activation_status: inactive
supersedes: []
superseded_by: []
verified_commit:
verified_paths: []
owner: jjohare
review_trigger: the code import landing; visionclaw-wire's first publication; a proposal to depend on any VisionClaw crate other than visionclaw-wire; a licence change to any crate
repo: codescape
---

# ADR-0001: Codescape is extracted from VisionClaw into its own repository with a per-crate licence split

## Context
- Codescape was built inside VisionClaw under [ADR-2143](https://github.com/DreamLab-AI/VisionClaw/blob/feat/codescape/docs/adr/ADR-2143-codescape-section-separate-workspace-and-port.md) (amendments A1 to A4), on branch `feat/codescape` from `b63068e3d`. It already had its own Cargo workspace, lock, port and a tested isolation boundary from the knowledge-work system.
- Two couplings tied it to VisionClaw's tree: `codescape-model/src/wire.rs:18-19` includes VisionClaw's `visionclaw-protocol/src/wire_fixtures.rs` by `#[path]`, and `codescape-layout`'s `cuda` feature depends on `visionclaw-gpu` by path for build-compiled PTX.
- In VisionClaw every Codescape crate was `AGPL-3.0-only` (workspace default). The I/O-free crates `codescape-model` and `codescape-collab` contain nothing estate-specific and are useful on their own.
- VisionClaw's repository is about 72 GB with its history; Codescape's code is a small fraction of it.

## Decision
1. **Own repository.** Codescape lives in `DreamLab-AI/codescape` with a fresh history. The first import commit cites the VisionClaw range (`b63068e3d^..098d3ff98` on `feat/codescape`, matching ADR-2143 A5) it came from. Paths map `crates/codescape/<crate>/` to `crates/<crate>/` and `client/src/codescape/` to `client/`.
2. **Licence split.** The copyright holder relicenses at extraction:
   - `codescape-model`, `codescape-collab`: `MIT OR Apache-2.0`, publishable to crates.io.
   - `codescape-ingest`, `codescape-layout`, `codescape-server`, `client/`, and everything else: `AGPL-3.0-only`.
   - Nothing permissively licensed may depend on anything AGPL (BASELINE I11).
3. **Wire coupling.** VisionClaw publishes a new crate, `visionclaw-wire` (`MIT OR Apache-2.0`), holding the frozen 52-byte V3 record, the V5 envelope, `0x23`, `0x34` and their fixtures. `codescape-model` depends on it from crates.io instead of a `#[path]` include. It is the only `visionclaw-*` crate Codescape may depend on (BASELINE I1a). No crate takes a git dependency on the VisionClaw repository (BASELINE I10).
4. **GPU coupling.** `codescape-layout` no longer links `visionclaw-gpu`. The CUDA engine loads the unified PTX and its `ptx-build-manifest.txt` at runtime from `CODESCAPE_PTX_DIR` and applies the existing checks unchanged: `c_params` 212 bytes, the launched kernels' arities, `provenance=compiled` and a matching content tag (BASELINE I7). The content-tag function (FNV-1a 64) is copied byte-exactly from `visionclaw-gpu` and pinned by test vectors, and the directory must hold the PTX exactly as the build wrote it (after its rewrite), since the tag is checked against the manifest's `rewritten` field. Without a valid PTX it falls back to the CPU engine and says why in `/api/health`.
5. **graphicle** stays a pinned git dependency at rev `003f93605d895ed69f0cf06690dd296474385b99`.
6. **Deployment stays in VisionClaw.** VisionClaw mounts this repository as a git submodule at `codescape/` and keeps the `codescape` compose service and the dev nginx `/codescape/` location. This repository documents that contract in `deploy/README.md` and ships no compose file of its own.

## Invariants
- ADR-2143's invariants I1 to I9 carry over unchanged in substance, re-homed in `docs/BASELINE-codescape.md`.
- I1a (new): `visionclaw-wire` is the only `visionclaw-*` dependency allowed, in every feature set. Today's isolation test bans named crates and its cuda case asserts that `visionclaw-gpu` is present, so the check (a prefix ban with one exception, cuda assertion inverted) is added at import. Dropping `visionclaw-gpu` closes ADR-2143's "cuda graph covered from P2" caveat.
- I10 (from ADR-2143 A5, same number): no git dependency on the VisionClaw repository. Not enforced until import.
- I11 (new): no permissive crate depends on an AGPL crate. Not enforced until the import adds a licence check over `cargo metadata`.

## Consequences
- **Gained.** Codescape builds, tests and releases without a VisionClaw checkout. The two permissive crates can be published. The GPU path no longer needs nvcc at Codescape build time, and a PTX swap no longer needs a rebuild.
- **Costs.** VisionClaw must publish and version `visionclaw-wire`, and a V3 change becomes a two-repository change (publish, then bump here). The PTX now crosses a runtime boundary, so provenance rests on the manifest check, whose known gap (a hand-written manifest passes) matters more than before. A submodule pointer must be bumped in VisionClaw for each deployed change.
- **Follow-on work at import.**
  - The I1 negative control builds against VisionClaw's `visionclaw-ontology` by sibling path; it needs a self-contained fixture.
  - The client's `protocol.test.ts` and its isolation exception import VisionClaw's `client/src/types/binaryProtocol.ts`; a source for those test fixtures is undecided.
  - Add the I1a, I10 and I11 checks.
  - Re-run the P0/P1 gates here and commit the output under `docs/evidence/`.
- **Forbidden.** A dependency on any VisionClaw crate other than `visionclaw-wire`; a path or git dependency on the VisionClaw repository; compiled-in PTX.

## Alternatives rejected
- **Stay in VisionClaw.** Codescape's releases would remain tied to a 72 GB repository and its CI, the permissive crates could not be published cleanly from an AGPL tree with estate-specific neighbours, and isolation from the knowledge-work system would keep depending on reviewers noticing cross-tree edits.
- **`git subtree split` to carry history.** The Codescape tree was committed late (ADR-2143 A2.6 cites working-tree lines because the tree was untracked), so a split would carry little real history while dragging VisionClaw paths and commit messages about unrelated work. A fresh import that cites the source range keeps the provenance without the noise.
- **Git dependency on VisionClaw for the wire fixtures and the GPU crate.** Cargo would fetch the 72 GB repository for every cold build, the dependency would pull VisionClaw's workspace resolution into Codescape's lock, and `visionclaw-gpu` brings `visionclaw-domain` with it, which is exactly the knowledge-side surface I1 exists to keep out.

## Verification
- The repository skeleton (this record, BASELINE, licences, workspace stub) is committed; no code is imported yet, so `implementation_status` is `partial` and `verified_commit` is left empty until the import commit can be named.
- To verify after import: `cargo test --workspace`, `cargo tree -p codescape-model | grep -c graphicle` = 0, `cargo metadata` shows no `visionclaw-*` package other than `visionclaw-wire`, and the I1a, I10 and I11 checks pass.
