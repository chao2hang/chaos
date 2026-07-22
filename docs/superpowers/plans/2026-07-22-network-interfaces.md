# Network Interfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver daed-parity WAN/LAN/kernel interface configuration: persist via `config_meta`, render into dae `global {}` on Apply, expose REST + `/network` UI.

**Architecture:** Single Network document in `config_meta` (`network.document.v1`). `chaos-core` owns `NetworkConfig` + rendering. `chaos-api` exposes GET/PUT `/network` and GET `/network/interfaces`. Apply loads the document and calls `render_dae_config_with_network`. Web adds a Policies nav page matching DNS save/dirty patterns.

**Tech Stack:** Rust (chaos-core, chaos-store, chaos-api), SvelteKit, shared locales en/zh-CN.

## Global Constraints

- Defaults: WAN `["auto"]`, LAN `[]`, `auto_config_kernel_parameter: true` (no upgrade break).
- Multi-value dae format: comma-joined (`eth0,wlan0`).
- PUT does not Apply; Apply remains Dashboard/runtime.
- Forbid empty WAN; forbid `auto` on LAN; allow unknown interface names on save.
- Filter `lo` from inventory; non-Linux returns empty inventory without 500.
- Auth same as DNS (authenticated user).
- Spec: `docs/superpowers/specs/2026-07-22-network-interfaces-design.md`.

---

### Task 1: NetworkConfig + dae render (core)

**Files:**
- Modify: `crates/chaos-core/src/config_render.rs`
- Test: same file `#[cfg(test)]`

**Interfaces:**
- Produces: `NetworkConfig`, `NetworkConfig::default()`, `render_dae_config_with_network(nodes, plane, network)`, `normalize_interface_name`, `normalize_interface_list`
- Consumes: existing `ConfigPlane`, `NodeForConfig`

- [ ] **Step 1: Write failing tests**

Add tests:

```rust
#[test]
fn default_network_emits_wan_auto_and_kernel_true() {
    let rendered = render_dae_config(&[], &ConfigPlane::default());
    assert!(rendered.contains("wan_interface: auto\n"));
    assert!(rendered.contains("auto_config_kernel_parameter: true\n"));
    assert!(!rendered.contains("lan_interface:"));
}

#[test]
fn network_config_renders_multi_wan_lan_and_kernel_false() {
    let network = NetworkConfig {
        wan_interfaces: vec!["eth0".into(), "wlan0".into()],
        lan_interfaces: vec!["docker0".into(), "br-lan".into()],
        auto_config_kernel_parameter: false,
    };
    let rendered = render_dae_config_with_network(&[], &ConfigPlane::default(), &network);
    assert!(rendered.contains("wan_interface: eth0,wlan0\n"));
    assert!(rendered.contains("lan_interface: docker0,br-lan\n"));
    assert!(rendered.contains("auto_config_kernel_parameter: false\n"));
}

#[test]
fn empty_lan_omits_lan_interface_line() {
    let network = NetworkConfig {
        wan_interfaces: vec!["auto".into()],
        lan_interfaces: vec![],
        auto_config_kernel_parameter: true,
    };
    let rendered = render_dae_config_with_network(&[], &ConfigPlane::default(), &network);
    assert!(!rendered.contains("lan_interface:"));
}

#[test]
fn normalize_interface_list_trims_dedupes_and_lowercases_auto() {
    let list = normalize_interface_list(&[" AUTO ".into(), "eth0".into(), "eth0".into(), "".into()]);
    assert_eq!(list, vec!["auto".to_string(), "eth0".to_string()]);
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cargo test -p chaos-core default_network_emits -- --nocapture
```

Expected: compile error / test not found or NetworkConfig missing.

- [ ] **Step 3: Implement**

Replace `LanConfig` / `render_dae_config_with_lan` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub wan_interfaces: Vec<String>,
    pub lan_interfaces: Vec<String>,
    pub auto_config_kernel_parameter: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            wan_interfaces: vec!["auto".into()],
            lan_interfaces: vec![],
            auto_config_kernel_parameter: true,
        }
    }
}

pub fn normalize_interface_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() {
        return None;
    }
    if name.eq_ignore_ascii_case("auto") {
        return Some("auto".into());
    }
    if name.len() > 64 {
        return None;
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
    {
        return None;
    }
    Some(name.to_string())
}

pub fn normalize_interface_list(raw: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in raw {
        let Some(name) = normalize_interface_name(item) else {
            continue;
        };
        if !out.iter().any(|existing| existing == &name) {
            out.push(name);
        }
    }
    out
}

pub fn render_dae_config(nodes: &[NodeForConfig], plane: &ConfigPlane) -> String {
    render_dae_config_with_network(nodes, plane, &NetworkConfig::default())
}

