# 分流编排 Start/End + Chain 骨架 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add fixed Start/End anchors, configurable fallback via End, and Chain nodes (inspector hops) to 分流编排—without user-facing schema version labels and without pretending multi-hop works on Linux dae.

**Architecture:** Bump internal `ORCHESTRATION_VERSION` to 3. Extend `FlowNodeKind` with `Start`, `End`, `Chain`. Migrate old documents on load (`migrate_orchestration_document`). Compile: rules by priority; single-hop chain collapses; multi-hop publish fails with capability issue; fallback from End’s sole outbound edge. Web mirrors validation/connect/create helpers and new Svelte Flow node components.

**Tech Stack:** Rust (`chaos-core`, `chaos-api`), SvelteKit + `@xyflow/svelte`, existing orchestration meta keys in SQLite.

## Global Constraints

- UI product name only: **分流编排** / existing `flow.*` keys — **never** show V2/V3/schema version to users.
- Spec: `docs/superpowers/specs/2026-07-21-orchestration-start-end-design.md`
- Legal edges only: `start→rule`, `rule→node_group|builtin|chain`, `end→node_group|builtin`.
- End targets: `direct` or `node_group` only (no chain, no reject this slice).
- Multi-hop chain (≥2 hops): draft OK if graph-valid; **publish fails** (`chain_multi_hop_unsupported`); never silent flatten to parallel group.
- Single-hop chain: collapse to that hop’s outbound on compile/publish.
- Start edges do not emit dae rules.
- Fixed node ids: `start`, `end`, `direct`.
- True multi-hop data plane remains out of scope (`docs/platform/orchestration-v3-hops.md`).

## File map

| File | Responsibility |
|------|----------------|
| `crates/chaos-core/src/orchestration.rs` | Schema, migrate, validate, compile, defaults, unit tests |
| `crates/chaos-api/src/routes/orchestration.rs` | Load/save/publish call migrate+normalize; chain hop resource checks; error codes |
| `apps/web/src/lib/api.ts` | DTO unions for start/end/chain; `version: 2 \| 3` |
| `apps/web/src/lib/orchestration.ts` | create/sanitize/decorate/canConnect/validateLocal/autoLayout/migrate client mirror |
| `apps/web/src/lib/flow/StartNode.svelte` | Start canvas node |
| `apps/web/src/lib/flow/EndNode.svelte` | End canvas node |
| `apps/web/src/lib/flow/ChainNode.svelte` | Chain canvas node |
| `apps/web/src/lib/components/features/OrchestrationCanvas.svelte` | Register nodeTypes |
| `apps/web/src/lib/components/features/OrchestrationInspector.svelte` | Start/End/Chain panels; End fallback picker; chain hop editor |
| `apps/web/src/routes/(app)/orchestrate/+page.svelte` | Toolbar add chain; create rule auto-links Start; i18n |
| `locales/en.json`, `locales/zh-CN.json` | Copy + error codes; nav stays 分流编排; no version strings |

---

### Task 1: Core schema — Start / End / Chain + version 3 + migration

**Files:**
- Modify: `crates/chaos-core/src/orchestration.rs`
- Test: same file `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `ORCHESTRATION_VERSION: u32 = 3`
  - `FlowNodeKind::{Start, End, Chain, Rule, NodeGroup, Builtin}`
  - `FlowNodeData.hops: Vec<GroupSource>` (reuse `GroupSource` for hops; empty for non-chain)
  - `pub fn migrate_orchestration_document(doc: OrchestrationDocument) -> OrchestrationDocument`
  - `fn start_node() -> FlowNode`, `fn end_node() -> FlowNode` (ids `start` / `end`)
  - Default document includes start, end, default group, direct; edge `end→direct`

- [ ] **Step 1: Write failing tests for migration and default shape**

Add tests (adjust helper constructors as needed):

```rust
#[test]
fn default_document_has_start_end_and_fallback_edge() {
    let document = OrchestrationDocument::default();
    assert_eq!(document.version, 3);
    assert!(document.nodes.iter().any(|n| n.kind == FlowNodeKind::Start && n.id == "start"));
    assert!(document.nodes.iter().any(|n| n.kind == FlowNodeKind::End && n.id == "end"));
    assert!(document.nodes.iter().any(|n| n.kind == FlowNodeKind::Builtin && n.id == "direct"));
    assert!(document
        .edges
        .iter()
        .any(|e| e.source == "end" && e.target == "direct"));
}

