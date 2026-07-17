# chaos Product Design

**Date:** 2026-07-18  
**Status:** Approved for implementation planning  
**Location:** `/home/chaos/projects/personal/chaos` (new monorepo, sibling of `daed` / `dae-wing`)

## One-liner

**chaos** is a modern proxy control plane: **Rust REST API + SvelteKit console + vendored dae data plane**. It aims to **fully replace daed** as an installable product so users **do not need to install dae, daed, or dae-wing separately**.

## Background

- `daed` today: React dashboard + bundled dae-wing (Go GraphQL) + dae (eBPF core).
- Node latency UX was added on the daed frontend and depends on wing fields such as `nodeLatencies` / `testNodeLatencies`.
- System package `daed` 1.27.0 may lag frontend APIs; chaos starts clean with its own stack and packaging model.

## Goals

1. **Full product replacement roadmap** for daed management UX (auth, nodes/subscriptions, latency, groups, config/routing/DNS, runtime, packaging).
2. **Rust backend** that **natively drives dae** via a **bundled dae binary** (config write, load/reload, status)—not via dae-wing process or GraphQL compatibility.
3. **SvelteKit frontend** talking only to chaos REST JSON APIs.
4. **Bundled dae in releases** so install = ready to run.
5. **MVP demo path:** login → import nodes/subscriptions → latency test → generate config and load/reload **bundled** dae.

## Non-goals

| Non-goal | Notes |
|----------|--------|
| Rewrite dae eBPF in Rust | Keep upstream dae as data plane |
| dae-wing GraphQL compatibility layer | Public API is chaos REST only |
| Multi-tenant SaaS | Single-machine admin model for MVP |
| Full Windows/macOS eBPF parity in MVP | Linux first |
| Reuse daed React code | New SvelteKit app |
| MVP migration wizard from existing daed | May coexist; different ports and data dirs |

## Decisions (locked)

| Topic | Decision |
|-------|----------|
| Product scope | Full daed replacement (phased delivery) |
| Frontend | SvelteKit + TypeScript |
| Backend | Rust |
| API | REST + JSON |
| Repo | Monorepo at `~/projects/personal/chaos` |
| dae control | Exec bundled (or override) dae binary + config files |
| Packaging | Ship chaos + dae together |
| MVP closed loop | Login → import → latency → apply/load dae |
| Port | Default **2030** (avoid clash with daed `:2023`) |

## Phased roadmap

| Phase | Name | Deliverable |
|-------|------|-------------|
| **P0** | Skeleton | Monorepo, health, auth, SvelteKit shell, dev scripts |
| **P1** | Resources | Nodes/subscriptions import + CRUD + SQLite |
| **P2** | Latency | Single/bulk latency + availability |
| **P3** | Orchestration | Groups, policies, bindings |
| **P4** | Config surface | Config / Routing / DNS editors |
| **P5** | Runtime | dae start/stop, apply/reload, status, basic traffic |
| **P6** | Release | Packages with vendored dae, systemd, docs |

**MVP = P0 + P1 + P2 + minimal P5**  
(P3/P4 use a fixed minimal dae config template until editors exist.)

### MVP success criteria

- On a clean Linux install (or dev build), **no separate dae install** is required.
- User can: setup/login → import share links or subscription URL → run latency tests → **Apply** so bundled dae loads generated config (or surface clear permission/kernel errors).
- UI never hard-depends on dae-wing GraphQL.

## Architecture (recommended approach)

**Single product binary/process model (“chaos supervisor + vendored dae”):**

- `chaos-api` serves REST and (in release) static web assets.
- `chaos-dae` locates bundled `dae`, writes config under chaos data dir, runs load/reload.
- `chaos-core` owns domain logic (parse links, latency probe, config rendering).
- `chaos-store` persists to SQLite.

Rejected for v1:

- Linking dae as a Rust library (Go + eBPF embed cost).
- Requiring system-wide `dae`/`daed` (allowed only as **override** via `CHAOS_DAE_BIN` for hackers, not the default product path).

## Repository layout

```text
chaos/
├── apps/web/                 # SvelteKit
├── crates/
│   ├── chaos-api/            # HTTP entry
│   ├── chaos-core/           # domain + latency
│   ├── chaos-dae/            # dae process/config integration
│   └── chaos-store/          # SQLite
├── third_party/ or cache/    # fetched dae binaries (pin by version)
├── packaging/                # P6
├── docs/superpowers/specs/
├── scripts/dev.sh
├── scripts/fetch-dae.sh
├── Cargo.toml
├── package.json
└── README.md
```

## Process model

### Development

```text
SvelteKit :5173  --REST-->  chaos-api :2030
                               ├── SQLite ./data/chaos.db
                               ├── config  ./data/dae/...
                               └── exec CHAOS_DAE_BIN or third_party/.../dae
```

### Release