pub fn render_dae_config_with_network(
    nodes: &[NodeForConfig],
    plane: &ConfigPlane,
    network: &NetworkConfig,
) -> String {
    // ... existing body, but global section:
    let wan = {
        let list = normalize_interface_list(&network.wan_interfaces);
        if list.is_empty() {
            "auto".to_string()
        } else {
            list.join(",")
        }
    };
    let lan = normalize_interface_list(&network.lan_interfaces);
    // emit wan_interface, auto_config_kernel_parameter, optional lan_interface
}
```

Add `use serde::{Deserialize, Serialize};` if not present. Remove `LanConfig` entirely.

- [ ] **Step 4: Run tests — expect PASS**

```bash
cargo test -p chaos-core --lib
```

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-core/src/config_render.rs
git commit -m "feat(core): render configurable WAN/LAN interfaces into dae global"
```

---

### Task 2: Store helpers for network document

**Files:**
- Modify: `crates/chaos-store/src/config_plane.rs`
- Modify: `crates/chaos-store/src/lib.rs` (re-exports)

**Interfaces:**
- Produces: `META_NETWORK_DOCUMENT`, `get_network_document(pool) -> Result<Option<String>>`, `set_network_document(pool, json) -> Result<()>`
- Or reuse generic `get_meta` / `set_meta` with constant only

- [ ] **Step 1: Add constant**

```rust
pub const META_NETWORK_DOCUMENT: &str = "network.document.v1";
```

Export from `lib.rs` via existing `pub use config_plane::{...}`.

- [ ] **Step 2: Optional thin wrappers** (prefer constant + get_meta/set_meta only to stay DRY)

No new migration (config_meta table already exists).

- [ ] **Step 3: Commit**

```bash
git add crates/chaos-store/src/config_plane.rs crates/chaos-store/src/lib.rs
git commit -m "feat(store): meta key for network document"
```

---

### Task 3: Network API routes + validation

**Files:**
- Create: `crates/chaos-api/src/routes/network.rs`
- Modify: `crates/chaos-api/src/routes/mod.rs`
- Modify: `crates/chaos-api/src/main.rs` (`.merge(network_router())`)
- Modify: `locales/en.json`, `locales/zh-CN.json` (error codes)
- Test: unit/integration tests inside `network.rs` `#[cfg(test)]` following nodes/dns patterns

**Interfaces:**
- Produces: `network_router() -> Router<AppState>`
- Routes: `GET/PUT /network`, `GET /network/interfaces`
- DTOs: `NetworkDocument`, `NetworkInterfaceInfo`, `NetworkInterfacesResponse`

- [ ] **Step 1: Implement `network.rs`**

```rust
// GET/PUT NetworkDocument
// validate:
//   wan = normalize_interface_list; if empty -> network_wan_required
//   each raw name: if normalize_interface_name returns None -> network_invalid_interface
//   if any lan item is auto -> network_lan_auto_forbidden
//   store JSON via set_meta(META_NETWORK_DOCUMENT)
// GET interfaces: list non-lo with ips/up/is_default_route (Linux /sys or netlink-lite via std::fs)
```

Inventory implementation strategy (no new crate if possible):

- Read `/sys/class/net/*` for names; skip `lo`.
- `operstate` == `up` → up.
- Parse `/proc/net/fib_trie` is hard; instead parse `ip route` is process-heavy.
- Prefer: read `/proc/net/route` for IPv4 default (destination 00000000) → iface name `is_default_route`.
- IPs: parse `/proc/net/if_inet6` + optional `getifaddrs` via `nix` only if already a dep; else omit IPs with empty array first, or use `std::process::Command` — **do not** spawn. Use `std::fs` + `/sys/class/net/<iface>/address` is MAC not IP.
- Check Cargo.toml for `network-interface` / `nix` / `rtnetlink`. If none, return name + up + is_default_route from `/proc/net/route` and empty `ips` initially; still meet daed-parity selection by name.

Actually check deps — if none, implement:

```rust
fn list_network_interfaces() -> Vec<NetworkInterfaceInfo>
```

using `/sys/class/net` + `/proc/net/route` + reading IPv4 from `/proc/net/fib_trie` is complex. Simpler path: keep diagnostics' `list_interfaces` and enhance with `/sys/class/net/{name}/operstate` and default route from `/proc/net/route`. For IPs use `std::net` + reading `/proc/net/if_inet6` and for IPv4 parse `ip addr` — avoid. **Accept empty ips[]** if no safe reader; UI still works with names.

- [ ] **Step 2: Wire router + error locale keys**

```json
"error.network_wan_required": "Select at least one WAN interface (or auto)",
"error.network_invalid_interface": "Invalid network interface name",
"error.network_lan_auto_forbidden": "LAN interfaces cannot use auto"
```

zh-CN equivalents.

- [ ] **Step 3: Tests**

```rust
#[tokio::test]
async fn network_get_defaults_and_put_round_trip() { ... }

#[tokio::test]
async fn network_put_rejects_empty_wan() { ... }

#[tokio::test]
async fn network_put_rejects_auto_on_lan() { ... }
```

Follow `nodes.rs` test harness: temp sqlite, issue JWT, `oneshot` requests.

- [ ] **Step 4: Run**