#[test]
fn migrates_v2_document_injects_start_end_and_start_rule_edges() {
    let v2 = OrchestrationDocument {
        version: 2,
        nodes: vec![
            rule("rule-domain", RuleMatcherKind::DomainSuffix, "example.com", Some(1)),
            group(),
            direct_builtin(),
        ],
        edges: vec![FlowEdge::new("r-g", "rule-domain", "group")],
        viewport: FlowViewport::default(),
    };
    let migrated = migrate_orchestration_document(v2);
    assert_eq!(migrated.version, 3);
    assert!(migrated.nodes.iter().any(|n| n.id == "start"));
    assert!(migrated.nodes.iter().any(|n| n.id == "end"));
    assert!(migrated
        .edges
        .iter()
        .any(|e| e.source == "start" && e.target == "rule-domain"));
    assert!(migrated
        .edges
        .iter()
        .any(|e| e.source == "end" && e.target == "direct"));
    // original rule target preserved
    assert!(migrated
        .edges
        .iter()
        .any(|e| e.source == "rule-domain" && e.target == "group"));
}
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test -p chaos-core migrates_v2_document -- --nocapture`  
Expected: compile error or FAIL (no `migrate_orchestration_document`, version still 2).

- [ ] **Step 3: Implement schema + migration (minimal)**

1. Set `ORCHESTRATION_VERSION = 3`.
2. Extend enum:

```rust
pub enum FlowNodeKind {
    Start,
    End,
    Rule,
    NodeGroup,
    Builtin,
    Chain,
}
```

3. Add `hops: Vec<GroupSource>` to `FlowNodeData` with `#[serde(default)]`. Update custom `Serialize` for `FlowNodeData`:
   - Start/End: serialize empty object fields as needed (or only kind-specific); prefer:
     - chain: `name` + `hops`
     - start/end: minimal (no matcher/sources)
   - Keep rule/group/builtin serialization behavior.
4. `start_node()` / `end_node()` positions: start ~`(40, 200)`, end ~`(760, 280)` (layout can refine later).
5. `OrchestrationDocument::default()`:

```rust
nodes: vec![start_node(), default_group(), direct_builtin(), end_node()],
edges: vec![FlowEdge::new("end-direct", "end", "direct")],
```

6. `migrate_orchestration_document`:

```rust
pub fn migrate_orchestration_document(mut document: OrchestrationDocument) -> OrchestrationDocument {
    // If already v3 and has start+end, still ensure invariants (idempotent).
    if !document.nodes.iter().any(|n| n.kind == FlowNodeKind::Start) {
        document.nodes.push(start_node());
    }
    if !document.nodes.iter().any(|n| n.kind == FlowNodeKind::End) {
        document.nodes.push(end_node());
    }
    if !document.nodes.iter().any(|n| {
        n.kind == FlowNodeKind::Builtin && n.data.builtin == Some(BuiltinKind::Direct)
    }) {
        document.nodes.push(direct_builtin());
    }
    // Ensure start → each rule
    let rule_ids: Vec<String> = document
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Rule)
        .map(|n| n.id.clone())
        .collect();
    for rule_id in rule_ids {
        let has = document
            .edges
            .iter()
            .any(|e| e.source == "start" && e.target == rule_id);
        if !has {
            document.edges.push(FlowEdge::new(
                &format!("start-{rule_id}"),
                "start",
                &rule_id,
            ));
        }
    }
    // Ensure end has exactly one outbound to terminal; default direct if missing
    let end_outs: Vec<_> = document
        .edges
        .iter()
        .filter(|e| e.source == "end")
        .cloned()
        .collect();
    if end_outs.is_empty() {
        // Optional: if legacy had only hard-coded fallback, attach direct.
        // If a single non-rule terminal is unambiguous later, keep direct for safety.
        document
            .edges
            .push(FlowEdge::new("end-direct", "end", "direct"));
    }
    document.version = ORCHESTRATION_VERSION;
    document
}
```

7. Update existing tests that assert `version == 2` or default node list; include start/end edges where validation will require them (Task 2). For this task, migration + default tests must pass; older compile tests may temporarily need start/end nodes/edges if you run full suite—prefer updating them in Task 2, but fix compile breaks now so `cargo test -p chaos-core` stays green after Task 2.

- [ ] **Step 4: Run migration tests**

