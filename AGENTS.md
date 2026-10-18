# Codescape: agent instructions

> Canonical, tool-neutral instructions for every coding agent (Claude Code, Codex, others). Claude-only affordances live in `CLAUDE.md`, which imports this file.

Codescape is a near-real-time 3D view of a codebase that humans and agents share by voice. It is a separate product from VisionClaw's knowledge-work system and must stay that way. Only constraints you cannot discover from the code live here.

## Ground truth

The governing document is [`docs/BASELINE-codescape.md`](docs/BASELINE-codescape.md); its *Invariants* section is the compliance surface. Decisions amending it are in the [`docs/adr/`](docs/adr/) ledger (routing: [`docs/adr/README.md`](docs/adr/README.md)). Lookup order: BASELINE, then its `file:line` citations, then `docs/adr/`. Rationale for anything decided before extraction is VisionClaw's ADR-2143 and its amendments A1 to A4; it is history here, not authority.

New decision: copy `docs/adr/TEMPLATE.md` to the next free `ADR-NNNN-slug.md`, fill the three status axes honestly, update BASELINE in the same change, add the row to `docs/adr/README.md`.

## Layout

| Path | Contents |
|---|---|
| `crates/codescape-model` | pure types, ids, LOD, V3 mapping (MIT OR Apache-2.0) |
| `crates/codescape-collab` | pure actors, heat, floor, grammar (MIT OR Apache-2.0) |
| `crates/codescape-ingest` | graphicle facade; the only graphicle user (AGPL-3.0-only) |
| `crates/codescape-layout` | CPU oracle, `cuda` engine over runtime PTX (AGPL-3.0-only) |
| `crates/codescape-server` | actix, every network adapter (AGPL-3.0-only) |
| `client/` | Vite root of the desktop client (AGPL-3.0-only) |
| `deploy/` | the compose and nginx contract the host deployment implements |
| `docs/evidence/<phase>/` | committed gate output per phase |

Paths imported from VisionClaw map as: `crates/codescape/<crate>/` becomes `crates/<crate>/`, and `client/src/codescape/` becomes `client/`.

## Build and test

```bash
cargo test --workspace                                   # all crates, CPU only
cargo test -p codescape-model                            # no graphicle, no GPU, no network
cargo test -p codescape-collab                           # incl. grammar goldens
cargo test -p codescape-ingest                           # compiles graphicle (slow cold)
cargo test -p codescape-layout                           # CPU engine and ABI parsing
cargo test -p codescape-server                           # incl. tests/isolation.rs
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps

# GPU, required for the P2 gate (output committed under docs/evidence/P2/):
CODESCAPE_GPU_TESTS=1 CODESCAPE_PTX_DIR=/path/to/ptx \
  cargo test -p codescape-layout --features cuda -- --ignored gpu_conforms_to_cpu_oracle

cd client && npm ci && npx vitest run && npm run build   # desktop client
```

The two permissive crates must also pass `cargo publish --dry-run -p <crate>` before a release.

## Isolation rules (never weaken)

1. **Build graph.** No ontology, memory-cloud, vault, Oxigraph, Postgres, RuVector, Solid or VisionClaw server crate in the resolved graph, default or `cuda`. Enforced by `crates/codescape-server/tests/isolation.rs`. From import, the only `visionclaw-*` dependency permitted is `visionclaw-wire`, and no crate takes a git dependency on the VisionClaw repository (BASELINE I1a, I10; checks added at import).
2. **Boot refusal.** The server exits 2 when `RUVECTOR_PG_CONNINFO` is set. The test spawns the binary; never test it against the runner's own environment.
3. **Client imports.** `client/**` imports only its own modules and the allowlisted packages (react, three, R3F and the test tooling). Enforced by the client isolation vitest.
4. **Filesystem.** A deployment mounts repositories read-only and as source directories only, never a knowledge corpus, a root `.env` or a secrets tree. Codescape never runs as a process inside a container that can read a corpus.
5. **Egress.** Outbound connections go through the single allowlist in `codescape-server/src/egress.rs`: ASR, TTS, the Loom door and, from P6, the voice-intent endpoint. Adding a target needs an ADR.

## Conventions

- **No new wire format.** Positions are the frozen 52-byte V3 record inside V5 envelopes; agent actions are `0x23`; pacing honours `0x34` acks. Everything else is JSON on `/ws/events`. Never claim a tag byte or a flag bit; actors carry `0x8000_0000`, code nodes none, bit 29 stays free.
- **Interned ids, never hashed.** 26-bit ids, monotonic within an epoch; a full re-extract bumps the epoch.
- **Licence split.** `codescape-model` and `codescape-collab` never gain a dependency on an AGPL crate, on graphicle, or on anything doing I/O. A new crate states its licence in its own manifest.
- **Commands are typed before they run.** LLM output that fails `Command::validate` is spoken back, never executed. Codescape holds no signing key.
- **graphicle** is pinned by git rev; bumping it is its own change with a re-run of the ingest tests. Never enable its `transcription` or `bedrock` features.
- **PTX** comes from `CODESCAPE_PTX_DIR` at runtime. Never commit PTX, and never weaken the ABI or provenance checks to make a GPU run pass.
- **Forbidden routes.** Knowledge routes (`/api/memory-cloud/*`, `/api/analytics/*`, `/api/graph/fold`, `/api/canary/*`) return 404 and stay that way.
- UK English in prose and docs. No em-dashes in prose.

## Memory

Durable agent memory is the shared RuVector store, reached only through the memory MCP tools, namespace `project-state` (prefix keys with `codescape-`). Search it before re-deciding something; store durable lessons afterwards.
