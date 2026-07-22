# Network Interfaces (WAN / LAN / Kernel) Design

**Date:** 2026-07-22  
**Status:** Approved for implementation  
**Product alignment:** daed Config → *Interface and Kernel Options*

## One-liner

Operators configure dae **WAN / LAN interfaces** and **auto_config_kernel_parameter** on a dedicated Network page; values persist in `config_meta` and are rendered into `global {}` on Apply.

## Background

- chaos currently hardcodes `wan_interface: auto` and `auto_config_kernel_parameter: true` in `config_render.rs`.
- `LanConfig` exists but Apply always calls `render_dae_config(..., None)` so LAN never appears.
- Dashboard diagnostics only **lists** interface names (read-only).
- daed exposes multi-select `wanInterface` (includes `auto`) and `lanInterface`, plus `autoConfigKernelParameter`, fed by a live interface inventory (name, IPs, default-route flag).

## Goals

1. Parity with daed’s **Interface and Kernel Options** surface (not full global form).
2. Persist settings across restarts.
3. Apply path writes the same values into `config.dae` `global {}`.
4. Default behavior matches today’s chaos (no breaking change on upgrade).
5. en + zh-CN UI strings; automated tests for render + API validation.

## Non-goals

| Non-goal | Notes |
|----------|--------|
| Full dae `global` form | tproxy port, log level, check URLs, dial mode, etc. stay hardcoded |
| LAN device inventory / MAC allowlist | Spec task 3.2 second half; later |
| Network document inside Config Profiles | Can unify with DNS later |
| Windows Wintun interface picker | Windows data plane remains unavailable |
| `disable_snat` | Not a dae global field; remove incomplete `LanConfig` path |

## Decisions (locked)

| Topic | Decision |
|-------|----------|
| Approach | Independent Network document API + `config_meta` (same pattern as DNS) |
| UI | New `/(app)/network` under Policies nav (between DNS and Settings) |
| Storage | Single meta key `network.document.v1` (JSON) |
| WAN default | `["auto"]` |
| LAN default | `[]` (omit `lan_interface` line) |
| Kernel default | `true` |
| Multi-value format | Comma-joined in dae config (`eth0,wlan0`) |
| Save vs Apply | PUT only persists; Apply/reload is separate (Dashboard) |
| Missing interface names | Allow save (hot-plug); dae fails at runtime if invalid |
| `auto` on LAN | Forbidden |
| Empty WAN | Forbidden |
| `lo` in inventory | Filtered out of GET interfaces |
| Auth | Same as DNS: any authenticated user |

## Data model

### Network document

```json
{
  "wan_interfaces": ["auto"],
  "lan_interfaces": [],
  "auto_config_kernel_parameter": true
}
```

### Meta key

- Key: `network.document.v1`
- Value: JSON string of the document above
- Missing key → API and Apply use defaults

### Core type

```rust
pub struct NetworkConfig {
    pub wan_interfaces: Vec<String>,
    pub lan_interfaces: Vec<String>,
    pub auto_config_kernel_parameter: bool,
}
```

Replace unused `LanConfig` / `render_dae_config_with_lan` with this type and `render_dae_config_with_network`.

## API

Base: `/api/v1` (existing auth middleware).

### `GET /network`

Returns the document (defaults if unset).

### `PUT /network`

Replaces the whole document. Response is the stored document. Does **not** Apply.

### `GET /network/interfaces`

```json
{
  "interfaces": [
    {
      "name": "eth0",
      "ips": ["192.168.1.2/24"],
      "up": true,
      "is_default_route": true
    }
  ]
}
```

- Linux: enumerate non-`lo` links with addresses and default-route flag.
- Non-Linux / failure: empty `interfaces` array (no 500).

### Validation (PUT)

| Condition | Error code |
|-----------|------------|
| `wan_interfaces` empty after normalize | `network_wan_required` |
| Interface name not `auto` and not `[A-Za-z0-9_.:-]+` | `network_invalid_interface` |
| `auto` in `lan_interfaces` | `network_lan_auto_forbidden` |
| Malformed body | `invalid_request` |

Normalization:

- Trim whitespace; drop empty strings.
- Lowercase exact token `auto` only (interface names otherwise keep case).
- Deduplicate preserving order.

## Render rules (Apply)

Always load `NetworkConfig` (defaults if missing). Emit:

```text
global {
  log_level: info
  tproxy_port: 12345
  allow_insecure: false
  wan_interface: <comma-joined wan>
  auto_config_kernel_parameter: true|false
  lan_interface: <comma-joined lan>   # only if lan non-empty
}
```

- If WAN empty after load (corrupt meta), fall back to `auto` at render time.
- Call sites: runtime Apply; any config preview/export that builds dae config.

## UI

### Navigation

- Policies group: Groups, Routing, DNS, **Network**, Settings.
- Icon: distinct from DNS’s `Network` (e.g. `Cable` / `EthernetPort`).

### Page `/(app)/network`

1. PageHeader: Network / 网络.
2. Section Interfaces:
   - WAN multi-select: fixed option `auto` + inventory chips (name, IPs, default-route badge).
   - LAN multi-select: inventory only (no `auto`).
3. Section Kernel:
   - Toggle `auto_config_kernel_parameter` + help text (dae-aligned).
4. Save button → PUT; toast success/error.
5. Dirty guard: same pattern as DNS (`beforeNavigate` + `data-chaos-unsaved`).

Diagnostics panel remains read-only inventory; optional summary of configured WAN/LAN is nice-to-have, not required.

## i18n

Add keys under `nav.network`, `network.*`, and error codes `error.network_*` in `locales/en.json` and `locales/zh-CN.json`. Labels:

- WAN Interface / WAN 接口
- LAN Interface / LAN 接口  
- Auto detect / 自动检测
- Auto config kernel parameter / 自动配置内核参数

Descriptions match daed meaning: WAN = proxy localhost; LAN = proxy LAN.

## Testing

### Unit (`chaos-core`)

- Default network → `wan_interface: auto`, kernel true, no `lan_interface`.
- Multi WAN + multi LAN + kernel false → correct lines.
- Empty LAN → no `lan_interface` line.

### API (`chaos-api`)

- GET defaults.
- PUT round-trip.
- PUT empty WAN → 400 `network_wan_required`.
- PUT `auto` in LAN → 400 `network_lan_auto_forbidden`.
- GET interfaces does not panic.

### Manual DoD

1. Open `/network`: defaults as above.
2. Select LAN + save → reload persists.
3. Dashboard Apply → `data/dae/config.dae` `global` matches.
4. Clear LAN, WAN-only auto → Apply drops `lan_interface`.

## File map

| Area | Files |
|------|--------|
| Core | `crates/chaos-core/src/config_render.rs` |
| Store | `crates/chaos-store/src/config_plane.rs`, `lib.rs` exports |
| API | `crates/chaos-api/src/routes/network.rs` (new), `mod.rs`, `runtime.rs`, optional `config.rs` |
| i18n backend | error code map if centralized |
| Web | `apps/web/src/routes/(app)/network/+page.svelte`, `+layout.svelte`, `api.ts`, locales |

## Definition of Done

1. UI can set WAN / LAN / kernel flag.
2. Values survive refresh.
3. Apply writes matching `global {}`.
4. Upgrade path keeps previous effective defaults.
5. en + zh-CN complete.
6. Core + API tests pass.

## Implementation order

1. `NetworkConfig` + render + unit tests (remove `LanConfig`).
2. Store get/set + API routes + validation tests.
3. Wire Apply/load.
4. Web API client + Network page + nav + i18n.
5. Manual DoD checklist.
