# chaos

Modern control plane for [dae](https://github.com/daeuniverse/dae): **Rust API + SvelteKit UI + vendored dae**.

> Status: **MVP on branch `feat/mvp`**. Design: [product design](docs/superpowers/specs/2026-07-18-chaos-product-design.md). Plan: [MVP plan](docs/superpowers/plans/2026-07-18-chaos-mvp.md).

## Goals

- Full replacement for daed as a product experience (phased)
- Single install ships `chaos` and `dae` (no separate dae/daed/dae-wing required for the product path)
- REST JSON API; SvelteKit console

## Layout

```text
apps/web              SvelteKit console
crates/chaos-api      REST server (127.0.0.1:2030)
crates/chaos-core     domain + latency + config render
crates/chaos-dae      dae binary integration
crates/chaos-store    SQLite
scripts/dev.sh        run API + web together
scripts/fetch-dae.sh  download pinned dae into third_party/
third_party/dae/      VERSION pin + current/dae (gitignored binary)
data/                 gitignored SQLite, jwt secret, dae work dir
```

## Prerequisites

- Linux (data plane matches dae)
- Rust stable (`cargo`, `rustc` 1.97+ tested)
- Node 20+ and **pnpm** 10+
- Optional: network access for `scripts/fetch-dae.sh`

## Quick start (dev)

```bash
cd /home/chaos/projects/personal/chaos
git checkout feat/mvp   # if needed

# JS deps
pnpm install

# Optional: vendored dae (enables real apply; needs privileges for eBPF)
./scripts/fetch-dae.sh

# One process: API on :2030 + web on :5173
./scripts/dev.sh
# or: pnpm dev
```

Open **http://127.0.0.1:5173**

1. **Setup** — first run only: create the **first account** (this user is always **admin**)
2. **Login** with that account (or later accounts, if any)
3. **Nodes** / **Subscriptions** / **Flow** — import & orchestrate
4. **Dashboard** — Test latency, Apply config, Stop dae

### Auth system rule

- On a **fresh install** (`users` empty), only `/api/v1/auth/setup` may create the first user.
- That first user is stored with **`role = admin`** and is the system administrator.
- After setup, `/setup` is closed (`already_initialized`); further accounts (future multi-user) default to `role = user`.
- JWT includes `role` for admin-gated APIs later.

### Split terminals

```bash
pnpm dev:api    # cargo run -p chaos-api  → http://127.0.0.1:2030
pnpm dev:web    # SvelteKit               → http://127.0.0.1:5173 (proxies /api)
```

### Environment

| Variable | Default | Meaning |
|----------|---------|---------|
| `CHAOS_BIND` | `127.0.0.1:2030` | API listen address |
| `CHAOS_DATABASE_URL` | `sqlite:./data/chaos.db?mode=rwc` | SQLite |
| `CHAOS_JWT_SECRET` | auto `./data/jwt.secret` | JWT HMAC secret |
| `CHAOS_DAE_BIN` | `third_party/dae/current/dae` if present | dae binary path |
| `CHAOS_DAE_WORK_DIR` | `./data/dae` | config + pid dir |

## Ports vs system daed

| Service | Port |
|---------|------|
| **chaos API** | **2030** |
| system **daed** (if installed) | **2023** |

They can run side by side. chaos does **not** use dae-wing GraphQL.

## Apply / dae notes

- **Latency tests** are TCP connect probes; dae does not need to be running.
- **Apply** writes `data/dae/config.dae` (mode **0600** — dae rejects world-readable configs) and runs  
  `dae run -c <config.dae> --disable-pidfile [--disable-sudo]` (MVP restart strategy).
- Real transparent proxy needs **root / CAP_NET_ADMIN / eBPF** and a suitable kernel. Without privileges Apply returns `dae_permission_denied` or `dae_start_failed` with a log excerpt (not a generic 500).
- By default **sudo is disabled** so the API never hangs on a password prompt. Options:
  - run `chaos-api` as root, or
  - set `CHAOS_DAE_ALLOW_SUDO=1` with **passwordless** sudo for the dae binary, or
- use `CHAOS_DAE_BIN=crates/chaos-dae/tests/fixtures/fake-dae.sh` for UI-only apply tests.

### Platform boundaries

The bundled `dae` data plane is Linux-only. Windows builds keep the control
plane and orchestration editor available, but runtime apply reports
`windows_data_plane_unavailable` until the independent Wintun + sing-box/mihomo
backend is installed. See [docs/platform/windows-data-plane.md](docs/platform/windows-data-plane.md)
for the implementation boundary and native verification requirements.

V2 combined groups are member selection, not serial forwarding. Ordered
multi-hop source hiding is tracked as the separate
[Orchestration V3 hop-chain design](docs/platform/orchestration-v3-hops.md).

GeoIP lookups send node addresses to a third-party service and are disabled by
default. Set `CHAOS_GEOIP_ENABLED=1` only when that is acceptable.
- Inspect last run: `data/dae/dae.log`.

## i18n

Shared catalogs live at repo root:

```text
locales/en.json
locales/zh-CN.json
```

- **Web:** `$lib/i18n` + language switcher; choice stored in `localStorage.chaos_locale`; API calls send `Accept-Language`.
- **API:** `chaos-i18n` embeds the same JSON; `error.message` is localized; `error.code` stays stable.
- Default / fallback: **`en`**. Browser language is used on first visit when no stored preference.
- Design: [i18n design](docs/superpowers/specs/2026-07-18-chaos-i18n-design.md).

## Tests

```bash
cargo test --workspace
pnpm --dir apps/web check
pnpm --dir apps/web build
```

## License

TBD before public release (coordinate with dae AGPL components in packaging).