Run: `cargo test -p chaos-core default_document_has_start_end -- --nocapture`  
Run: `cargo test -p chaos-core migrates_v2_document -- --nocapture`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-core/src/orchestration.rs
git commit -m "feat(core): orchestration v3 schema with start/end/chain migration"
```

---

### Task 2: Core validate + compile (fallback from End, chain collapse)

**Files:**
- Modify: `crates/chaos-core/src/orchestration.rs` (`validate_orchestration`, `compile_orchestration`, `outbound`)
- Test: same module

**Interfaces:**
- Consumes: Task 1 kinds + `hops` + `migrate_orchestration_document`
- Produces:
  - Graph codes: `start_required`, `end_required`, `rule_start_required`, `end_target_required`, `end_target_invalid`, `chain_empty` (graph or runtime—use **graph** for empty chain as rule target on publish path via `dae_compatible`/runtime: prefer **runtime** for capability: `chain_multi_hop_unsupported`, `chain_empty`)
  - `compile_orchestration` fallback from `end→target`
  - `outbound` handles `Chain` (1 hop collapse)

- [ ] **Step 1: Write failing tests**

```rust
fn base_terminals() -> Vec<FlowNode> {
    vec![start_node(), end_node(), direct_builtin(), group()]
}

#[test]
fn compile_uses_end_fallback_group() {
    let mut nodes = base_terminals();
    nodes.push(rule(
        "rule-a",
        RuleMatcherKind::DomainSuffix,
        "example.com",
        Some(1),
    ));
    let document = OrchestrationDocument {
        version: 3,
        nodes,
        edges: vec![
            FlowEdge::new("s-r", "start", "rule-a"),
            FlowEdge::new("r-d", "rule-a", "direct"),
            FlowEdge::new("e-g", "end", "group"),
        ],
        viewport: FlowViewport::default(),
    };
    // group needs valid sources for dae_compatible; use group() helper with node source
    let compiled = document.compile().expect("compile");
    assert_eq!(compiled.fallback, dae_identifier("Proxy")); // group() name "Proxy"
}

#[test]
fn single_hop_chain_collapses_on_compile() {
    let chain = FlowNode {
        id: "chain-1".into(),
        kind: FlowNodeKind::Chain,
        position: FlowPosition { x: 500.0, y: 0.0 },
        data: FlowNodeData {
            name: "via".into(),
            hops: vec![GroupSource::Group {
                id: "group".into(), // resolved by target node id in outbound? Spec: hop is inventory source.
                // For collapse of hop kind Group with id pointing at flow node_group id:
                weight: 1,
            }],
            ..FlowNodeData::default()
        },
    };
    // Prefer hop → node_group **flow node id** resolved in compile:
    // If hop is Group { id } matching a node_group node id, outbound = dae_identifier(name).
    // If hop is Node { id }, publication expands later; for compile outbound use a synthetic approach:
    // Spec says 1 hop collapses to that hop's outbound. For GroupSource::Group where id is flow node id of node_group:
    let mut nodes = base_terminals();
    // Fix chain hop to reference flow group node id "group"
    let mut chain = chain;
    chain.data.hops = vec![GroupSource::Group {
        id: "group".into(),
        weight: 1,
    }];
    nodes.push(chain);
    nodes.push(rule(
        "rule-a",
        RuleMatcherKind::DomainSuffix,
        "example.com",
        Some(1),
    ));
    let document = OrchestrationDocument {
        version: 3,
        nodes,
        edges: vec![
            FlowEdge::new("s-r", "start", "rule-a"),
            FlowEdge::new("r-c", "rule-a", "chain-1"),
            FlowEdge::new("e-d", "end", "direct"),
        ],
        viewport: FlowViewport::default(),
    };
    let compiled = document.compile().unwrap();
    assert_eq!(compiled.conditions[0].outbound, dae_identifier("Proxy"));
    assert_eq!(compiled.fallback, "direct");
}

