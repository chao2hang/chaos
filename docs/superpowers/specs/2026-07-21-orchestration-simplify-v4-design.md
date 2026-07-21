# Orchestration V4: Simplify – Remove Chain, Complete Inspector

**Date:** 2026-07-21
**Status:** Design (pre-implementation)
**Supersedes:** V3 (Start/End/Chain addition, spec `2026-07-21-orchestration-start-end-design.md`)

## Motivation

The V3 schema introduced Chain nodes as a forward-looking concept for ordered-hop routing. In practice, the actual use case is **pure rule-based routing**: match traffic by domain/IP/geo, forward to backend node groups or direct, with priority ordering and a configurable fallback.

Chain was implemented as a partial feature with multi-hop unsupported and single-hop requiring runtime materialization. It added complexity without delivering real value. Additionally, the inspector UI had gaps for End node fallback editing and Start node information display.

This spec removes Chain entirely, completes the inspector, and simplifies the data model from 6 node kinds to 5.

## Core Principle

> The orchestration model should reflect what the system actually does: a priority-ordered list of match-forward rules with a configurable fallback. The graph canvas is a visualization layer, not an architectural requirement.

---

## 1. Data Model Changes

### 1.1 Node Kinds (5, down from 6)

| Kind | Type ID | Description | Terminal |
|---|---|---|---|
| Start | `"start"` | Traffic entry anchor (visual/structure only) | No |
| Rule | `"rule"` | Matcher + priority | No |
| NodeGroup | `"node_group"` | Weighted backend pool with load-balance policy | Yes |
| Builtin | `"builtin"` | Direct outbound (only `direct` exists) | Yes |
| End | `"end"` | Fallback exit anchor | No |

**Removed:** `FlowNodeKind::Chain` and all associated types (`ChainNodeInfo`, the separate `GroupSource` re-export).

### 1.2 FlowNodeData (Rust)

```rust
struct FlowNodeData {
    rule: Option<RuleNodeInfo>,
    node_group: Option<NodeGroupInfo>,
    builtin: Option<BuiltinNodeInfo>,
    // start and end have None for all fields
}
```

### 1.3 NodeGroup Sources

`GroupSource` is no longer a shared type between NodeGroup and Chain. NodeGroup sources are defined inline:

```rust
struct GroupSource {
    kind: GroupSourceKind,  // Node | Subscription | Group
    id: String,
    weight: u32,
}
```

This type only appears inside `NodeGroupInfo.sources`. No semantic ambiguity.

### 1.4 Edge Matrix

| From | To | Allowed |
|---|---|---|
| `start` | `rule` | Yes |
| `start` | anything else | No |
| `rule` | `node_group`, `builtin` | Yes |
| `rule` | anything else | No |
| `end` | `node_group`, `builtin` | Yes |
| `end` | anything else | No |
| `node_group`, `builtin` | anything | No (terminal) |

Removed row: `rule → chain`.

### 1.5 Document Defaults

```
nodes: [start_node, default_group ("proxy_01"), direct_builtin, end_node]
edges: [end → direct]
```

Unchanged from V3 — Chain was never in defaults.

---

## 2. Backend Changes

### 2.1 Validation Simplification

**Removed validation rules:**
- `chain_empty`
- `chain_multi_hop_unsupported`
- `chain_source_empty`
- `chain_hop_invalid`
- `chain_self_reference`

**Unchanged:** Rule priority uniqueness, edge validity matrix, source dedup, group cycle detection, name conflicts.

### 2.2 Compilation Simplification

`compile_orchestration()` drops the `chain_outbound()` branch:

```rust
fn compile_orchestration(doc: &OrchestrationDocument) -> Result<CompiledRouting> {
    let sorted_rules = sort_rules_by_priority(doc);
    let mut routing = Vec::new();

    for rule in &sorted_rules {
        let condition = compile_matcher(&rule.matcher);
        let outbound = match resolve_rule_target(doc, rule)? {
            Target::NodeGroup(id) => outbound(&id, &groups),
            Target::Builtin => outbound_direct(),
        };
        routing.push(RoutingRule { condition, outbound });
    }

    let fallback = resolve_end_fallback(doc)?;
    Ok(CompiledRouting { routing, fallback })
}
```

### 2.3 Publish Pipeline Simplification

`prepare_publish_plan()` removes calls to:
- `expand_materialized_chains()` — entire function deleted
- Chain-related `SourceCatalog` tracking

Simplified pipeline:
```
compile_orchestration()
  → expand_document_groups()
    → assign_runtime_group_ids()
      → build PublishedOrchestrationPlan
```

### 2.4 Unchanged

- Atomic publish transaction (SQLite BEGIN/COMMIT)
- Snapshot/rollback on failure
- Pending marker for crash recovery
- Config meta key structure

---

## 3. Migration: V3 → V4

### 3.1 `migrate_orchestration_document()` Logic

```
bump version 3 → 4

for each node in document.nodes:
    if node.kind == "chain":
        1. find all rules whose target points to this chain node
           - if chain has exactly 1 hop and hop.kind == "group"
             and that group points to a flow node_group:
               → rewrite rule.target to that node_group's id
           - otherwise:
               → rewrite rule.target to "direct" (builtin)
               → log warning: "rule <id> chain target degraded to direct"
        2. remove the chain node

for each edge in document.edges:
    if edge.source or edge.target is a removed chain node:
        remove edge

bump version to 4
```

### 3.2 Frontend Migration

