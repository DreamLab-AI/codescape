# Codescape desktop client

Placeholder. The client is imported from VisionClaw's `client/src/codescape/`, where it is its own Vite root, and lands here as `client/` (ADR-0001).

What arrives:

- `index.html`, `vite.config.ts`, `vitest.config.ts`, `tsconfig.json` and `src/` (React, three, React Three Fiber; scene, net, audio, UI, workers, demo mode).
- Its own `package.json`. In VisionClaw it shared the host client's dependencies; here it carries only the allowlisted packages.

Expected commands after import:

```bash
cd client
npm ci
npx vitest run          # includes the import-isolation test (BASELINE I9)
npm run build           # writes client/dist, served with --static-dir client/dist
npm run dev             # Vite dev server with COOP/COEP headers
```

Runtime addressing (`src/net/config.ts`): same origin by default; under a `/codescape` first path segment the page treats that as its prefix (served through VisionClaw's port); `?server=http://host:4100` overrides the server; `?demo=1` runs without one.

The isolation test must keep passing: the client imports nothing outside `client/` except allowlisted packages. Its one VisionClaw exception (`client/src/types/binaryProtocol.ts`, used by tests for wire fixtures) needs a replacement source at import.
