# BASELINE: Codescape

> Living governing document. Its *Invariants* section is the compliance surface. Amend it only together with a record in [`adr/`](adr/README.md), in the same change.

**Status (2026-10-18).** The code is imported from VisionClaw `cac36a028` (on `main`, range `b63068e3d^..cac36a028`; ADR-0001). Citations below are paths and lines in this repository.

## Purpose and scope

Codescape is a near-real-time 3D view of a codebase that humans and agents share. It extracts a code graph (graphicle), lays it out (CPU Barnes-Hut or CUDA), streams positions on the frozen V3/V5 wire to a desktop client and to the Godot XR client (unchanged wire; the client's Codescape mode is VisionClaw ADR-2143 A6), shows who is touching what from agent activity hooks, and takes push-to-talk voice and typed commands that are parsed into typed commands before anything runs.

Out of scope, permanently: any knowledge corpus, ontology, memory store or knowledge route. Codescape is a code-work tool.

## Crate DAG

```mermaid
flowchart BT
  model[codescape-model<br/>MIT OR Apache-2.0]
  collab[codescape-collab<br/>MIT OR Apache-2.0]
  ingest[codescape-ingest<br/>AGPL-3.0-only]
  layout[codescape-layout<br/>AGPL-3.0-only]
  server[codescape-server<br/>AGPL-3.0-only]
  wire[visionclaw-wire<br/>crates.io, MIT OR Apache-2.0]
  graphicle[graphicle<br/>git rev 003f9360, Apache-2.0]
  cust[cust 0.3.2<br/>feature cuda]

  wire --> model
  model --> collab
  model --> ingest
  graphicle --> ingest
  model --> layout
  cust -.-> layout
  model --> server
  collab --> server
  ingest --> server
  layout --> server
```

Arrows point from dependency to dependent. Rules:

- `codescape-model` and `codescape-collab` are I/O-free and permissively licensed; they depend on nothing AGPL and on nothing that does I/O. `cargo tree -p codescape-model` and `-p codescape-collab` show no graphicle.
- `codescape-ingest` is the only crate that depends on graphicle, and no graphicle type crosses its public API.
- `codescape-layout`'s `cuda` feature depends on `cust` only; the PTX is read at runtime from `CODESCAPE_PTX_DIR`. (In VisionClaw it once depended on `visionclaw-gpu` for compiled-in PTX; that edge was removed before import, ADR-2143 A5.)
- `codescape-server` holds every network adapter.
- A sixth crate would need a Codescape wire format, which is forbidden.

## Wire mapping

Codescape produces VisionClaw's frozen V3 record (52 bytes) inside V5 envelopes (`1 + 8 + 52n` bytes) on `/ws/positions`, and the same frames through `/wss` emulation for the XR client. The record layout and its fixtures come from `visionclaw-wire`.

| V3 field | Offset | Code-graph meaning |
|---|---|---|
| `wire_id` | 0 | interned 26-bit id; actors add `AGENT_FLAG` `0x8000_0000`; code nodes carry no flag bits; bit 29 stays free |
| position `x,y,z` | 4 | layout output |
| velocity `vx,vy,vz` | 16 | layout output |
| `sssp_distance` | 28 | BFS hops from the focused node |
| `sssp_parent` | 32 | BFS parent towards the focused node |
| `cluster_id` | 36 | crate or top-level directory |
| `anomaly` | 40 | activity heat, 0..1 |
| `community_id` | 44 | Leiden community, stable-matched across recomputes |
| `centrality` | 48 | importance, 0..1 |

Other frames: `0x23` agent action with the existing `AgentActionType` values; `0x34` acks pace the sender; `initialGraphLoad` (text) carries file-tier topology to XR; `0x43` pose frames on `/ws/presence`. Topology, selection, captions and query results are JSON on `/ws/events`.

## Invariants

`isolation.rs` below is `crates/codescape-server/tests/isolation.rs`. The cargo-metadata checks run with `--locked`, in the default and the `--all-features` (`cuda`) resolution.

| # | Invariant | Enforced by | State |
|---|---|---|---|
| I1 | No forbidden crate in the resolved build graph, default and `cuda` features: no ontology, memory-cloud, vault, Oxigraph, Postgres, RuVector, Solid or VisionClaw server crate | `isolation.rs` ban list `:16-37`, `build_graph_contains_no_knowledge_work_crates` `:106`, `cuda_feature_graph_contains_no_knowledge_work_crates` `:121`; negative control `the_check_itself_catches_an_injected_dependency` `:161` over the in-repo fixture `crates/codescape-server/tests/fixtures/forbidden/visionclaw-ontology` (workspace-excluded, always present) | enforced; gap: scans package names only |
| I1a | The only `visionclaw-*` crate in any feature set is `visionclaw-wire` | `isolation.rs` `visionclaw_wire_is_the_only_visionclaw_crate` `:139` (prefix ban, one exception, both feature sets); the cuda case asserts `visionclaw-gpu` is absent `:121`; the negative control also exercises the prefix filter `:161` | **enforced** |
| I2 | Records are byte-identical to the frozen fixtures: 52 B, `cluster@36`, `anomaly@40`, `community@44`, `centrality@48`, V5 = 1+8+52n | `crates/codescape-model/src/wire.rs` `record_matches_frozen_fixture_bytes` `:138` against `visionclaw-wire`'s fixtures; client side `client/src/__tests__/protocol.test.ts:137-177` decodes `client/src/__tests__/fixtures/v5_frame.bin`, which `crates/codescape-server/tests/wire_fixture.rs` pins to `visionclaw_wire::v5_frame` | enforced |
| I3 | Actors carry `AGENT_FLAG`; code records carry none | `codescape-model/src/wire.rs` `actors_carry_the_agent_flag` `:170`; the committed fixture carries one actor (`protocol.test.ts:146`) | enforced at helper level (a second encoding path could bypass) |
| I4 | `0x34` ack bytes equal the shared fixture | `codescape-model/src/wire.rs` `ack_round_trips_and_matches_fixture_shape` `:207` against `visionclaw_wire::broadcast_ack_frame`; client `protocol.test.ts:179` asserts the 21-byte layout | enforced in Rust; the client ack test checks layout, not a committed byte fixture |
| I5 | Ids are monotonic and stable, survive byte and serde round trips, exhaustion errors instead of wrapping, a hostile count is refused before allocation | `crates/codescape-model/src/ids.rs` tests `:221,237,249,283,296,313` | enforced |
| I6 | Codescape keeps its own `Cargo.lock` and is never a member of another workspace | root `Cargo.toml` `[workspace]`, committed `Cargo.lock`; every gate runs `--locked` | structural (separate repository) |
| I7 | PTX is refused unless `c_params` is 212 B, the launched kernels' arities match (`force_pass_kernel` 30, `cluster_cohesion_kernel` 13, `integrate_pass_kernel` 24) and `ptx-build-manifest.txt` says `provenance=compiled` with a content tag matching the file | `crates/codescape-layout/src/abi.rs:34,40,103,187`; `crates/codescape-layout/src/ptx_source.rs:107` binds the file to the manifest's `rewritten` tag; `content_tag` (FNV-1a 64) `:142`, pinned by `content_tag_matches_fnv1a_64_vectors` `:180`. ABI-parse fixtures `crates/codescape-layout/tests/fixtures/*.ptx` (the only committed PTX; never loaded at runtime) | enforced; gap: a hand-written manifest passes |
| I8 | `RUVECTOR_PG_CONNINFO` set means the server exits 2 at boot | `crates/codescape-server/src/config.rs:15,311`; `isolation.rs` `server_refuses_to_boot_with_a_ruvector_connection_string` `:272` spawns the binary | enforced |
| I9 | `client/**` reaches no file outside `client/` and only allowlisted packages | `client/src/__tests__/isolation.test.ts:19` (allowlist), `:71` (walk), `:111`, `:118` | enforced, no exceptions (the VisionClaw `binaryProtocol.ts` exception was replaced by the committed byte fixture at import) |
| I10 | No dependency on the VisionClaw repository: no `git+` source naming VisionClaw (or its former name AR-AI-Knowledge-Graph), and no path package outside this repository | `isolation.rs` `no_dependency_comes_from_the_visionclaw_repository` `:215` (both feature sets; exactly the five members are path packages); matcher control `the_visionclaw_repo_matcher_catches_known_urls` `:254` | **enforced** |
| I11 | A permissively licensed crate ships with no AGPL or GPL-3.0 crate (normal and build dependencies, transitively) | `isolation.rs` `no_permissive_crate_depends_on_an_agpl_crate` `:324` (both feature sets; at least two permissive members checked) | **enforced** |

Not invariants yet (recorded so nobody cites them as enforced):

- **GPU conforms to the CPU oracle** (files k-NN overlap at least 0.9; symbols at least the CPU self-baseline minus 0.1). The `#[ignore]` test `gpu_conforms_to_cpu_oracle` is a required P2 gate step with committed output, not an invariant.
- **Egress allowlist and no corpus mount.** `codescape-server/src/egress.rs` holds the allowlist, but no test proves the process cannot reach anything else, and compose non-publication is not an egress boundary. Open until an egress test exists.

## Deployment topology

```mermaid
flowchart LR
  B[Browser on LAN] -->|:3001 /codescape/| N[VisionClaw dev nginx]
  N -->|codescape:4100, prefix stripped,<br/>X-Forwarded-*| S[codescape service :4100]
  L[Host loopback :4100] --> S
  XR[Godot XR client] -->|/wss| S
  H[agentbox activity hook] -->|bearer, :4101,<br/>container network only| I[codescape service :4101]
  S --> ASR[nemotron-asr]
  S --> TTS[pocket-tts]
  S -.-> LOOM[Loom door]
```

- One process, two listeners: `:4100` for clients, `:4101` for hook ingest. `:4101` is never published.
- In VisionClaw, Codescape is its own compose service, built by `deploy/codescape-entrypoint.sh` from the `codescape/` submodule mounted read-only, with source directories mounted read-only, its own cargo caches, `cap_drop: ALL` plus `DAC_READ_SEARCH` (to read the compose secret), and `:4100` published on host loopback only. The dev nginx reaches it as a network peer. The full contract is [`../deploy/README.md`](../deploy/README.md).
- Standalone, it is one binary with `--bind 127.0.0.1`.

## Open items

- **Performance targets** (100k nodes): GPU step 5 ms, measured 7.8 ms (kernels 1.7 ms); cold settle 10 s, measured 13.7 s on GPU and 56.6 s on CPU. Cause identified as host-side work per step. Numbers were reported by workers, not re-run; re-run here after import.
- **Presence shim gaps** (P4): `/ws/presence` speaks the XR handshake but carries no avatars between clients; the XR agent hover label is empty.
- **XR writes** (P4): `/wss` ignores `nodePin`, `nodeUnpin` and `nodeDrag`, and there is no physics write, so the XR client greys those controls (VisionClaw ADR-2143 A6). Whether Codescape should honour drag and pin is undecided.
- **XR actor labels**: XR actors show the 64-character pubkey; they should carry a short display label, as the desktop halo label does.
- **Initial camera framing**: at L0 the auto tier's first framing can land inside a community hull.
- **P6 Publish and dispatch**: TLS and NIP-98 on every route, pubkey allowlist, published `:4100`, the kind-31402 relay to `/v1/voice-intent`. Until then auth is open and `:4100` stays on loopback (dev nginx waiver aside).
- **Audit gaps** (ADR-2143 A2.5): bearer scheme match is case-sensitive beyond `Bearer`/`bearer`; the rate limiter is one shared bucket with no constructor validation.
- **Import couplings**: closed at import (ADR-0001). `visionclaw-wire` 0.1.0 comes from crates.io, the PTX from `CODESCAPE_PTX_DIR`, the I1 negative control is an in-repo fixture, the client's wire cross-check is a committed byte fixture, and I1a, I10 and I11 are tested.

## Change process

A change that alters an invariant, the crate DAG, the wire mapping or the deployment contract needs an ADR in `adr/` and an edit here in the same commit. Review triggers: a graphicle rev bump; any change to the V3 record, V5 envelope or `0x23`; a new egress target or published port; a new crate; a licence change.
