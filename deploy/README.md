# Deployment contract

This repository ships no compose file. The host deployment (VisionClaw) implements the contract below and mounts this repository as a git submodule at `codescape/`. Any other host must meet the same contract.

## Compose contract

The service the host defines. Values are VisionClaw's; another host substitutes its own paths. `deploy/codescape-entrypoint.sh` is the entrypoint and reads only the variables listed here.

```yaml
services:
  codescape:
    container_name: codescape
    hostname: codescape
    build:                         # any image with cargo, node 22 and, for cuda, nvcc
      context: .
      dockerfile: Dockerfile.unified
      target: development
    entrypoint: ["/app/codescape/deploy/codescape-entrypoint.sh"]
    environment:
      CODESCAPE_SRC: /app/codescape                       # this repository, read-only
      CODESCAPE_GPU_SRC: /app/crates/visionclaw-gpu       # PTX build sources, read-only
      CODESCAPE_NODE_MODULES: /app/client/node_modules    # used when client/node_modules is absent
      CODESCAPE_TARGET_DIR: /codescape-target             # all build output, writable volume
      CODESCAPE_PTX_DIR: /codescape-target/ptx            # staged by the entrypoint
      CODESCAPE_DATA_DIR: /data/codescape
      CODESCAPE_PORT: "4100"
      CODESCAPE_INGEST_PORT: "4101"
      CODESCAPE_BIND: 0.0.0.0
      CODESCAPE_PREFER_CUDA: "1"
      # The first entry is the default scene. `estate` is the whole mount tree
      # (its L0 is the repositories); each member is also its own scene.
      CODESCAPE_REPO: estate=/repos/estate,visionclaw=/repos/estate/visionclaw,agentbox=/repos/estate/agentbox,codescape=/repos/estate/codescape,solid-pod-rs=/repos/estate/solid-pod-rs,nostr-rust-forum=/repos/estate/nostr-rust-forum,dreamlab-ai-website=/repos/estate/dreamlab-ai-website,loom=/repos/estate/loom,knowledgeGraph=/repos/estate/knowledgeGraph,sidestr-rs=/repos/estate/sidestr-rs,visiontouch-rs=/repos/estate/visiontouch-rs
      CODESCAPE_EXCLUDES: target/**,node_modules/**,data/**,logs/**,.dev-inputs/**,archive/**,vircadia-world/**,dist/**,.venv/**
      CODESCAPE_PATH_MAP: /home/devuser/workspace/project=/repos/estate/visionclaw:/home/devuser/workspace/project/agentbox=/repos/estate/agentbox:...  # one agent-side prefix per mounted repository
      CODESCAPE_INGEST_TOKEN_FILE: /run/secrets/codescape-ingest-token
      CODESCAPE_ASR_URL: ws://nemotron-asr:8000/api/asr-streaming
      CODESCAPE_TTS_URL: http://pocket-tts:8000
      CODESCAPE_JEV: "1"
      CODESCAPE_JEV_ENDPOINT: http://systemone:8097/v1/systemone
      CODESCAPE_JEV_MODEL: laya-typed-decisions
      CODESCAPE_JEV_LOCAL: "1"
      CODESCAPE_JEV_TIMEOUT_MS: "4000"
      # Unset until calibrated: enabled calls do not imply command acceptance.
      CODESCAPE_JEV_MIN_CONFIDENCE: ""
      CODESCAPE_LOOM_URL: ""                              # optional; empty means unset
      CODESCAPE_TRUST_PROXY: "1"
      CODESCAPE_TRUST_PROXY_FROM: visionclaw-server       # the nginx peer's network alias
      CODESCAPE_PUBLIC_PATH_PREFIX: /codescape
      RUST_LOG: info
      NVIDIA_VISIBLE_DEVICES: all
      NVIDIA_DRIVER_CAPABILITIES: compute,utility
    secrets:
      - codescape-ingest-token
    volumes:
      - ./codescape:/app/codescape:ro                                 # the submodule
      - ./crates/visionclaw-gpu:/app/crates/visionclaw-gpu:ro         # PTX build only
      - ./crates/visionclaw-domain:/app/crates/visionclaw-domain:ro   # visionclaw-gpu's path dependency
      # Dogfood repositories: source directories only, read-only (see Service).
      - ./.gitignore:/repos/estate/visionclaw/.gitignore:ro
      - ./src:/repos/estate/visionclaw/src:ro
      - ./crates:/repos/estate/visionclaw/crates:ro
      - ./client/src:/repos/estate/visionclaw/client/src:ro
      - ./xr-client:/repos/estate/visionclaw/xr-client:ro
      - ./scripts:/repos/estate/visionclaw/scripts:ro
      - ./docs:/repos/estate/visionclaw/docs:ro
      - ./tests:/repos/estate/visionclaw/tests:ro
      - ./agentbox/.gitignore:/repos/estate/agentbox/.gitignore:ro
      - ./agentbox/config:/repos/estate/agentbox/config:ro
      - ./agentbox/services:/repos/estate/agentbox/services:ro
      - ./agentbox/management-api:/repos/estate/agentbox/management-api:ro
      - ./agentbox/crates:/repos/estate/agentbox/crates:ro
      - ./agentbox/lib:/repos/estate/agentbox/lib:ro
      - ./agentbox/scripts:/repos/estate/agentbox/scripts:ro
      - ./agentbox/skills:/repos/estate/agentbox/skills:ro
      - ./agentbox/voice:/repos/estate/agentbox/voice:ro
      - ./agentbox/tests:/repos/estate/agentbox/tests:ro
      - ./agentbox/docs:/repos/estate/agentbox/docs:ro
      # Estate siblings (see below): one .gitignore plus source directories each,
      # all under the one estate root.
      - ${CODESCAPE_ESTATE_ROOT}/codescape/.gitignore:/repos/estate/codescape/.gitignore:ro
      - ${CODESCAPE_ESTATE_ROOT}/codescape/crates:/repos/estate/codescape/crates:ro
      - ${CODESCAPE_ESTATE_ROOT}/codescape/client:/repos/estate/codescape/client:ro
      # ... and likewise for solid-pod-rs, nostr-rust-forum, dreamlab-ai-website,
      # loom, knowledgeGraph, sidestr-rs and visiontouch-rs.
      # Own caches and state, never shared with another container.
      - codescape-cargo-cache:/root/.cargo/registry
      - codescape-cargo-git:/root/.cargo/git
      - codescape-target:/codescape-target
      - codescape-data:/data/codescape
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - DAC_READ_SEARCH
    ports:
      - "127.0.0.1:4100:4100"      # clients only; 4101 is never published
    networks:
      visionclaw_network:
        aliases:
          - codescape
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu, compute, utility]
    runtime: nvidia
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://127.0.0.1:4100/api/health"]
      interval: 30s
      timeout: 5s
      retries: 5
      start_period: 900s           # the first start compiles the workspace in release mode

volumes:
  codescape-cargo-cache:
  codescape-cargo-git:
  codescape-target:
  codescape-data:

secrets:
  codescape-ingest-token:
    file: ./secrets/codescape-ingest-token
```

