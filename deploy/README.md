# Deployment contract

This repository ships no compose file. The host deployment (VisionClaw) implements the contract below and mounts this repository as a git submodule at `codescape/`. Any other host must meet the same contract.

## Ports

| Port | Purpose | Exposure |
|---|---|---|
| `4100` | Clients: REST, `/ws/positions`, `/ws/events`, `/ws/voice`, `/wss`, `/ws/presence`, static desktop bundle | published on host **loopback only** (`127.0.0.1:4100`) until P6 NIP-98; VisionClaw's knob is `CODESCAPE_PUBLISH_ADDR` |
| `4101` | Hook ingest: `POST /ingest/v1/activity`, bearer token | **never published**; reachable on the container network only |

## Service

- Its own compose service and container (`codescape`, network alias `codescape`). Codescape never runs as a process in a container that mounts a knowledge corpus or carries `RUVECTOR_PG_CONNINFO`; if that variable is set the server exits 2.
- Repositories are mounted **read-only, source directories only**: never a whole checkout, a corpus directory, a root `.env` or a secrets tree. `CODESCAPE_REPO` names them as `label=/repos/<label>`.
- `CODESCAPE_PATH_MAP` rewrites the absolute paths agents report (their view of the checkout) to the mount paths.
- Its own cargo registry, git and target caches, never shared with another container.
- `security_opt: no-new-privileges:true`, `cap_drop: ALL`, `cap_add: DAC_READ_SEARCH` only (a compose file secret keeps the host's 0600 ownership).
- Persistent volume for `CODESCAPE_DATA_DIR` (`/data/codescape`): interner `ids.bin`, warm `positions.bin`, graphicle cache, timeline.
- Healthcheck `GET http://127.0.0.1:4100/api/health`. The first start compiles the workspace in release mode; allow a long start period.
- GPU: NVIDIA runtime, `NVIDIA_DRIVER_CAPABILITIES=compute,utility`; `CODESCAPE_CUDA_DEVICE` pins an ordinal.

## Ingest token

- A file secret mounted at `/run/secrets/codescape-ingest-token`, named by `CODESCAPE_INGEST_TOKEN_FILE`. VisionClaw's launcher mints it on first use under its `secrets/`.
- Without the file the ingest listener does not start.
- The agentbox activity-sink hook (agentbox ADR-2144) reads the same token and posts to `http://codescape:4101/ingest/v1/activity`.

## PTX

- `CODESCAPE_PTX_DIR` points at a read-only directory holding `visionclaw_unified.ptx` and `ptx-build-manifest.txt` from a real nvcc build (VisionClaw's `visionclaw-gpu` build output).
- At load the engine checks the `c_params` size (212 bytes), the launched kernels' arities, `provenance=compiled` in the manifest and the manifest's content tag against the file. Any failure falls back to the CPU engine and is reported in `/api/health`.
- The PTX must be exactly the file the build wrote after its rewrite step, not raw nvcc output: the manifest's `rewritten` tag is checked against it.
- PTX is never committed here (`*.ptx` is gitignored) and never compiled in.

## Reverse proxy (`/codescape/` on the shared port)

VisionClaw's dev nginx exposes Codescape at `http://<host>:3001/codescape/` as a **network peer**:

- `location = /codescape` redirects to `/codescape/`; the bundle uses relative asset paths.
- `location ^~ /codescape/` resolves `codescape:4100` lazily through Docker DNS (`resolver 127.0.0.11`), so nginx starts and answers 502 while the service is down.
- `rewrite ^/codescape/(.*)$ /$1 break` strips the prefix.
- Forwards `Host`, `X-Forwarded-Host`, `X-Forwarded-Proto`, `X-Forwarded-For`, and `X-Forwarded-Prefix: /codescape`; WebSocket upgrade with 3600 s timeouts; buffering off.
- Hides the upstream's `Cross-Origin-Opener-Policy`, `Cross-Origin-Embedder-Policy` and `Cross-Origin-Resource-Policy` so the browser sees exactly one copy of each.

Server side, the service sets:

| Variable | Value |
|---|---|
| `CODESCAPE_TRUST_PROXY` | `1` |
| `CODESCAPE_TRUST_PROXY_FROM` | the nginx container's network alias (a compose `container_name` is not a DNS name) |
| `CODESCAPE_PUBLIC_PATH_PREFIX` | `/codescape` |

Trust is narrowed to that peer: a sibling container sending the same headers is treated as trust off. Rate limiting reads the last `X-Forwarded-For` hop, the one nginx appends.

`:3001` is LAN-published in VisionClaw's dev profile, so `/codescape/` there is a deliberate dev-only exposure of an open-auth service (owner waiver, ADR-2143 A4). Production nginx carries no Codescape location until P6.

## Voice and LLM egress

`CODESCAPE_ASR_URL` (nemotron-asr streaming WebSocket), `CODESCAPE_TTS_URL` (pocket-tts) and optionally `CODESCAPE_LOOM_URL` (the Loom door, OpenAI-compatible `/v1`). These, plus the P6 voice-intent endpoint, are the only outbound targets.
