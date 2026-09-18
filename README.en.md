# chaos

**English** · [中文](README.md)

Modern control plane for [dae](https://github.com/daeuniverse/dae): **Rust API + SvelteKit console + vendored dae**.

One install is enough — no separate daed / dae-wing required for the product path.

[![Release](https://img.shields.io/github/v/release/chao2hang/chaos?display_name=tag&sort=semver)](https://github.com/chao2hang/chaos/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](Cargo.toml)

---

## Features

| Area | Capabilities |
|------|----------------|
| **Nodes / subscriptions** | Share-link import, subscription fetch & schedule, latency probes |
| **Orchestration** | Visual canvas: rules → node groups / DIRECT; validate, simulate, publish & apply |
| **DNS / network** | DNS upstreams & rules; WAN/LAN interfaces and kernel options |
| **Runtime** | Apply / reload / stop, logs, diagnostics (caps / eBPF / kernel) |
| **Authz** | First account is always **admin**; runtime mutations, publish, backup require admin |
| **Backup** | Create / list / download / restore (DB + `config.dae`) |
| **Packages** | **amd64** and **arm64** `.deb` + FHS `.tar.gz`; tag push builds releases |

The default bind is **`0.0.0.0:2030`** (all IPv4 interfaces), so LAN devices can connect through the host IP. Set `CHAOS_BIND=127.0.0.1:2030` to allow local access only.

---

## Install (recommended)

Download assets from [Releases](https://github.com/chao2hang/chaos/releases/latest).

### Debian / Ubuntu

```bash
# x86_64
curl -fL -O https://github.com/chao2hang/chaos/releases/download/v0.1.0/chaos_0.1.0_amd64.deb
sudo dpkg -i chaos_0.1.0_amd64.deb
# if dependencies fail: sudo apt-get install -f

# aarch64
# curl -fL -O https://github.com/chao2hang/chaos/releases/download/v0.1.0/chaos_0.1.0_arm64.deb
# sudo dpkg -i chaos_0.1.0_arm64.deb

sudo systemctl enable --now chaos
```

### Arch / CachyOS / non-Debian

`.deb` may fail on `Depends: libc6`. Use the FHS tarball:

```bash
curl -fL -O https://github.com/chao2hang/chaos/releases/download/v0.1.0/chaos_0.1.0_linux_amd64.tar.gz
sudo tar -xzf chaos_0.1.0_linux_amd64.tar.gz -C /
sudo systemctl enable --now chaos
```

### Open the console

- Local: **http://127.0.0.1:2030**
- LAN: **`http://<host-LAN-IP>:2030`**

> `0.0.0.0` exposes the service on every reachable interface. Configure the host firewall, and do not expose the port directly to the public Internet before creating the admin account.
>
> Existing installations must change `CHAOS_BIND` in `/etc/chaos/chaos.env` to `0.0.0.0:2030`, then run `sudo systemctl restart chaos`. Upgrades do not overwrite an existing environment file.

1. **First run** → create the admin account (`role = admin`)
2. Import nodes / subscriptions → latency test
3. **Orchestrate** rules → **Publish & apply**
4. **Dashboard** → status, reload / stop dae

```bash
systemctl status chaos
journalctl -u chaos -f
```

| Path | Purpose |
|------|---------|
| `/etc/chaos/chaos.env` | Environment overrides (conffile) |
| `/var/lib/chaos/` | DB, JWT, dae work dir, backups |
| `/usr/lib/chaos/bin/` | `chaos-api`, `dae` |
| `/usr/share/chaos/web/` | Static console |

---

## Automated releases

Push a **`v*`** tag to run GitHub Actions: build **amd64** + **arm64**, then publish a Release.

```bash
git tag v0.1.1
git push origin v0.1.1
```

Workflow: [`.github/workflows/release.yml`](.github/workflows/release.yml)

---

## Development

### Prerequisites

- Linux (data plane matches dae)
- Rust stable, Node 20+, **pnpm** 10+

### Run

```bash
pnpm install
./scripts/fetch-dae.sh          # optional; needed for real Apply
./scripts/dev.sh                # API :2030 + Web :5173
# or: pnpm dev
```

Open **http://127.0.0.1:5173**, or **`http://<host-LAN-IP>:5173`** from the same LAN (Vite proxies `/api`).

```bash
pnpm dev:api    # cargo run -p chaos-api
pnpm dev:web    # SvelteKit
```

### Layout

```text
apps/web              SvelteKit console
crates/chaos-api      REST (default 0.0.0.0:2030)
crates/chaos-core     domain / latency / config render
crates/chaos-dae      dae process integration
crates/chaos-store    SQLite
packaging/debian/     deb / tar packaging
scripts/fetch-dae.sh  fetch pinned dae
locales/              shared en + zh-CN catalogs
```

### Environment

| Variable | Dev default | Meaning |
|----------|-------------|---------|
| `CHAOS_BIND` | `0.0.0.0:2030` | API listen address; use `127.0.0.1:2030` for local-only access |
| `CHAOS_WEB_HOST` | `0.0.0.0` | Vite development server listen address |
| `CHAOS_DATABASE_URL` | `sqlite:./data/chaos.db?mode=rwc` | SQLite |
| `CHAOS_JWT_SECRET` | auto `./data/jwt.secret` | Secret string **or path** to secret file |
| `CHAOS_DAE_BIN` | `third_party/dae/current/dae` | dae binary |
| `CHAOS_DAE_WORK_DIR` | `./data/dae` | config / pid / logs |
| `CHAOS_DAE_LOG_LEVEL` | `info` | dae log level written to `config.dae` (`trace`…`fatal`; invalid falls back to `info`) |
| `CHAOS_DAE_LOG_MAX_BYTES` | `33554432` (32 MiB) | `dae.log` rotation threshold; one previous generation is kept |
| `CHAOS_WEB_DIR` | unset → no static UI | Packaged console directory |
| `CHAOS_BACKUP_DIR` | `./data/backups` | Backups (`/var/lib/chaos/backups` in packages) |
| `CHAOS_AUTOSTART_DAE` | — | `1` restores last config on boot |
| `CHAOS_GEOIP_ENABLED` | off | `1` enables third-party GeoIP lookups |

### Auth

- Empty DB: only **`POST /api/v1/auth/setup`** creates the first user (**admin**)
- Later users default to `user`
- **Apply / stop / reload, publish, node/group/subscription writes, DNS/network writes, backup** require **admin** (`403 admin_required`)

### Apply / privileges

- Latency tests are TCP probes; dae need not be running
- Apply writes `config.dae` mode **0600** and starts/reloads dae
- Transparent proxy usually needs **root or CAP_NET_ADMIN / CAP_BPF**
- Interactive sudo is avoided by default; run as root or use passwordless `CHAOS_DAE_ALLOW_SUDO=1`
- dae appends to `dae.log` and rotates it past `CHAOS_DAE_LOG_MAX_BYTES`, so the work directory holds at most two generations

### Local packages

```bash
./scripts/fetch-dae.sh
CHAOS_VERSION=0.1.0 CHAOS_ARCH=amd64 ./packaging/debian/build.sh
```

### Tests

```bash
cargo test --workspace
pnpm --dir apps/web check
pnpm --dir apps/web build
```

### i18n

Shared `locales/en.json` and `locales/zh-CN.json` for UI and API error messages (`Accept-Language`).

---

## Ports

| Service | Port |
|---------|------|
| **chaos** | **2030** |
| daed (if installed) | 2023 |

chaos does **not** use dae-wing GraphQL.

---

## Platform notes

- Vendored **dae** is **Linux-only**. See [docs/platform/windows-data-plane.md](docs/platform/windows-data-plane.md)
- GeoIP is **off** by default
- Design: [docs/superpowers/specs/2026-07-18-chaos-product-design.md](docs/superpowers/specs/2026-07-18-chaos-product-design.md)

---

## License

Workspace **MIT** (`Cargo.toml`). Bundled **dae** remains under upstream **AGPL** — comply when redistributing.