Compared with the service VisionClaw ran before extraction (`docker-compose.unified.yml` at `cac36a028`): the `crates:/app/crates`, `client/src:/app/client/src` and `scripts:/app/scripts` mounts are replaced by the submodule at `/app/codescape` and the two GPU source directories, and the entrypoint path moves into the submodule.

### What the entrypoint does

1. Exits 2 if `RUVECTOR_PG_CONNINFO` is set.
2. `CODESCAPE_FEATURES` (`auto` by default: `cuda` when `nvcc` and a device exist; or `cuda`, or anything else for CPU only).
3. With `cuda`: builds `visionclaw-gpu` through a throwaway wrapper package on the target volume (its sources are read-only) and stages `visionclaw_unified.ptx` and `ptx-build-manifest.txt` into `CODESCAPE_PTX_DIR`. The GPU sources are a build input for the PTX only; they never enter Codescape's Cargo graph (BASELINE I1a, I10). Without them the engine falls back to CPU.
4. `cargo build --release --locked -p codescape-server` from `CODESCAPE_SRC` into `CODESCAPE_TARGET_DIR`. `--locked` never writes the read-only checkout; the first build fetches `visionclaw-wire` and graphicle into the cargo cache volumes.
5. Copies `CODESCAPE_SRC/client` to `CODESCAPE_TARGET_DIR/client-build`, links its `node_modules` to the checkout's own or, when absent, `CODESCAPE_NODE_MODULES` (VisionClaw's dev image ships the same pinned packages), and runs `vite build --configLoader runner` there into `CODESCAPE_STATIC_DIR` (default `$CODESCAPE_TARGET_DIR/codescape-dist`). Nothing is written into the linked `node_modules`.
6. Execs `codescape-server` with the repos, excludes, ports, data and static directories, and the ingest token when set.