#[test]
fn multi_hop_chain_is_not_dae_compatible() {
    let mut nodes = base_terminals();
    nodes.push(FlowNode {
        id: "chain-1".into(),
        kind: FlowNodeKind::Chain,
        position: FlowPosition { x: 500.0, y: 0.0 },
        data: FlowNodeData {
            hops: vec![
                GroupSource::Node {
                    id: "node-1".into(),
                    weight: 1,
                },
                GroupSource::Node {
                    id: "node-2".into(),
                    weight: 1,
                },
            ],
            ..FlowNodeData::default()
        },
    });
    nodes.push(rule(
        "rule-a",
        RuleMatcherKind::DomainSuffix,
        "example.com",
        Some(1),
    ));
    let document = OrchestrationDocument {
        version: 3,
        nodes,
        edges: vec![
            FlowEdge::new("s-r", "start", "rule-a"),
            FlowEdge::new("r-c", "rule-a", "chain-1"),
            FlowEdge::new("e-d", "end", "direct"),
        ],
        viewport: FlowViewport::default(),
    };
    let report = document.validate();
    assert!(report.valid, "graph may be valid");
    assert!(!report.dae_compatible);
    assert!(report
        .issues
        .iter()
        .any(|i| i.code == "chain_multi_hop_unsupported"));
    assert!(document.compile().is_err());
}