`orchestration.ts` `migrateDocument()` implements identical logic. Both frontend and backend handle V3 documents so that loading an old draft works regardless of which side processes first.

### 3.3 Storage Compatibility

| Key | Migration |
|---|---|
| `orchestration.flow.v2.draft` | Migrated on load |
| `orchestration.flow.v2` | Migrated on load, saved as V4 on next publish |
| `orchestration.plan.v2` | No migration needed (plan contains compiled routing, not chain nodes) |

### 3.4 Version Bump

`ORCHESTRATION_VERSION`: `3` → `4`

---

## 4. Frontend Changes

### 4.1 TypeScript Types (`api.ts`)

**Removed:**
- `ChainNodeData`
- `FlowNodeDataChain`
- `GroupSource` (as independent export)

**Simplified:**
```typescript
type FlowNodeData = RuleNodeData | NodeGroupData | BuiltinNodeData | StartNodeData | EndNodeData;
```

### 4.2 Orchestration Logic (`orchestration.ts`)

**Removed:**
- `createChainNode()` factory
- `GroupSource` type definition
- `canConnect()` chain branch
- `validateLocal()` chain checks
- `decorateDocument()` chain decoration
- `sanitizeDocument()` chain normalization
- `migrateDocument()` chain migration (replaced by V3→V4)

**Added:**
- `migrateDocument()` V3→V4 migration (chain removal + rule redirection)

**Version:** `ORCHESTRATION_VERSION = 4`

### 4.3 Page (`+page.svelte`)

**Removed:**
- `createChain()` function
- `updateChain()` function
- `addNodeAt('chain', ...)` branch
- Chain card from left library panel
- `onupdatechain` prop to inspector

### 4.4 Canvas (`OrchestrationCanvas.svelte`)

- Remove chain from `onaddnode` allowed kinds
- Remove chain from `onDrop` handler

### 4.5 Inspector (`OrchestrationInspector.svelte`)

**Added — End node panel:**

When `node.type === 'end'`:
- Dropdown listing all `node_group` and `builtin` nodes as fallback target options
- Current fallback target shown as selected
- Description text: "当没有规则匹配时，流量将转发到此目标"
- On change: calls `onsetendtarget(nodeId, targetId)` — replaces End's outbound edge

**Added — Start node panel:**

When `node.type === 'start'`:
- Read-only display of connected rule count
- Description text: "入口流量从这里进入，按规则优先级分发"

**Removed props:**
- `onupdatechain` — no longer needed

### 4.6 Flow Nodes

- `ChainNode.svelte` — **deleted**
- `apps/web/src/lib/index.ts` — remove ChainNode export

---

## 5. Testing Strategy

### 5.1 Unit Tests (Rust)

| Test | File |
|---|---|
| `migrate_v3_to_v4_removes_chain_nodes` | `chaos-core/tests/orchestration.rs` |
| `migrate_v3_to_v4_rewrites_rule_target` | same |
| `migrate_v3_to_v4_single_hop_to_group_preserved` | same |
| `compile_without_chain_produces_correct_routing` | same |
| `edge_validation_rejects_rule_to_chain` | same |
| `validate_rejects_chain_nodes` | same |

### 5.2 Unit Tests (TypeScript)

| Test | File |
|---|---|
| `migrateDocument removes chain nodes` | `orchestration.test.ts` (update `latencySessionCore.test.ts`) |
| `migrateDocument rewrites rule target from chain to group` | same |
| `canConnect rejects rule → chain connection` | same |
| `createRuleNode target must be node_group or builtin` | same |

### 5.3 Integration Tests (Rust)

| Test | File |
|---|---|
| `publish_flow_without_chain_succeeds` | `chaos-dae/tests/manager.rs` |
| `publish_v3_document_migrates_and_publishes` | same |

### 5.4 E2E Tests (Playwright)

| Test | Description |
|---|---|
| Inspector: End node shows fallback selector | Select End → dropdown appears → change target → edge updates |
| Inspector: Start node shows rule count | Select Start → displays connected rule count |
| Library: Chain node not shown | Node library panel has no Chain entry |

---

## 6. File Change Summary

| File | Action | Lines (est.) |
|---|---|---|
| `crates/chaos-core/src/orchestration.rs` | Modify | −80 / +40 |
| `crates/chaos-api/src/routes/orchestration.rs` | Modify | −60 / +5 |
| `apps/web/src/lib/api.ts` | Modify | −30 / +2 |
| `apps/web/src/lib/orchestration.ts` | Modify | −120 / +40 |
| `apps/web/src/routes/(app)/orchestrate/+page.svelte` | Modify | −50 / +0 |
| `apps/web/src/lib/components/features/OrchestrationCanvas.svelte` | Modify | −10 / +0 |
| `apps/web/src/lib/components/features/OrchestrationInspector.svelte` | Modify | −5 / +60 |
| `apps/web/src/lib/flow/ChainNode.svelte` | Delete | −60 |
| `apps/web/src/lib/index.ts` | Modify | −2 / +0 |
| `design-system/design-system/chaos/MASTER.md` | Modify | −10 / +10 |
| `docs/design-system/MASTER.md` | Modify | −10 / +10 |

Net: approximately **−300 / +150 lines** across 11 files.

---

## 7. Non-Goals

- Multi-hop routing — explicitly not in scope
- List-based UI — keeping the canvas paradigm
- Changes to load-balancing policies or node group semantics
- Changes to the publish lifecycle or atomicity guarantees
- Removing Start/End anchor nodes — they provide clear visual entry/exit points
