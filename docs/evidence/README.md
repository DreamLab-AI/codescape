# Evidence

Gate output for each phase lives in `docs/evidence/<phase>/`, committed raw with a header recording the date, host, `rustc`, HEAD, the graphicle rev and the `Cargo.lock` sha256, and an exit status per command.

## Before extraction

The P0 and P1 evidence was produced inside VisionClaw and stays there, on branch `feat/codescape`:

- `docs/evidence/codescape/P0/README.md`: model tests, the isolation test catching an injected `visionclaw-ontology`, `cargo metadata --locked` with no forbidden package.
- `docs/evidence/codescape/P0/finding-F-stale-fallback-ptx.md`: why fallback-provenance PTX is refused.
- `docs/evidence/codescape/P1/README.md`, `gate-raw.txt`: 197 Rust tests passed (6 ignored), clippy, fmt, rustdoc and 60 vitest tests, all `exit=0`, plus the cross-vendor audit verdicts.
- `docs/evidence/codescape/P1/gate-xr-wire-raw.txt`, `xr-headset-run.txt`: the unchanged Godot client decoding captured frames and rendering Codescape on a headset.

Performance and conformance numbers in the P1 README were reported by workers and never re-run with committed output.

## After import

These gates are re-run in this repository once the code lands, and their raw output is committed here:

- `P0/`: `cargo test -p codescape-model`; the isolation suite including a self-contained negative control.
- `P1/`: `cargo test --workspace`, clippy `-D warnings`, `fmt --check`, rustdoc `-Dwarnings`, the client vitest suite, and the I1a, I10 and I11 checks.
- `P2/`: `CODESCAPE_GPU_TESTS=1 cargo test -p codescape-layout --features cuda -- --ignored gpu_conforms_to_cpu_oracle` against runtime-loaded PTX, plus the step-time and settle measurements with raw output.

A number without committed raw output is labelled as reported, not as evidence.