```bash
cargo test -p chaos-api network_ -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-api/src/routes/network.rs crates/chaos-api/src/routes/mod.rs crates/chaos-api/src/main.rs locales/en.json locales/zh-CN.json
git commit -m "feat(api): network document and interface inventory endpoints"
```

---

### Task 4: Wire Apply (+ export if easy)

**Files:**
- Modify: `crates/chaos-api/src/routes/runtime.rs`
- Modify: `crates/chaos-api/src/routes/config.rs` (export should include network when loading meta)

- [ ] **Step 1: Helper**

```rust
async fn load_network_config(state: &AppState) -> Result<NetworkConfig, ApiError> {
    let Some(raw) = chaos_store::get_meta(&state.pool, chaos_store::META_NETWORK_DOCUMENT).await? else {
        return Ok(NetworkConfig::default());
    };
    match serde_json::from_str::<NetworkConfig>(&raw) {
        Ok(mut doc) => {
            doc.wan_interfaces = normalize_interface_list(&doc.wan_interfaces);
            doc.lan_interfaces = normalize_interface_list(&doc.lan_interfaces);
            if doc.wan_interfaces.is_empty() {
                doc.wan_interfaces = vec!["auto".into()];
            }
            Ok(doc)
        }
        Err(_) => Ok(NetworkConfig::default()),
    }
}
```

- [ ] **Step 2: Apply path**

Replace `render_dae_config(&for_config, &plane)` with:

```rust
let network = load_network_config(state).await?;
let content = render_dae_config_with_network(&for_config, &plane, &network);
```

- [ ] **Step 3: Export path** — load network from store same way when possible; if export currently uses default plane only, at least pass stored network for global section accuracy.

- [ ] **Step 4: Commit**

```bash
git add crates/chaos-api/src/routes/runtime.rs crates/chaos-api/src/routes/config.rs
git commit -m "feat(api): apply renders stored network interfaces into dae config"
```

---

### Task 5: Web client + Network page + nav

**Files:**
- Modify: `apps/web/src/lib/api.ts`
- Create: `apps/web/src/routes/(app)/network/+page.svelte`
- Modify: `apps/web/src/routes/(app)/+layout.svelte`
- Modify: `locales/en.json`, `locales/zh-CN.json`

- [ ] **Step 1: api.ts**

```ts
export type NetworkDocument = {
  wan_interfaces: string[];
  lan_interfaces: string[];
  auto_config_kernel_parameter: boolean;
};

export type NetworkInterfaceInfo = {
  name: string;
  ips: string[];
  up: boolean;
  is_default_route: boolean;
};

export function getNetwork() {
  return api<NetworkDocument>('/api/v1/network');
}
export function putNetwork(doc: NetworkDocument) {
  return api<NetworkDocument>('/api/v1/network', { method: 'PUT', body: JSON.stringify(doc) });
}
export function getNetworkInterfaces() {
  return api<{ interfaces: NetworkInterfaceInfo[] }>('/api/v1/network/interfaces');
}
```

- [ ] **Step 2: Page** — mirror DNS: load, dirty, save, toast, beforeNavigate.

UI controls:
- WAN: checkbox list starting with `auto`, then interfaces
- LAN: checkboxes for interfaces only
- Kernel toggle
- Save button

- [ ] **Step 3: Nav** — insert `{ href: '/network', label: t('nav.network'), icon: Cable }` after DNS; import `Cable` from lucide.

- [ ] **Step 4: i18n keys** — `nav.network`, `network.title`, `network.subtitle`, `network.wan`, `network.lan`, `network.wanHint`, `network.lanHint`, `network.autoDetect`, `network.kernel`, `network.kernelHint`, `network.save`, `network.saved`, `network.loadFailed`, `network.saveFailed`, `network.defaultRoute`, `network.noInterfaces`.

- [ ] **Step 5: Manual smoke** (if dev server available) or typecheck:

```bash
cd apps/web && pnpm exec svelte-check --threshold error 2>/dev/null | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add apps/web locales
git commit -m "feat(web): network page for WAN/LAN and kernel options"
```

---

### Task 6: Verification gate

- [ ] **Step 1: Full backend tests**

```bash
cargo test -p chaos-core --lib
cargo test -p chaos-api network_
```

- [ ] **Step 2: Confirm default render unchanged for fresh DB**

Fresh defaults still produce `wan_interface: auto` and no `lan_interface`.

- [ ] **Step 3: Spec checklist**

Map DoD from design doc; mark complete only when all pass.

- [ ] **Step 4: Final commit if fixups**

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| NetworkConfig + render | 1 |
| Meta key | 2 |
| GET/PUT /network | 3 |
| GET /network/interfaces | 3 |
| Validation codes | 3 |
| Apply wiring | 4 |
| /network UI + nav | 5 |
| i18n | 3+5 |
| Tests | 1,3,6 |
| Defaults non-breaking | 1,4,6 |

## Placeholder scan

None intentional; inventory may return empty `ips` if OS parse limited — still valid per name-based selection.
