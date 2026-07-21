# 分流编排：Start / End 锚点与 Chain 骨架

**Date:** 2026-07-21  
**Status:** Approved for implementation (design review)  
**Product name (UI):** 分流编排 only — do **not** surface schema or product “V2/V3” version labels to end users  
**Depends on:** Existing graph orchestration (`crates/chaos-core/src/orchestration.rs`, orchestrate UI), runtime publish/apply  
**Related:** `docs/platform/orchestration-v3-hops.md` (true multi-hop data plane — out of scope for execution in this slice)

## Goal

Make the orchestration canvas readable and operable as a flow:

1. **Start** and **End** anchors so traffic entry and default exit are obvious.
2. **Configurable fallback** via End (not hard-coded `direct`).
3. **Chain** node as a control-plane skeleton for ordered hops (inspector-edited), without pretending the Linux dae engine can execute multi-hop yet.
4. Keep compile semantics as **priority rule match → outbound**, plus **fallback from End**.

## Non-goals

- True serial proxy multi-hop enforcement on Linux dae (see platform hop doc acceptance gates).
- End → chain or End → reject/block in this slice.
- Drawing hop order as canvas edges.
- Showing document/schema version numbers in the UI.
- DNS page or non-orchestration modules.
- Flattening nested groups and presenting them as multi-hop (explicitly rejected by platform docs).

## Current baseline (before this work)

- Node kinds: `rule`, `node_group`, `builtin(direct)`.
- Edges: `rule → node_group | builtin` only.
- Compile: rules by priority → conditions; **fallback always `"direct"`**.
- UI: Svelte Flow canvas + inspector; no entry/exit anchors.

## Approach (chosen)

**Anchored graph enhancement** (control plane + UI skeleton):

```text
start  → rule  → node_group | builtin(direct) | chain
end    → node_group | builtin(direct)     # exactly one edge = fallback
```

- Start edges are **UI/validation only** (not dae match steps).
- Chain hops are an **ordered list in the inspector**, not canvas nodes.
- Publish is **capability-gated** for multi-hop chains.

Internal document schema version may bump for migration; users only see 「分流编排」.

---

## Graph model

### Node kinds

| kind | Count | Deletable | Role |
|------|-------|-----------|------|
| `start` | exactly 1 | no | Traffic entry anchor |
| `rule` | 0..n | yes | Matcher + priority |
| `node_group` | 0..n | yes* | Terminal weighted outbound |
| `builtin` (`direct`) | exactly 1 | no | Direct terminal |
| `chain` | 0..n | yes* | Ordered hop container (control plane) |
| `end` | exactly 1 | no | Default exit / fallback anchor |

\* Deletion blocked while referenced by a rule or end edge (same spirit as `resource_in_use`).

Stable ids for fixed nodes (recommended): `start`, `end`, `direct` (direct already uses a fixed id today).

### Legal edges

| Edge | Cardinality | Notes |
|------|-------------|--------|
| `start → rule` | each rule: exactly 1 inbound from start | Auto-created when adding a rule |
| `rule → node_group \| builtin \| chain` | each rule: exactly 1 outbound | Replace edge to retarget |
| `end → node_group \| builtin` | end: exactly 1 outbound | Defines fallback |

**Illegal (reject on connect / validate):**

- `start → end`, `start → group|chain|direct`
- any outgoing from `node_group`, `builtin`, `chain`
- `end → chain`
- self-loops, duplicate pairs, dangling endpoints
- hop-as-edge on canvas

### Chain data (node `data`)

```text
name?: string          # canvas label
hops: Hop[]            # ordered
```

```text
Hop =
  | { kind: "node", id, weight? }
  | { kind: "subscription", id, weight? }
  | { kind: "group", id, weight? }
```

Align field shapes with existing `GroupSource` where practical so inventory pickers stay shared. Weight optional for chain hops in this slice if unused by single-hop collapse.

### End data

No separate fallback string required in node data if the **sole `end → *` edge** is authoritative. Optional mirrored field is allowed only if kept in sync with that edge on every sanitize.

### Default empty document

- Nodes: `start`, `end`, `direct`, optional default `node_group` (preserve current default group if product still wants a seed proxy group).
- Edges: `end → direct`.
- No rules ⇒ all traffic uses End fallback (direct).

---

## Compile semantics

1. Collect all `rule` nodes; sort by `(priority asc, id)`.
2. Each rule’s single outbound becomes a compiled route:
   - `node_group` / `direct` → same outbound naming as today (`dae_identifier` / `"direct"`).
   - `chain`:
     - **0 hops:** graph may allow draft save with warning; **publish fails** (`chain_empty` or equivalent).
     - **1 hop:** **collapse** to that hop’s outbound (node → its rendered identity / subscription expansion policy as for groups; group → group outbound). Publish allowed; behavior ≡ V2 single outbound.
     - **≥2 hops:** draft OK if graph-valid; **publish fails** with `chain_multi_hop_unsupported` while the active data plane does not advertise ordered-hop support. **Do not** silently flatten multi-hop into a parallel group.
3. `CompiledRouting.fallback` = outbound of End’s single target (group name or `"direct"`). **Must not** hard-code `"direct"` when End points elsewhere.
4. Start topology does not emit dae rules.

Future true multi-hop IR remains specified in `orchestration-v3-hops.md`; this slice only reserves chain shape and gates publish.

---

## Migration (load path, user-invisible)