`CODESCAPE_SKIP_BUILD=1` skips steps 3 to 5 and runs the last build.

## Ports

| Port | Purpose | Exposure |
|---|---|---|
| `4100` | Clients: REST, `/ws/positions`, `/ws/events`, `/ws/voice`, `/wss`, `/ws/presence`, static desktop bundle | published on host **loopback only** (`127.0.0.1:4100`) until P6 NIP-98; VisionClaw's knob is `CODESCAPE_PUBLISH_ADDR` |
| `4101` | Hook ingest: `POST /ingest/v1/activity`, bearer token | **never published**; reachable on the container network only |

## Estate siblings

VisionClaw's deployment maps the whole [VisionFlow](https://github.com/DreamLab-AI/VisionFlow) estate, not only the two dogfood repositories. Every repository is mounted under one root, `/repos/estate/<label>`, by the same rule as above: the root `.gitignore` plus its source directories, read-only, and nothing else. `CODESCAPE_ESTATE_ROOT` is the host path of the directory holding the sibling checkouts (on the reference host, the agentbox workspace volume).

Scenes are nested roots. `estate=/repos/estate` is listed first and is therefore the default scene: L0 directory nodes are minted from the first path component, so its L0 is the ten repositories and the client's repository panel shows and hides each one. Each member is listed again as its own scene (`visionclaw=/repos/estate/visionclaw` and so on), reachable with `?repo=<label>` and from the panel's scene links. A hook-reported path under nested roots is delivered to every scene that contains it, so activity lights up in the estate view and the member's. Nothing is ingested twice from disk beyond the walk itself: each scene keeps its own cache under the data directory.

| Label | Mounted | Deliberately left out |
|---|---|---|
| `codescape` | `client`, `crates`, `deploy`, `docs` | `target` |
| `solid-pod-rs` | `crates`, `docs`, `scripts` | `.agentic-qe` witness keys |
| `nostr-rust-forum` | `crates`, `docs`, `scripts`, `tests`, `benchmarks` | `dist`, `target` |
| `dreamlab-ai-website` | `src`, `content`, `marketing`, `forum-config`, `scripts`, `tests`, `docs` | `dist`, `public` (binary assets), `node_modules`, `.agentbox` push keys |
| `loom` | `crates`, `deploy`, `docs`, `tools`, `tests`, `bench`, `scripts`, `model` | `app`, `data`, `legacy`, `uplift-results` (run output, ~700 MB) |
| `knowledgeGraph` | `explorer`, `docs`, `scripts`, `static` | `archive` and `dist`: the published corpus. Codescape maps the explorer and tooling, never a corpus (BASELINE I4) |
| `sidestr-rs` | `docs` and the eleven `sidestr-*` / `webledgers-teller` crate directories | `.agentic-qe` witness keys |
| `visiontouch-rs` | `apps`, `crates`, `docs`, `scripts`, `tools` | `archive`, `target` |

Adding a sibling means: its `.gitignore` and source directories as `:ro` binds under `/repos/estate/<label>`, a `<label>=/repos/estate/<label>` entry in `CODESCAPE_REPO` for its own scene, and one `CODESCAPE_PATH_MAP` entry per agent-side path the hooks will report for it (longest prefix wins, so a nested checkout such as `project/codescape` may sit beside the standalone one). Build output and key material are never mounted; the exclude list is a second fence, not the first, and must never name a member repository (an `agentbox/**` exclude would remove that member from the estate).

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
- The agentbox activity-sink hook ([agentbox ADR-2144](https://github.com/DreamLab-AI/agentbox/blob/feat/codescape-activity-sink/docs/adr/ADR-2144-activity-sink-hook.md)) reads the same token and posts to `http://codescape:4101/ingest/v1/activity`.

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

## System One rollout

The host Compose deployment now opts the existing Jev command classifier in.
Its endpoint is the local System One façade on `visionclaw_network`, not the
agentbox loopback proxy (loopback would resolve inside Codescape itself).
The existing egress allowlist derives the judge URL from this configuration.
Grammar matches still bypass the model, every accepted command still passes
`Command::validate`, and failures retain the existing Loom fallback.

No command acceptance threshold has been calibrated. The deployment leaves
`CODESCAPE_JEV_MIN_CONFIDENCE` unset, preserving the current conservative
acceptance rule; enablement alone is not evidence of improved command accuracy.
Opt out with `CODESCAPE_JEV=0`. Recreate the Codescape service through the host
launch workflow to apply environment changes; a shell export cannot alter the
running process. No restart was performed as part of this change.
