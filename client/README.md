# Codescape desktop client

The browser client: React, three and React Three Fiber over the frozen V3/V5 position wire, with JSON events, push-to-talk voice, a typed command palette, a timeline, SpacePilot input and a demo mode. It is its own Vite root with its own `package.json`; it imports nothing from outside this directory (BASELINE I9). Licence: AGPL-3.0-only.

```bash
cd client
npm ci                  # with NODE_ENV unset, or devDependencies are skipped
npm run typecheck       # tsc --noEmit
npm test                # vitest run, incl. the import-isolation test
npm run build           # writes ../target/codescape-dist (override: CODESCAPE_DIST)
npm run dev             # Vite dev server with COOP/COEP headers
```

Serve a build with `codescape-server --static-dir target/codescape-dist`.

Runtime addressing (`src/net/config.ts`): same origin by default; under a `/codescape` first path segment the page treats that as its prefix (served through VisionClaw's port); `?server=http://host:4100` overrides the server; `?demo=1` runs without one.

## Layout

| Path | Contents |
|---|---|
| `src/protocol.ts` | V5 position frames, `0x34` acks, `0x23` agent actions, `/ws/events` JSON shapes |
| `src/net/` | reconnecting sockets, the frame pipeline, REST |
| `src/scene/` | nodes, edges, hulls, labels, picking, LOD, camera, presence |
| `src/ui/`, `src/audio/`, `src/input/` | HUD and palette, push-to-talk PCM, SpacePilot (WebHID) |
| `src/workers/positions.worker.ts` | off-main-thread frame decoding |
| `src/__tests__/` | vitest suites; `fixtures/v5_frame.bin` is the wire fixture |

## Wire fixture

`src/__tests__/fixtures/v5_frame.bin` is a V5 frame of four records (a code node, an actor carrying `AGENT_FLAG`, an id at the top of the 26-bit range, an id the client does not know) written by `visionclaw_wire::v5_frame`. `crates/codescape-server/tests/wire_fixture.rs` fails if it ever differs from what the published wire crate produces, and `protocol.test.ts` decodes every field of it. Never edit it by hand; regenerate with `CODESCAPE_REGEN_FIXTURES=1 cargo test -p codescape-server --test wire_fixture` after a wire change, which needs an ADR.

## Dependencies

The direct dependencies are pinned to the exact versions VisionClaw's client resolved at import (react 19.2.4, three 0.183.0, @react-three/fiber 9.5.0, @react-three/drei 10.7.7, vite 6.4.1, vitest 4.0.18, typescript 5.9.3). Transitive versions come from this directory's own `package-lock.json` and can differ from VisionClaw's. In the VisionClaw deployment the entrypoint builds with VisionClaw's `client/node_modules` when this checkout has none (`deploy/README.md`), so keep these pins in step with it.