When loading a stored document that lacks `start`/`end` (or internal version &lt; current):

1. Insert unique `start` and `end` if missing.
2. For each `rule`, ensure exactly one `start → rule` edge.
3. Set `end →` target from legacy compiled/stored fallback (default `direct`). If fallback names a group, resolve to that `node_group` node; on failure, attach `end → direct` and surface a recoverable validation issue (not a version banner).
4. Leave existing `rule → group|direct` edges unchanged; do not invent chains.
5. Persist only on explicit save/publish; migration may be pure in-memory until save.

No UI copy that says “upgraded from V2”. Optional neutral toast is not required.

---

## Validation layers

| Layer | When | Rules |
|-------|------|--------|
| **Graph** | save draft + publish | Fixed node counts; legal edge set; rule start-in + single out; end single terminal out; positions/viewport finite; group/rule field rules as today; chain hop refs exist; composed group cycles as today |
| **Capability** | **publish only** | Multi-hop chain unsupported; empty chain as rule target |

- **Save draft:** Graph must pass. Capability issues → warn in UI (disable or annotate Publish), do not block save if graph-valid (product choice: allow save with multi-hop draft).
- **Publish:** Graph + capability. On failure, stable API `code` + i18n message; no schema version in message text.

### Suggested issue / error codes (API)

- `start_required` / `end_required` / `direct_required`
- `invalid_connection`
- `rule_start_required` / `rule_target_required`
- `end_target_required` / `end_target_invalid`
- `chain_empty` / `chain_multi_hop_unsupported`
- Existing codes retained where still applicable (`invalid_rule_pattern`, `group_terminal_required`, …)

---

## UI / UX

### Product copy

- Page and nav: **分流编排** (and en equivalent “Orchestration” / “Traffic flow” — pick existing i18n keys; no “V2/V3”).
- End inspector label: **默认出口** (default exit).
- Chain capability hint (example): 「当前引擎仅支持单跳出口；多跳可保存草稿，发布需引擎支持。」
- Single-hop chain: 「将按普通出口发布。」

### Canvas

- Layout zones: Start left; rules center; outbounds + End right.
- Auto-layout respects zones.
- Toolbar add: Rule (auto-link Start), Node group, Chain. Not Start/End/Direct.
- Connection matrix enforces legal edges with short human hints.

### Inspector

| Selection | Contents |
|-----------|----------|
| Start | Read-only role + connected rule count |
| Rule | Matcher, pattern, priority; outbound target picker |
| Node group | Name, policy, sources (existing) |
| Chain | Name; ordered hop list add/remove/reorder; capability banner |
| End | Default exit picker: `direct` or a `node_group` (rewires end edge) |
| Direct | Read-only terminal |

### Save / Publish / Apply

- Save → draft document (graph validation).
- Publish → capability checks + existing atomic publish plan path; fallback from End.
- Apply → unchanged entrypoint; consumes published plan / rendered config with new fallback.

---

## Backend / frontend touchpoints (implementation guide)

| Area | Likely files |
|------|----------------|
| Schema, validate, compile | `crates/chaos-core/src/orchestration.rs` |
| API load/save/publish | `crates/chaos-api/src/routes/orchestration.rs`, runtime apply load of compiled fallback |
| Store meta keys | existing orchestration flow/draft/plan keys; migrate on read |
| Web helpers | `apps/web/src/lib/orchestration.ts` |
| Canvas / nodes | `OrchestrationCanvas`, new `StartNode` / `EndNode` / `ChainNode` under `apps/web/src/lib/flow/` |
| Page / inspector | `apps/web/src/routes/(app)/orchestrate/+page.svelte`, `OrchestrationInspector.svelte` |
| i18n | `locales/*` — no version strings |
| Types | `apps/web/src/lib/api.ts` orchestration DTOs |

Exact function splits left to the implementation plan.

---

## Testing / acceptance

1. **Migration:** Old document loads with Start/End; every rule linked from Start; End edge matches legacy fallback; can save without manual rewiring.
2. **Graph validation:** Missing end edge, dual rule outs, illegal edge types → blocked with human-readable errors.
3. **Fallback publish:** End → named group ⇒ rendered dae `fallback:` is that group, not hard-coded direct.
4. **Chain 1 hop:** Publish succeeds; routing outbound equals collapsed hop.
5. **Chain ≥2 hops:** Draft save OK; publish returns `chain_multi_hop_unsupported` (or equivalent); Apply must not pretend multi-hop.
6. **UI:** No version labels; Start/End/Direct not deletable.
7. **Regression:** Graphs with only group/direct outbounds behave as before aside from Start edges and configurable fallback.

## Risks

| Risk | Mitigation |
|------|------------|
| Users believe multi-hop already works | Inspector + publish error; never silent flatten |
| Migration loses fallback | Resolve carefully; fail soft to direct + issue |
| Auto start edges fight manual edits | Max one start→rule per rule; reject duplicates |

## Open decisions (resolved in brainstorm)

| Topic | Decision |
|-------|----------|
| Topology | Fixed Start/End anchors + rule fan-out (not linear rule pipeline) |
| End targets | `direct` or `node_group` only |
| Multi-hop execution | Control-plane skeleton only; publish gated |
| User-facing versioning | None — product is just 分流编排 |

## Implementation next step

After user approves this written spec, create an implementation plan via the writing-plans skill (task checklist, tests first where applicable, file-level steps).
