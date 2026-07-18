# chaos Groups / Routing / DNS Design (P3–P4 slice)

**Date:** 2026-07-18  
**Status:** Approved for implementation (user requested daed-parity routing)  
**Depends on:** MVP closed loop + i18n

## Goal

Replace the fixed minimal dae template with **editable** groups, routing rules, and DNS so Apply generates config comparable to daed’s control-plane surface (not full Monaco routingA yet).

## MVP of this epic (shippable slice)

| Area | Capability |
|------|------------|
| **Groups** | CRUD groups: name, policy, optional tag filter; seed `proxy` |
| **Routing** | Ordered rule lines (raw dae match → outbound) + fallback; seed sensible defaults |
| **DNS** | Named upstreams + ordered request rules + fallback; seed alidns/googledns |
| **Render** | `render_dae_config(nodes, groups, routing, dns)` used by Apply |
| **UI** | Pages: Groups, Routing, DNS under app nav |

## Non-goals (later)

- Full routingA Monaco editor / LSP  
- GeoIP/geosite file management UI  
- Per-user multi-profile configs  
- Live traffic stats per rule  

## Data model (SQLite)

```sql
groups(id, name UNIQUE, policy, filter_tag NULL, sort_order, created_at)
routing_rules(id, expression, outbound, sort_order, enabled)
-- fallback stored as config_meta key routing.fallback

dns_upstreams(id, name UNIQUE, address, sort_order)
dns_rules(id, expression, upstream, sort_order, enabled)
-- fallback: config_meta key dns.fallback
```

`config_meta(key PRIMARY KEY, value TEXT)` for simple scalars (fallbacks).

## API (auth required)

- `GET/POST /api/v1/groups`, `PATCH/DELETE /api/v1/groups/{id}`  
- `GET/PUT /api/v1/routing` — full document: `{ rules: [...], fallback }`  
- `GET/PUT /api/v1/dns` — `{ upstreams, rules, fallback }`  

## Config render

```
dns { upstream { ... } routing { request { rules...; fallback } } }
node { ... }
group { name { filter: ...; policy: ... } }
routing { rules...; fallback }
```

Built-in outbounds: `direct`, `must_direct`, `block`, plus group names.

## Defaults (seed if empty)

- Group `proxy`, policy `min_moving_avg`, no filter  
- Routing: `pname(NetworkManager, systemd-resolved) -> must_direct`, fallback `proxy`  
- DNS upstreams alidns + googledns; fallback `alidns`  

## Success criteria

1. User can add a group, change fallback, add a domain rule, change DNS upstream, Apply, and generated `config.dae` reflects changes.  
2. Fresh DB still Applies with seeded defaults.  
3. Tests: render unit tests + store CRUD + API auth.  
