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

1. **Setup** admin user (first run) or **Login**
2. **Nodes** / **Subscriptions** — import
3. **Dashboard** — Test all latency, Apply config, Stop dae

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
- **Apply** writes a minimal dae config and runs `dae run -c <work_dir>` (MVP restart strategy). Real transparent proxy needs **capabilities/root** and a suitable kernel; without privileges apply may fail with a clear error.
- For UI-only testing of apply spawn without eBPF, point `CHAOS_DAE_BIN` at `crates/chaos-dae/tests/fixtures/fake-dae.sh`.

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