#[test]
fn rejects_rule_without_start_edge_and_illegal_edges() {
    let mut nodes = base_terminals();
    nodes.push(rule(
        "rule-a",
        RuleMatcherKind::DomainSuffix,
        "example.com",
        Some(1),
    ));
    let document = OrchestrationDocument {
        version: 3,
        nodes,
        edges: vec![
            FlowEdge::new("r-d", "rule-a", "direct"),
            FlowEdge::new("e-d", "end", "direct"),
            // missing start→rule-a
        ],
        viewport: FlowViewport::default(),
    };
    let report = document.validate();
    assert!(!report.valid);
    assert!(report
        .issues
        .iter()
        .any(|i| i.code == "rule_start_required"));
}
```

**Hop collapse rule (lock this):**  
When compiling a chain target:
- `hops.len() == 0` → runtime issue `chain_empty`, not dae_compatible.
- `hops.len() == 1` → resolve hop:
  - `GroupSource::Group { id }` if `id` equals a `node_group` **flow node id**, outbound = `dae_identifier(that.name)`; if `id` is catalog group only, treat as invalid at publish (resource) — for core compile, require hop group id to match a flow `node_group` id **or** use node hop only for later publish expansion. **Simplest ship rule:** single hop must be `Group` pointing at a **flow** `node_group` id, or hop is ignored and we look up node_group by id; `Node`/`Subscription` single hop is allowed for **publish expansion** in API but compile outbound uses a temporary runtime group name only at publish.  
  - For **core unit tests**, use hop `Group { id: flow_node_group_id }`.
  - `Node`/`Subscription` 1-hop: mark `dae_compatible` only if API publish can expand; core compile may emit outbound placeholder — **prefer:** core compile requires 1-hop chain hop to resolve to `node_group` flow node or `direct` builtin id via special case. If hop is `Node`, set outbound to a stable synthetic only in publish path.  
  **Implementer decision locked here:**  
  - Compile `outbound` for chain: only collapse when hop is `Group` whose `id` matches a `FlowNodeKind::NodeGroup` node id, OR hop is not used and chain wrongly targeted—else runtime `chain_hop_unresolved`.  
  - API publish already expands groups from plan; for 1-hop `Node`/`Subscription`, **Task 3** expands chain into a published group membership then uses that group name as outbound. Core compile for 1-hop Node may return Err/runtime until publish prepares groups—**simpler:** document that **this slice only supports chain hops of kind `group` (flow node_group id) and optionally treat Node/Subscription as runtime-only with publish building an ephemeral group**. Match GroupSource already used on node_group.sources.  
  - **Ship:** hops reuse `GroupSource`; 1-hop collapse in `outbound_from_chain(document, chain_node)`:
    1. If hop `Group { id }` and nodes map has NodeGroup with that id → `dae_identifier(name)`.
    2. If hop `Group { id }` equals something else → runtime `chain_hop_unresolved`.
    3. If hop `Node` or `Subscription` → runtime `chain_hop_needs_publish_expand` **or** for compile-only tests use Group hops only; API publish (Task 3) materializes a runtime node_group from chain hops like group sources then uses that group’s dae name.  
  **Final lock for implementer:** On compile, if chain hops are exactly one `Group` referencing a flow `node_group` id, collapse. If hops are Node/Subscription (any count), set runtime issue `chain_requires_materialization` and do not set dae_compatible until API materializes—**too heavy.**  

  **YAGNI lock:** Chain hops in this slice are **only** `GroupSource::Group` with **flow node_group id** for collapse, **plus** allow Node/Subscription in data model for UI inventory but **publish** builds one ephemeral runtime group from all hops (same as node_group sources flatten) **only when hops.len()==1** OR when all hops flatten to nodes for a **single** outbound group (parallel selection, NOT multi-hop). Spec says multi-hop ≥2 fails. So:
  - hops.len()>=2 → `chain_multi_hop_unsupported` (even if all Node).
  - hops.len()==1 → materialize like one group source into outbound name at publish; compile can use: if Group→flow node_group, dae name; if Node/Subscription, outbound = chain’s `dae_identifier(chain.data.name)` and publish ensures a runtime group with that name containing the hop—**align with node_group compile** which uses group name as outbound.

  **Practical implementer path:**  
  - `outbound(chain)` = `dae_identifier(&chain.data.name)` if name non-empty, else error `invalid_chain_name`.  
  - Publish (Task 3) creates runtime group from chain hops (same membership expansion as node_group.sources) when hops.len()==1 OR when hops.len()>=2 reject before.  
  - Wait—spec says 1 hop collapses to **that hop’s** outbound, not chain name. So for hop Group→flow group, use that group’s name; for hop Node, publish creates members on a runtime group named from chain.  

  Use this algorithm in core:

```rust
fn chain_outbound(nodes: &HashMap<&str, &FlowNode>, chain: &FlowNode) -> Result<String, &'static str> {
    match chain.data.hops.as_slice() {
        [] => Err("chain_empty"),
        [_] if chain.data.hops.len() >= 2 => unreachable!(),
        [h] => match h {
            GroupSource::Group { id, .. } => {
                let Some(n) = nodes.get(id.as_str()) else { return Err("chain_hop_unresolved") };
                if n.kind != FlowNodeKind::NodeGroup { return Err("chain_hop_unresolved") };
                Ok(dae_identifier(&n.data.name))
            }
            // Node/Subscription: use chain name as outbound key; publish fills members
            GroupSource::Node { .. } | GroupSource::Subscription { .. } => {
                let name = dae_identifier(&chain.data.name);
                if name.is_empty() || normalized_dae_identifier(&chain.data.name).is_none() {
                    return Err("invalid_chain_name");
                }
                Ok(name)
            }
        },
        _ => Err("chain_multi_hop_unsupported"),
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test -p chaos-core compile_uses_end_fallback -- --nocapture`  
Expected: FAIL (fallback still `"direct"`).

- [ ] **Step 3: Implement validation + compile**

Update `validate_orchestration`:

1. After node map built: require exactly one Start, one End, one Direct (codes `start_required`, `end_required`, `direct_required`).
2. Edge legality:

```rust
fn edge_allowed(source: &FlowNode, target: &FlowNode) -> bool {
    match source.kind {
        FlowNodeKind::Start => target.kind == FlowNodeKind::Rule,
        FlowNodeKind::Rule => matches!(
            target.kind,
            FlowNodeKind::NodeGroup | FlowNodeKind::Builtin | FlowNodeKind::Chain
        ),
        FlowNodeKind::End => matches!(
            target.kind,
            FlowNodeKind::NodeGroup | FlowNodeKind::Builtin
        ),
        _ => false,
    }
}
```

3. Per-node:
   - **Rule:** exactly one outbound; exactly one inbound from start (`rule_start_required` if not); existing matcher/priority checks.
   - **NodeGroup / Builtin / Chain:** no outbound (`group_terminal_required` / `builtin_terminal_required` / `chain_terminal_required`).
   - **Start:** no inbound required; may have many outbounds to rules only.
   - **End:** exactly one outbound to group|builtin (`end_target_required` / `end_target_invalid`).
   - **Chain:** validate name like group if Node/Subscription hop needs name; hops unique; for hops.len()>=2 push **runtime** `chain_multi_hop_unsupported`; hops.is_empty() runtime `chain_empty` if any rule targets this chain.
4. Incoming map for rules from start.

Update `compile_orchestration`:

```rust
// find end node and its single edge
let end = document.nodes.iter().find(|n| n.kind == FlowNodeKind::End).unwrap();
let end_edge = document.edges.iter().find(|e| e.source == end.id).unwrap();
let fallback_target = nodes[end_edge.target.as_str()];
let fallback = outbound(fallback_target); // not chain

// rule targets: if chain, use chain_outbound
```

Update `outbound` for NodeGroup/Builtin only; call `chain_outbound` for chains.

Update all existing tests to version 3 + start/end edges so suite passes.

- [ ] **Step 4: Run full core tests**

Run: `cargo test -p chaos-core`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-core/src/orchestration.rs
git commit -m "feat(core): validate start/end topology and compile end fallback"
```

---

### Task 3: API load/save/publish integration

**Files:**
- Modify: `crates/chaos-api/src/routes/orchestration.rs`
- Test: existing tests in that file + new cases if present; else core coverage + manual curl

**Interfaces:**
- Consumes: `migrate_orchestration_document`, version 3 validation
- Produces: GET/PUT always return migrated docs; publish fails multi-hop with validation issues surfaced as today (`orchestration_invalid`); optional map first issue code into message later—keep existing error mapping if already generic

- [ ] **Step 1: Call migrate in normalize path**

```rust
fn normalize_document(mut document: OrchestrationDocument) -> OrchestrationDocument {
    document = chaos_core::orchestration::migrate_orchestration_document(document);
    // existing runtime_group_id rewrite for Group sources...
    // also rewrite chain hops Group ids the same way as node_group sources
    document
}
```

Apply hop id rewrite for `FlowNodeKind::Chain` the same as sources on groups.

- [ ] **Step 2: Limits / version check**

`check_document_limits`: allow `version == ORCHESTRATION_VERSION` (3). After migrate, version is 3.

Count hops toward `MAX_FLOW_SOURCES` similarly:

```rust
let source_count: usize = document.nodes.iter().map(|node| {
    node.data.sources.len() + node.data.hops.len()
}).sum();
```

- [ ] **Step 3: Publish materialization**

Where publish builds `PublishedGroup` from node_group nodes, also for each **Chain** that is a rule target with `hops.len()==1`:
- Materialize membership from hop (Node/Subscription/Group) using same expansion as group sources.
- Outbound name from compile (already collapsed).

If compile fails on multi-hop, publish returns `orchestration_invalid` (existing). Ensure validation issues include `chain_multi_hop_unsupported`.

Resource existence checks for chain hops: same as group sources (`orchestration_references` patterns).

- [ ] **Step 4: Fix API unit tests**

Update fixtures that build `OrchestrationDocument { version: 2, ...}` to version 3 with start/end edges. Run:

```bash
cargo test -p chaos-api orchestration
cargo test -p chaos-api
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-api/src/routes/orchestration.rs
git commit -m "feat(api): migrate orchestration docs and publish chain single-hop"
```

---

### Task 4: Web types + orchestration.ts helpers

**Files:**
- Modify: `apps/web/src/lib/api.ts`
- Modify: `apps/web/src/lib/orchestration.ts`

**Interfaces:**
- Produces TS:

```ts
export type OrchestrationStartNodeDto = OrchestrationNodePresentation & {
  id: string;
  type: 'start';
  position: { x: number; y: number };
  data: Record<string, never> | { route_count?: number };
};
export type OrchestrationEndNodeDto = /* type: 'end'; data: { route_count?: number } */;
export type OrchestrationChainData = {
  name: string;
  hops: OrchestrationSource[];
  route_count?: number;
  target_name?: string;
};
export type OrchestrationChainNodeDto = /* type: 'chain'; data: OrchestrationChainData */;
export type OrchestrationNodeDto =
  | OrchestrationRuleNodeDto
  | OrchestrationNodeGroupDto
  | OrchestrationBuiltinNodeDto
  | OrchestrationStartNodeDto
  | OrchestrationEndNodeDto
  | OrchestrationChainNodeDto;
export type OrchestrationDocument = {
  version: 2 | 3;
  nodes: OrchestrationNodeDto[];
  edges: OrchestrationEdgeDto[];
  viewport: { x: number; y: number; zoom: number };
  needs_republish?: boolean;
};
```

- Produces functions:
  - `createStartNode`, `createEndNode`, `createChainNode`
  - `migrateDocument(doc): OrchestrationDocument` (mirror Rust)
  - `canConnect` legal matrix
  - `setEndTarget(endId, targetId, nodes, edges)`
  - `validateLocal` version 3 rules
  - `autoLayout` zones: start x=40, rules x=280, outbounds x=560, end x=800

- [ ] **Step 1: Extend api.ts types** as above.

- [ ] **Step 2: Implement helpers in orchestration.ts**

```ts
export function canConnect(connection, nodes, edges): boolean {
  // start→rule (max one start inbound per rule)
  // rule→node_group|builtin|chain (replace: allow connect if no other out, or caller replaces)
  // end→node_group|builtin (only one end out — createConnectionEdge should replace)
  ...
}

export function createConnectionEdge(...) {
  // If source is rule or end, drop existing outs from that source before add (match setRuleTarget)
}
```

Update `setRuleTarget` to allow chain targets via `canConnect`.

Add `setEndTarget(targetId, nodes, edges)` mirroring setRuleTarget for source `end`.

`validateLocal`: `document.version !== 3` → after migrate client-side, always sanitize to 3; if not 3 and not migratable, `unsupported_version`. Prefer: `sanitizeDocument` always emits `version: 3` and `decorateDocument` calls migrate.

`createRuleNode`: callers must also add start edge—export:

```ts
export function addRuleWithStart(
  document: OrchestrationDocument,
  position: { x: number; y: number }
): OrchestrationDocument
```

- [ ] **Step 3: Typecheck**

Run: `cd apps/web && pnpm exec svelte-check --threshold error`  
(or project’s usual `pnpm check` if defined in package.json)

Expected: no errors related to OrchestrationNodeDto exhaustiveness—fix switch sites.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/api.ts apps/web/src/lib/orchestration.ts
git commit -m "feat(web): orchestration types and helpers for start/end/chain"
```

---

### Task 5: Canvas node components + register types

**Files:**
- Create: `apps/web/src/lib/flow/StartNode.svelte`
- Create: `apps/web/src/lib/flow/EndNode.svelte`
- Create: `apps/web/src/lib/flow/ChainNode.svelte`
- Modify: `apps/web/src/lib/components/features/OrchestrationCanvas.svelte`

**Interfaces:**
- Match BuiltinNode styling tokens (`--line-strong`, `--surface-subtle`, handles).
- Start: source handle only (Right).
- End: source handle only (Right) for fallback edge to terminal—**or** target on left + source right. Spec: `end → terminal`, so End has **source** handle on right; terminals keep target left.
- Chain: target left (from rule), no source out.

- [ ] **Step 1: Create StartNode.svelte**

```svelte
<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import { t } from '$lib/i18n.svelte';
  let { selected = false }: NodeProps = $props();
</script>
<div class="flow-node start-node" class:selected>
  <strong>{t('flow.start')}</strong>
  <small>{t('flow.start.hint')}</small>
  <Handle type="source" position={Position.Right} id="out" />
</div>
<!-- styles similar to BuiltinNode; pill/capsule border-radius -->
```

- [ ] **Step 2: Create EndNode.svelte** — label `t('flow.end')` / `t('flow.fallback')`.

- [ ] **Step 3: Create ChainNode.svelte** — show name + hop count.

- [ ] **Step 4: Register in OrchestrationCanvas.svelte**

```ts
const nodeTypes = {
  rule: RuleNode,
  node_group: GroupNode,
  builtin: BuiltinNode,
  start: StartNode,
  end: EndNode,
  chain: ChainNode
};
```

- [ ] **Step 5: Visual smoke**

Run: `pnpm dev` / open 分流编排 — nodes render without XYFlow unknown-type warnings.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/flow/StartNode.svelte apps/web/src/lib/flow/EndNode.svelte apps/web/src/lib/flow/ChainNode.svelte apps/web/src/lib/components/features/OrchestrationCanvas.svelte
git commit -m "feat(web): start, end, and chain flow nodes"
```

---

### Task 6: Inspector + orchestrate page UX

**Files:**
- Modify: `apps/web/src/lib/components/features/OrchestrationInspector.svelte`
- Modify: `apps/web/src/routes/(app)/orchestrate/+page.svelte`
- Modify: `locales/en.json`, `locales/zh-CN.json`

**Interfaces:**
- Inspector branches for `start` | `end` | `chain`.
- End: select fallback among `direct` + node_groups → `setEndTarget`.
- Chain: name field; hop list add/remove/reorder using inventory (nodes/subscriptions/groups) like group sources; banner for multi-hop.
- Page toolbar: add Rule (with start edge), Group, Chain; never delete start/end/direct.
- On load: `setDocument(migrateDocument(apiDoc))`.
- Publish button disabled or error when `validateLocal` has runtime issues including `chain_multi_hop_unsupported`.

- [ ] **Step 1: i18n keys** (no version strings)

`locales/zh-CN.json` / `en.json`:

```json
"nav.orchestrate": "分流编排",
"flow.title": "分流编排",
"flow.start": "开始",
"flow.start.hint": "规则入口",
"flow.end": "结束",
"flow.end.hint": "未匹配流量的默认出口",
"flow.addChain": "添加链路",
"flow.chain": "链路",
"flow.chain.hops": "跳数列表",
"flow.chain.multiHopHint": "当前引擎仅支持单跳出口；多跳可保存草稿，发布需引擎支持。",
"flow.chain.singleHopHint": "将按普通出口发布。",
"flow.chain.emptyHint": "请至少添加一跳。",
"error.chain_multi_hop_unsupported": "多跳链路在当前引擎下不能发布",
"error.chain_empty": "链路还没有跳点",
"error.rule_start_required": "规则需要连接到开始节点",
"error.end_target_required": "请设置默认出口",
"error.start_required": "缺少开始节点",
"error.end_required": "缺少结束节点"
```

English equivalents without “V3”. Keep `flow.fallback` as default exit label.

- [ ] **Step 2: Inspector UI**

For `node.type === 'end'`: dropdown of terminals, on change call parent callback `onSetEndTarget(id)`.

For `node.type === 'chain'`: reuse patterns from group source editor (simplified).

For `start`: read-only connected rule count from edges.

- [ ] **Step 3: Page wiring**

- `AddableNodeKind = 'rule' | 'node_group' | 'chain'`
- create rule → push node + `start→rule` edge
- delete node → strip edges; block delete start/end/direct
- `validateLocal` before save/publish; save allows graph-valid with runtime warnings; publish requires `dae_compatible`

- [ ] **Step 4: Manual QA checklist**

1. Load old draft (if any) → Start/End present, rules linked.
2. End → group → publish → dae config `fallback: <group>`.
3. Chain 1 hop group → publish OK.
4. Chain 2 hops → save OK, publish error humanized.
5. UI shows no “V2/V3”.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/components/features/OrchestrationInspector.svelte \
  apps/web/src/routes/\(app\)/orchestrate/+page.svelte \
  locales/en.json locales/zh-CN.json
git commit -m "feat(web): inspector and page UX for start/end/chain orchestration"
```

---

### Task 7: Regression suite + acceptance pass

**Files:**
- Modify tests as needed under `crates/chaos-core`, `crates/chaos-api`
- Optional: small unit tests for `orchestration.ts` if project has vitest—**skip if none**

- [ ] **Step 1: Run backend tests**

```bash
cargo test -p chaos-core
cargo test -p chaos-api
```

Expected: PASS.

- [ ] **Step 2: Run frontend check**

```bash
cd apps/web && pnpm install && pnpm check
```

Expected: PASS (or project-equivalent).

- [ ] **Step 3: Spec acceptance mapping**

| Spec # | Verify |
|--------|--------|
| Migration | Task 1 tests + GET returns start/end |
| Graph validation | Task 2 tests |
| Fallback publish | Task 3 + manual/render assert |
| Chain 1 hop | Task 2/3 |
| Chain ≥2 publish fail | Task 2 + API |
| UI no version | Task 6 copy review |
| Regression | full cargo test |

- [ ] **Step 4: Final commit if fixes**

```bash
git add -A
git commit -m "test: acceptance for orchestration start/end/chain slice"
```

---

## Spec coverage self-check

| Spec requirement | Task |
|------------------|------|
| Start/End anchors | 1, 5, 6 |
| Configurable fallback via End | 2, 3, 6 |
| Chain hops inspector | 4, 6 |
| Legal edge matrix | 2, 4 |
| Migrate old docs | 1, 3, 4 |
| 1-hop collapse / ≥2 gate | 2, 3 |
| No user-facing version | 6 Global Constraints |
| i18n human errors | 6 |
| Non-goal true multi-hop engine | documented; publish gate only |

## Placeholder scan

No TBD steps; hop collapse algorithm locked under Task 2.

## Type consistency

- Rust kinds: `start`/`end`/`chain` snake_case serde = TS `type` strings.
- `ORCHESTRATION_VERSION = 3` = TS sanitized `version: 3`.
- Hops: `Vec<GroupSource>` = `OrchestrationSource[]`.
- Error codes shared strings listed in Task 6.
