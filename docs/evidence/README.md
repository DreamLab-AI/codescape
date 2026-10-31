# Evidence

Gate output for each phase lives in `docs/evidence/<phase>/`, committed raw with a header recording the date, host, `rustc`, HEAD, the graphicle rev and the `Cargo.lock` sha256, and an exit status per command.

## Before extraction

The P0 and P1 evidence was produced inside VisionClaw and stays there, under [`docs/evidence/codescape/`](https://github.com/DreamLab-AI/VisionClaw/tree/main/docs/evidence/codescape) on `main` (as of `cac36a028`):

- `docs/evidence/codescape/P0/README.md`: model tests, the isolation test catching an injected `visionclaw-ontology`, `cargo metadata --locked` with no forbidden package.
- `docs/evidence/codescape/P0/finding-F-stale-fallback-ptx.md`: why fallback-provenance PTX is refused.
- `docs/evidence/codescape/P1/README.md`, `gate-raw.txt`: 197 Rust tests passed (6 ignored), clippy, fmt, rustdoc and 60 vitest tests, all `exit=0`, plus the cross-vendor audit verdicts.
- `docs/evidence/codescape/P1/gate-xr-wire-raw.txt`, `xr-headset-run.txt`: the unchanged Godot client decoding captured frames and rendering Codescape on a headset.

Performance and conformance numbers in the P1 README were reported by workers and never re-run with committed output.

## Import

[`import-gate.txt`](import-gate.txt): the gate run for the import commit (ADR-0001), raw, with the header above. It ran on the staged tree before the commit (so its HEAD line names the parent, `77a1748`, and the publish dry-run uses `--allow-dirty`). It covers fmt, clippy and tests with and without `cuda`, rustdoc, the publish dry-run of the permissive crates, the graphicle-free check on `codescape-model` and `codescape-collab`, the single `visionclaw-*` package, and the client `npm ci`, typecheck, tests and build. It is not the P0/P1 phase evidence below, which still has to be re-run here.

## After import

These gates are re-run in this repository once the code lands, and their raw output is committed here:

- `P0/`: `cargo test -p codescape-model`; the isolation suite including a self-contained negative control.
- `P1/`: `cargo test --workspace`, clippy `-D warnings`, `fmt --check`, rustdoc `-Dwarnings`, the client vitest suite, and the I1a, I10 and I11 checks.
- `P2/`: `CODESCAPE_GPU_TESTS=1 cargo test -p codescape-layout --features cuda -- --ignored gpu_conforms_to_cpu_oracle` against runtime-loaded PTX, plus the step-time and settle measurements with raw output.

A number without committed raw output is labelled as reported, not as evidence.

## Phase gates re-run here (2026-10-31)

Source: `0256a85` (P0, P1 Rust up to the fmt check) and `865bb26` (fmt fix onward; the P1 client and P2 runs). Host `agentbox`, rustc 1.99.1, `CARGO_TARGET_DIR=/home/devuser/workspace/codescape/target`, raw output, an `exit=` per command. Review follow-up: the rustdoc `-Dwarnings` and `cargo tree` checks were re-run at `0438f96` (docs-only after `865bb26`) and appended to `P1/gate-rust.txt` and `P0/gate.txt`; the client gate header now names `865bb26`, the tree it ran at.

| Phase | File | Result |
|---|---|---|
| P0 | [`P0/gate.txt`](P0/gate.txt) | `codescape-model` 22 + 2 passed; isolation suite 8 passed (includes the negative control, I1a, I10, I11); `cargo tree` for model and collab: zero graphicle lines |
| P1 Rust | [`P1/gate-rust.txt`](P1/gate-rust.txt) | `cargo fmt --check` **exit=1** at `0256a85` (`voice/jev.rs`), kept as the failure, fixed in `865bb26`, re-run exit=0; clippy `-D warnings` exit=0; `cargo test --workspace --locked` exit=0 (0 failed, 6 ignored: the CUDA device tests); `cargo doc` with `-Dwarnings` exit=0 |
| P1 client | [`P1/gate-client.txt`](P1/gate-client.txt) | vitest 8 files / 86 tests passed (includes the I9 isolation test); `vite build` exit=0 |
| P2 | [`P2/gpu-conformance.txt`](P2/gpu-conformance.txt) | the A2.1 required command passed on an RTX 6000 Ada: k-NN overlap (k=10, 400 steps) GPU vs CPU symbols 0.868, files 1.000 (A1 thresholds: symbols ≥ CPU self-baseline 0.734 − 0.1; files ≥ 0.9) |
| P2 | [`P2/ptx-abi-egress.txt`](P2/ptx-abi-egress.txt) | PTX refusal tests (stale 172 B layout, fallback provenance, tag mismatch, arity drift) 15 passed; egress allowlist unit tests 4 passed |

Not evidence, still open: the P2 `docker inspect` (no `/vault`) check is host-side; an egress test that proves outbound connections from the running container are blocked (A2.2) does not exist, since the egress tests above cover the in-process allowlist only; edit-to-visible ≤ 1 s, GPU step ≤ 5 ms at 100k and cold settle ≤ 10 s at 100k were not measured. The PTX used for P2 is the 2026-10-17 `visionclaw-gpu` build (`provenance=compiled`, tag-matched, sha256 in the file), not the PTX the running service loads (tag `7b41d3561a264b78`). The step-time and settle numbers in the P1 README of the VisionClaw tree remain "reported", not evidence.