```text
chaos.service
  listen 127.0.0.1:2030 (configurable)
  data /var/lib/chaos/
  dae  /usr/lib/chaos/bin/dae  (example layout)
```

Coexistence with system `daed` is supported via different unit name, port, and data directory.

## REST outline (MVP)

Prefix: `/api/v1`  
Auth: Bearer token after login. First-run `POST /auth/setup` when no users exist.

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/auth/setup` | Create admin (only if empty) |
| POST | `/auth/login` | Token |
| GET | `/health` | Liveness + whether dae binary is visible |
| GET/POST | `/nodes` | List / import |
| PATCH/DELETE | `/nodes/{id}` | Update / delete |
| GET/POST | `/subscriptions` | List / import URL |
| POST | `/subscriptions/{id}/refresh` | Re-fetch |
| POST | `/latency/test` | `{ "ids"?: string[] }` omit = all |
| GET | `/latency` | Latest results |
| GET | `/runtime` | running / dirty / versions |
| POST | `/runtime/apply` | Render config + dae load/reload |
| POST | `/runtime/stop` | Stop dae |

Error shape: `{ "error": { "code": string, "message": string } }`.

Latency MVP: **synchronous** batch response (progress streaming is post-MVP).

## Data flows

**Subscription import:** UI → POST `/subscriptions` → fetch URL → parse nodes → store.

**Latency:** UI → POST `/latency/test` → concurrent probes with timeout → persist → return `{ id, latency_ms?, alive, tested_at, message? }[]`.

**Apply:** UI → POST `/runtime/apply` → render minimal dae config from store → write files → bundled dae load/reload → status.

## MVP default routing/config policy

Until P3/P4 editors ship:

- Generate a **fixed minimal template**: `direct` + imported nodes as proxy outbounds; minimal routing that sends traffic to proxy (exact template fixed in implementation plan).
- Document that complex routing waits for P4.

## Latency semantics

| Item | MVP definition |
|------|----------------|
| Measures | Connectivity / handshake delay to node address |
| Does not measure | Bandwidth or full production path via dae |
| Requires dae running? | No |
| Failure | `alive: false`; previous success kept until overwritten |

UI color thresholds (constants, no settings UI in MVP): green &lt; 200ms, yellow 200–500ms, red &gt; 500ms or failed.

## Error handling

| Case | Behavior |
|------|----------|
| Unauthorized | 401 |
| Setup when users exist | 409 |
| Bad subscription fetch | 4xx/502 + message |
| Per-link parse failure | Item error; batch continues |
| Missing dae binary | `dae_binary_missing` on health/apply |
| Permission | `dae_permission_denied` |
| Probe timeout | Node failed; batch continues |
| Unexpected | 500 + request id; no stack to client |

## Security

- Password hashing: argon2 (or equivalent).
- Tokens with expiry; single admin is enough for MVP.
- Default bind **127.0.0.1**.
- Subscription fetch: timeouts and body size limits; document SSRF posture.
- Avoid logging full sensitive node share URLs.

## Packaging / vendored dae

**Dev:** `scripts/fetch-dae.sh` pins a dae release into repo cache; `CHAOS_DAE_BIN` override.

**Release (P6):** install chaos binary + web assets + dae + geo guidance; systemd unit examples.

**CI:** build Rust + web; real eBPF apply is not a default CI gate.

## Frontend IA (MVP)

- Setup / Login  
- Dashboard (health, dae status, latency summary)  
- Nodes / Subscriptions  
- Latency entry points (section actions and/or dedicated view)  
- Runtime (Apply / Stop, error surface)

## Default tech choices

| Layer | Choice |
|-------|--------|
| HTTP | axum + tower |
| DB | SQLite + sqlx |
| Web | SvelteKit + TypeScript |
| Workspace | Cargo + pnpm |

Auth mechanism (JWT vs server session) is fixed in the first implementation plan—pick **one**.

## Testing principles

- Unit tests: parsing, config render, latency result merge.
- Integration: `chaos-dae` against a mock executable in CI.
- E2E: optional after MVP manual closed-loop demo.

## Risks

- Privileges for dae; must fail loudly with guidance.
- Multi-arch CI and dae version pinning cost (owned in P6; dev uses fetch script earlier).
- Latency ≠ production path; document clearly.

## Implementation order (for writing-plans)

1. Scaffold monorepo (cargo + pnpm + dev script).  
2. SQLite + auth setup/login.  
3. Nodes/subscriptions CRUD + parsers.  
4. Latency API + UI.  
5. Config render + apply/reload via chaos-dae.  
6. fetch-dae + health dae visibility.  
7. Dashboard wiring for closed loop.

## Relationship to daed work

The daed web node-latency UX branch remains a **daed frontend enhancement**.  
chaos is a **new product** that re-implements management + latency natively and vendors dae for distribution—not a continuation commit inside `daed/`.
