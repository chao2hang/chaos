# Orchestration V4 Simplify Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Chain node type from orchestration, complete Inspector with End/Start panels, and migrate V3→V4 documents.

**Architecture:** Backend-first approach: remove Chain types from Rust core (validation, compilation, migration), clean API routes, then mirror all changes in TypeScript (types, orchestration logic, UI components). Each task produces independently testable changes.

**Tech Stack:** Rust (chaos-core, chaos-api), TypeScript (Svelte 5 runes, @xyflow/svelte)

## Global Constraints

- Version bump: `ORCHESTRATION_VERSION` from 3 to 4
- V3 documents auto-migrate on load (frontend and backend)
- Node kinds: 5 (Start, Rule, NodeGroup, Builtin, End) — remove Chain
- Edge matrix: `rule→node_group|builtin` (remove `rule→chain`)
- No breaking changes to published plans — V3 plans in `META_ORCHESTRATION_PLAN` are unaffected
- Inspector must have End node fallback selector and Start node info panel

---

### Task 1: Rust Core — Remove Chain from Types, Validation, and Edge Matrix

**Files:**
- Modify: `crates/chaos-core/src/orchestration.rs`

**Interfaces:**
- Produces: `FlowNodeKind` without `Chain` variant; `FlowNodeData` without `hops` field; `edge_allowed()` without chain row; `ORCHESTRATION_VERSION = 4`

- [ ] **Step 1: Bump version and remove Chain variant**

In `crates/chaos-core/src/orchestration.rs`, line 10:
```rust
pub const ORCHESTRATION_VERSION: u32 = 4;
```

Lines 46-55, remove `Chain` from `FlowNodeKind`:
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeKind {
    Start,
    End,
    Rule,
    NodeGroup,
    Builtin,
}
```

- [ ] **Step 2: Remove `hops` from `FlowNodeData`**

Lines 80-98, remove `hops` field:
```rust
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FlowNodeData {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_policy")]
    pub policy: String,
    #[serde(default)]
    pub sources: Vec<GroupSource>,
    #[serde(default)]
    pub matcher: Option<RuleMatcher>,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default)]
    pub builtin: Option<BuiltinKind>,
    #[serde(default)]
    pub runtime_group_id: Option<String>,
}
```

Lines 126-139, remove `hops` from Default:
```rust
impl Default for FlowNodeData {
    fn default() -> Self {
        Self {
            name: String::new(),
            policy: default_policy(),
            sources: vec![],
            matcher: None,
            priority: None,
            builtin: None,
            runtime_group_id: None,
        }
    }
}
```

Lines 100-124, update Serialize impl to remove `hops`:
```rust
impl Serialize for FlowNodeData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut data = serializer.serialize_struct("FlowNodeData", 6)?;
        if let Some(matcher) = &self.matcher {
            data.serialize_field("matcher", matcher)?;
            if let Some(priority) = self.priority {
                data.serialize_field("priority", &priority)?;
            }
        } else if let Some(builtin) = self.builtin {
            data.serialize_field("builtin", &builtin)?;
        } else {
            data.serialize_field("name", &self.name)?;
            data.serialize_field("policy", &self.policy)?;
            data.serialize_field("sources", &self.sources)?;
            if let Some(runtime_group_id) = &self.runtime_group_id {
                data.serialize_field("runtime_group_id", runtime_group_id)?;
            }
        }
        data.end()
    }
}
```

- [ ] **Step 3: Remove chain validation from `validate_orchestration()`**

Lines 440-457, remove the `FlowNodeKind::Chain` match arm and the `validate_chain` function call:
```rust
// DELETE lines 440-457 (the entire Chain match arm)
```

Lines 596-640, delete the entire `validate_chain` function.

- [ ] **Step 4: Update `edge_allowed()`**

Lines 497-509, remove chain from edge matrix:
```rust
fn edge_allowed(source: &FlowNode, target: &FlowNode) -> bool {
    match source.kind {
        FlowNodeKind::Start => target.kind == FlowNodeKind::Rule,
        FlowNodeKind::Rule => matches!(
            target.kind,
            FlowNodeKind::NodeGroup | FlowNodeKind::Builtin
        ),
        FlowNodeKind::End => {
            matches!(target.kind, FlowNodeKind::NodeGroup | FlowNodeKind::Builtin)
        }
        FlowNodeKind::NodeGroup | FlowNodeKind::Builtin => false,
    }
}
```

- [ ] **Step 5: Remove `chain_outbound()` and update `outbound()`**

Delete lines 784-792 (the entire `chain_outbound` function).

Lines 775-783, update `outbound` to remove Chain from unreachable:
```rust
fn outbound(node: &FlowNode) -> String {
    match node.kind {
        FlowNodeKind::Builtin => "direct".into(),
        FlowNodeKind::NodeGroup => dae_identifier(&node.data.name),
        FlowNodeKind::Start | FlowNodeKind::End | FlowNodeKind::Rule => {
            unreachable!("node kind cannot be a terminal outbound")
        }
    }
}
```

- [ ] **Step 6: Compile and run existing tests**

Run: `cargo test -p chaos-core --lib orchestration`
Expected: FAIL — chain-related tests reference deleted code, need updating in Task 4.

- [ ] **Step 7: Commit**

```bash
git add crates/chaos-core/src/orchestration.rs
git commit -m "refactor(core): remove Chain from orchestration types and validation"
```

---

### Task 2: Rust Core — Add V3→V4 Migration

**Files:**
- Modify: `crates/chaos-core/src/orchestration.rs`

**Interfaces:**
- Consumes: `OrchestrationDocument` with `version` field (might be 3)
- Produces: `migrate_orchestration_document()` handles V3→V4 by removing chain nodes and rewriting rule targets

- [ ] **Step 1: Add V3→V4 migration block to `migrate_orchestration_document()`**

After the existing migration logic (before `document.version = ORCHESTRATION_VERSION;` at line 316), insert:

```rust
    // V3 → V4: remove chain nodes
    if document.version == 3 {
        let chain_ids: HashSet<_> = document
            .nodes
            .iter()
            .filter(|node| node.kind == FlowNodeKind::Chain)
            .map(|node| node.id.clone())
            .collect();

        if !chain_ids.is_empty() {
            // Rewrite rules that target chain nodes
            for edge in document.edges.iter_mut() {
                if chain_ids.contains(&edge.target) {
                    let chain_node = document
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.target)
                        .unwrap();
                    // If chain has exactly 1 hop pointing to a flow node_group, use that
                    let fallback_target = match chain_node.data.hops.as_slice() {
                        [GroupSource::Group { id, .. }] => {
                            if document
                                .nodes
                                .iter()
                                .any(|n| n.id == *id && n.kind == FlowNodeKind::NodeGroup)
                            {
                                id.clone()
                            } else {
                                "direct".to_string()
                            }
                        }
                        _ => "direct".to_string(),
                    };
                    edge.target = fallback_target;
                }
            }

            // Remove chain nodes and their edges
            document
                .edges
                .retain(|edge| !chain_ids.contains(&edge.source) && !chain_ids.contains(&edge.target));
            document
                .nodes
                .retain(|node| node.kind != FlowNodeKind::Chain);
        }
    }
```

Note: `FlowNodeKind::Chain` is still referenced here for migration purposes. The compiler will error because we removed it in Task 1. We need to keep the `Chain` variant or handle this differently.

Actually — the cleanest approach: keep the `Chain` variant in `FlowNodeKind` but gate it behind a migration-only usage pattern, OR use a different approach.

Simpler approach: Instead of matching on `FlowNodeKind::Chain`, match on the serde tag string `"chain"` in the migration. But the type is already deserialized at this point and `Chain` variant was removed.

**Resolution:** Add back `Chain` as a variant but handle it in the full match. Actually, the simplest way: since V3 documents are deserialized into the current `FlowNodeKind` type, and we just removed `Chain`, serde will fail to deserialize V3 documents containing chain nodes.

**Better approach:** Deserialize `kind` as a `String` during migration, or add a fallback in the serde deserializer. However, the simplest fix is to handle this BEFORE deserialization, or use `#[serde(other)]`.

**Recommended:** Add `#[serde(other)]` to `FlowNodeKind` so unknown variants (like `"chain"`) are discarded rather than causing errors. But this complicates the migration.

**Simplest fix:** Read the raw JSON before deserializing, strip chain nodes during migration. OR: keep the Chain variant but only for deserialization, immediately removing it in migrate.

Let's keep the Variant with a clear comment:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeKind {
    Start,
    End,
    Rule,
    NodeGroup,
    Builtin,
    /// Legacy V3 variant — removed during migration.
    #[serde(rename = "chain")]
    Chain,
}
```

This is the pragmatic approach. The variant exists purely for deserialization compatibility and is stripped in `migrate_orchestration_document()`. All validation and compilation code must reject/handle it.

Update: Since we need Chain variant for migration deserialization, let's NOT remove it in Task 1. Instead, keep it with a `#[doc(hidden)]` annotation and reject it in all current code paths.

**Revised approach for Task 1+2:**
- Keep `Chain` variant in `FlowNodeKind` but mark it as legacy
- Keep `hops` in `FlowNodeData` (needed for migration)
- In `validate_orchestration()`: add a graph error for chain nodes ("chain_unsupported")
- In `edge_allowed()`: remove chain from allowed targets
- In `compile_orchestration()`: remove chain branch (compile will find chain via validation failure)
- In `migrate_orchestration_document()`: strip chain nodes per the migration logic above
- In `outbound()`: keep Chain in unreachable

This is cleaner. Let's revise.

- [ ] **Step 1 (revised): Keep Chain variant for deserialization, add "unsupported" validation**

In `FlowNodeKind` (line 48-55), keep Chain with deprecation comment:
```rust
pub enum FlowNodeKind {
    Start,
    End,
    Rule,
    NodeGroup,
    Builtin,
    /// V3 legacy — removed by migration. Not valid in V4 documents.
    Chain,
}
```

In `validate_orchestration()`, replace the old Chain match arm (lines 440-457) with:
```rust
FlowNodeKind::Chain => {
    graph(&mut issues, "chain_unsupported", Some(&node.id), None);
}
```

- [ ] **Step 2: Keep `hops` in `FlowNodeData` (needed for migration deserialization)**

No change to `FlowNodeData` struct. `hops` field stays for V3 document deserialization.

- [ ] **Step 3: Add V3→V4 migration logic**

Insert before `document.version = ORCHESTRATION_VERSION;` (line 316 of original):

```rust
    // V3 → V4: remove chain nodes
    if document.version == 3 {
        let chain_ids: HashSet<_> = document
            .nodes
            .iter()
            .filter(|node| node.kind == FlowNodeKind::Chain)
            .map(|node| node.id.clone())
            .collect();

        if !chain_ids.is_empty() {
            // Rewrite rule edges that target chain nodes
            for edge in document.edges.iter_mut() {
                if chain_ids.contains(&edge.target) {
                    let chain_node = document
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.target)
                        .unwrap();
                    let fallback = match chain_node.data.hops.as_slice() {
                        [GroupSource::Group { id, .. }] => {
                            if document
                                .nodes
                                .iter()
                                .any(|n| n.id == *id && n.kind == FlowNodeKind::NodeGroup)
                            {
                                id.clone()
                            } else {
                                "direct".to_string()
                            }
                        }
                        _ => "direct".to_string(),
                    };
                    edge.target = fallback;
                }
            }

            // Remove chain nodes and any edges still referencing them
            document
                .edges
                .retain(|edge| !chain_ids.contains(&edge.source) && !chain_ids.contains(&edge.target));
            document
                .nodes
                .retain(|node| node.kind != FlowNodeKind::Chain);
        }
    }
```

- [ ] **Step 4: Simplify edge_allowed()**

Lines 497-509:
```rust
fn edge_allowed(source: &FlowNode, target: &FlowNode) -> bool {
    match source.kind {
        FlowNodeKind::Start => target.kind == FlowNodeKind::Rule,
        FlowNodeKind::Rule => matches!(
            target.kind,
            FlowNodeKind::NodeGroup | FlowNodeKind::Builtin
        ),
        FlowNodeKind::End => {
            matches!(target.kind, FlowNodeKind::NodeGroup | FlowNodeKind::Builtin)
        }
        FlowNodeKind::NodeGroup | FlowNodeKind::Builtin | FlowNodeKind::Chain => false,
    }
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-core/src/orchestration.rs
git commit -m "feat(core): add V3→V4 migration, strip chain nodes"
```

---

### Task 3: Rust Core — Simplify Compilation

**Files:**
- Modify: `crates/chaos-core/src/orchestration.rs`

**Interfaces:**
- Consumes: `edge_allowed` without chain; `FlowNodeKind::Chain` blocked by validation
- Produces: `compile_orchestration()` without chain_outbound branch

- [ ] **Step 1: Remove chain_outbound function**

Delete lines 784-792 (the entire `chain_outbound` function):
```rust
// DELETE: fn chain_outbound(...)
```

- [ ] **Step 2: Simplify compile_orchestration target resolution**

Lines 543-546, change:
```rust
                outbound: match target.kind {
                    FlowNodeKind::Chain => chain_outbound(&nodes, target),
                    _ => outbound(target),
                },
```
To:
```rust
                outbound: outbound(target),
```

- [ ] **Step 3: Update outbound() unreachable**

Lines 775-783:
```rust
fn outbound(node: &FlowNode) -> String {
    match node.kind {
        FlowNodeKind::Builtin => "direct".into(),
        FlowNodeKind::NodeGroup => dae_identifier(&node.data.name),
        FlowNodeKind::Start | FlowNodeKind::End | FlowNodeKind::Rule | FlowNodeKind::Chain => {
            unreachable!("node kind cannot be a terminal outbound")
        }
    }
}
```

(Keeping Chain in unreachable since it'll never reach here — validation blocks it.)

- [ ] **Step 4: Delete validate_chain()**

Delete lines 596-640 (entire `validate_chain` function body — but keep a minimal version that just emits `chain_unsupported` if called, though actually the validate_orchestration match arm handles it now).

Already handled in Task 2 via the match arm. Delete the unused function.

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-core/src/orchestration.rs
git commit -m "refactor(core): remove chain_outbound, simplify compile_orchestration"
```

---

### Task 4: Rust Core — Update Tests

**Files:**
- Modify: `crates/chaos-core/src/orchestration.rs` (tests module, lines 927-1338)

**Interfaces:**
- Produces: Updated tests that verify V3→V4 migration, chain rejection, and correct compilation without chain

- [ ] **Step 1: Remove the chain helper function**

Delete lines 969-980 (the `chain()` test helper).

- [ ] **Step 2: Replace the chain compilation test**

Lines 1046-1083, replace "single_hop_chain_compiles_and_multi_hop_is_capability_gated" with:

```rust
    #[test]
    fn migrates_v3_document_with_chain_nodes() {
        let document = OrchestrationDocument {
            version: 3,
            nodes: vec![
                rule(
                    "rule-1",
                    RuleMatcherKind::DomainSuffix,
                    "example.com",
                    Some(1),
                ),
                group(),
                direct_builtin(),
                chain(vec![GroupSource::Group {
                    id: "group".into(),
                    weight: 1,
                }]),
                start_node(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("start-rule-1", "start", "rule-1"),
                FlowEdge::new("rule-chain", "rule-1", "chain"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };

        let migrated = migrate_orchestration_document(document);
        assert_eq!(migrated.version, ORCHESTRATION_VERSION);
        assert!(!migrated.nodes.iter().any(|n| n.kind == FlowNodeKind::Chain));
        // rule-1 should now target "group" (the chain's single hop)
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "rule-1" && e.target == "group"));
    }

    #[test]
    fn v3_chain_with_non_group_hop_degrades_to_direct() {
        let document = OrchestrationDocument {
            version: 3,
            nodes: vec![
                rule(
                    "rule-1",
                    RuleMatcherKind::DomainSuffix,
                    "example.com",
                    Some(1),
                ),
                direct_builtin(),
                start_node(),
                end_node(),
                chain(vec![GroupSource::Node {
                    id: "node-1".into(),
                    weight: 1,
                }]),
            ],
            edges: vec![
                FlowEdge::new("start-rule-1", "start", "rule-1"),
                FlowEdge::new("rule-chain", "rule-1", "chain"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };

        let migrated = migrate_orchestration_document(document);
        assert!(!migrated.nodes.iter().any(|n| n.kind == FlowNodeKind::Chain));
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "rule-1" && e.target == "direct"));
    }

    #[test]
    fn v4_rejects_chain_nodes_in_validation() {
        let mut document = OrchestrationDocument::default();
        document.nodes.push(chain(vec![]));
        document.nodes[document.nodes.len() - 1].kind = FlowNodeKind::Chain;
        let report = document.validate();
        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "chain_unsupported"));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p chaos-core --lib orchestration`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/chaos-core/src/orchestration.rs
git commit -m "test(core): update orchestration tests for V4, add migration coverage"
```

---

### Task 5: Rust API — Remove Chain from Publish Pipeline

**Files:**
- Modify: `crates/chaos-api/src/routes/orchestration.rs`

**Interfaces:**
- Consumes: `FlowNodeKind` with Chain variant gated by validation
- Produces: publish pipeline without chain materialization

- [ ] **Step 1: Remove chain references from normalization**

Search for `FlowNodeKind::Chain` or `GroupSource` in the normalization code (lines ~588-620 area). Read the normalization function to find chain-related logic:

Run: `grep -n "Chain\|chain\|hops" crates/chaos-api/src/routes/orchestration.rs`

Expected findings: normalization may reference `hops` for chain node data. Remove any chain-specific normalization.

- [ ] **Step 2: Remove expand_materialized_chains if it exists**

Search for `expand_materialized_chains` or `materialized_chain` in the file.

Run: `grep -n "materialized_chain\|expand_materialized" crates/chaos-api/src/routes/orchestration.rs`

If found, delete the function and its call site.

- [ ] **Step 3: Remove chain from SourceCatalog if present**

Check `SourceCatalog` for chain-specific fields. Based on the code read, `SourceCatalog` doesn't track chains directly — it tracks node/subscription/group IDs. No change needed.

- [ ] **Step 4: Compile and verify**

Run: `cargo check -p chaos-api`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add crates/chaos-api/src/routes/orchestration.rs
git commit -m "refactor(api): remove chain from publish pipeline"
```

---

### Task 6: TypeScript — Remove Chain Types from api.ts

**Files:**
- Modify: `apps/web/src/lib/api.ts`

- [ ] **Step 1: Remove Chain type definitions**

Lines 238, delete `'chain'` from type union:
```typescript
export type OrchestrationNodeKind = 'start' | 'end' | 'rule' | 'node_group' | 'builtin';
```

Lines 280-286, delete `OrchestrationChainData`:
```typescript
// DELETE OrchestrationChainData entirely
```

Lines 288-293, simplify `OrchestrationNodeData`:
```typescript
export type OrchestrationNodeData =
    | OrchestrationRuleData
    | OrchestrationNodeGroupData
    | OrchestrationBuiltinData
    | OrchestrationAnchorData;
```

Lines 337-342, delete `OrchestrationChainNodeDto`:
```typescript
// DELETE OrchestrationChainNodeDto entirely
```

Lines 344-350, simplify `OrchestrationNodeDto`:
```typescript
export type OrchestrationNodeDto =
    | OrchestrationRuleNodeDto
    | OrchestrationNodeGroupDto
    | OrchestrationBuiltinNodeDto
    | OrchestrationStartNodeDto
    | OrchestrationEndNodeDto;
```

Lines 364-370, update `OrchestrationDocument` version:
```typescript
export type OrchestrationDocument = {
    version: 2 | 3 | 4;
    nodes: OrchestrationNodeDto[];
    edges: OrchestrationEdgeDto[];
    viewport: { x: number; y: number; zoom: number };
    needs_republish?: boolean;
};
```

- [ ] **Step 2: Check for type errors**

Run: `cd apps/web && npx tsc --noEmit 2>&1 | head -40`
Expected: Type errors from orchestration.ts and components (fixed in later tasks)

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/api.ts
git commit -m "refactor(web): remove Chain types from api.ts"
```

---

### Task 7: TypeScript — Update orchestration.ts Logic

**Files:**
- Modify: `apps/web/src/lib/orchestration.ts`

- [ ] **Step 1: Remove Chain imports and types**

Line 6, remove `OrchestrationChainData` from imports. Lines 33-35, remove `ChainDataPatch`:
```typescript
export type RuleDataPatch = Partial<OrchestrationRuleData>;
export type GroupDataPatch = Partial<OrchestrationNodeGroupData>;
export type NodeDataPatch = RuleDataPatch | GroupDataPatch;
```

- [ ] **Step 2: Remove createChainNode()**

Delete lines 113-126 (entire `createChainNode` function).

- [ ] **Step 3: Update migrateDocument() to V4 with chain removal**

Lines 128-145, replace the entire function:

```typescript
const ORCHESTRATION_VERSION = 4;

export function migrateDocument(document: OrchestrationDocument): OrchestrationDocument {
    const nodes = [...(document.nodes ?? [])];
    const edges = [...(document.edges ?? [])];

    // V3 → V4: remove chain nodes
    if (document.version === 3) {
        const chainIds = new Set(
            nodes.filter((node) => node.type === 'chain').map((node) => node.id)
        );
        if (chainIds.size > 0) {
            // Rewrite rule edges targeting chain nodes
            for (const edge of edges) {
                if (chainIds.has(edge.target)) {
                    const chainNode = nodes.find((node) => node.id === edge.target);
                    if (chainNode?.type === 'chain') {
                        const hop = chainNode.data.hops?.[0];
                        edge.target =
                            hop?.kind === 'group' &&
                            nodes.some((n) => n.id === hop.id && n.type === 'node_group')
                                ? hop.id
                                : 'direct';
                    }
                }
            }
            // Remove chain nodes and their edges
            // TypeScript cast needed because we're filtering by runtime type
            const withoutChains = nodes.filter((node) => node.type !== 'chain') as OrchestrationNodeDto[];
            nodes.length = 0;
            nodes.push(...withoutChains);
            const remainingEdges = edges.filter(
                (edge) => !chainIds.has(edge.source) && !chainIds.has(edge.target)
            );
            edges.length = 0;
            edges.push(...remainingEdges);
        }
    }

    // Ensure required anchors
    if (!nodes.some((node) => node.type === 'start')) nodes.push(createStartNode());
    if (!nodes.some((node) => node.type === 'end')) nodes.push(createEndNode());
    if (!nodes.some((node) => node.type === 'builtin' && node.data.builtin === 'direct')) {
        nodes.push(createDirectBuiltin({ x: 1080, y: 320 }));
    }
    for (const rule of nodes.filter((node) => node.type === 'rule')) {
        if (!edges.some((edge) => edge.source === 'start' && edge.target === rule.id)) {
            edges.push({ id: `start-${rule.id}`, source: 'start', target: rule.id });
        }
    }
    if (!edges.some((edge) => edge.source === 'end')) {
        edges.push({ id: 'end-direct', source: 'end', target: 'direct' });
    }
    return { ...document, version: ORCHESTRATION_VERSION, nodes, edges, viewport: safeViewport(document.viewport) };
}
```

- [ ] **Step 4: Update sanitizeDocument version and remove chain sanitize**

Line 153, change version:
```typescript
        version: ORCHESTRATION_VERSION,
```

Lines 185-198, delete the chain case from `sanitizeNode`:
```typescript
// DELETE: if (node.type === 'chain') { ... return ... }
```

Lines 160-211, the chain branch (`if (node.type === 'chain')`) should be removed entirely.

- [ ] **Step 5: Update canConnect()**

Lines 372-373, remove chain from rule targets:
```typescript
    if (sourceNode.type === 'rule') {
        return targetNode.type === 'node_group' || targetNode.type === 'builtin';
    }
```

- [ ] **Step 6: Update decorateDocument()**

Lines 254-256, remove chain from targets map:
```typescript
        const targets = new Map(
            document.nodes
                .filter((node) => node.type === 'node_group' || node.type === 'builtin')
                .map((node) => [node.id, node.type === 'builtin' ? 'DIRECT' : node.data.name])
        );
```

Lines 294-310, delete the chain decoration block (entire `if (node.type === 'chain')` case).

- [ ] **Step 7: Update autoLayout()**

Lines 413-431, remove chain from outbounds:
```typescript
    const outbounds = [
        ...document.nodes.filter((node) => node.type === 'node_group'),
        ...document.nodes.filter((node) => node.type === 'builtin')
    ];
```

- [ ] **Step 8: Update validateLocal()**

Line 442, change version check:
```typescript
    if (document.version !== ORCHESTRATION_VERSION) graph('unsupported_version');
```

Line 481, remove chain from allowed connections:
```typescript
            const allowed =
                (source.type === 'start' && target.type === 'rule') ||
                (source.type === 'rule' && ['node_group', 'builtin'].includes(target.type)) ||
                (source.type === 'end' && ['node_group', 'builtin'].includes(target.type));
```

Lines 541-557, delete the chain validation block (entire `if (node.type === 'chain')` case).

- [ ] **Step 9: Check TypeScript compilation**

Run: `cd apps/web && npx tsc --noEmit 2>&1 | grep -c error`
Expected: Errors reduced from Task 6 (remaining errors are in page.svelte, inspector, canvas — fixed in next tasks)

- [ ] **Step 10: Commit**

```bash
git add apps/web/src/lib/orchestration.ts
git commit -m "refactor(web): remove Chain from orchestration.ts, add V3→V4 migration"
```

---

### Task 8: TypeScript — Clean +page.svelte

**Files:**
- Modify: `apps/web/src/routes/(app)/orchestrate/+page.svelte`

- [ ] **Step 1: Remove chain imports and type references**

Line 36, remove `OrchestrationChainData` from imports. Line 46, remove `createChainNode` from imports. Line 56, add `ORCHESTRATION_VERSION` to imports:
```typescript
import {
    autoLayout,
    createGroupNode,
    createRuleNode,
    decorateDocument,
    ORCHESTRATION_VERSION,
    sanitizeDocument,
    setEndTarget,
    setRuleTarget,
    snapshotDocument,
    validateLocal
} from '$lib/orchestration';
```

Line 67, remove chain from AddableNodeKind:
```typescript
    type AddableNodeKind = 'rule' | 'node_group';
```

Line 115, change version:
```typescript
        version: ORCHESTRATION_VERSION,
```

- [ ] **Step 2: Remove chain helper functions**

Delete lines 287-289 (nextChainIndex):
```typescript
// DELETE: function nextChainIndex()
```

Lines 291-317, simplify addNodeAt:
```typescript
    function addNodeAt(kind: AddableNodeKind, position: { x: number; y: number }) {
        let created: OrchestrationNodeDto | null = null;
        mutate(() => {
            created = kind === 'rule'
                ? createRuleNode(position, nextRulePriority())
                : createGroupNode(position, nextGroupIndex());
            flowNodes = [...flowNodes, created];
            if (kind === 'rule' && created) {
                flowEdges = [...flowEdges, { id: `start-${created.id}`, source: 'start', target: created.id }];
            }
            if (kind === 'node_group') {
                const directY = 70 + flowNodes.filter((node) => node.type === 'node_group').length * 180;
                flowNodes = flowNodes.map((node) =>
                    node.type === 'builtin'
                        ? { ...node, position: { x: 520, y: directY } }
                        : node
                );
            }
        }, `add:${kind}`);
        if (created) {
            const id = (created as OrchestrationNodeDto).id;
            selectNode(id);
            setTimeout(() => selectNode(id), 60);
        }
    }
```

Lines 319-324, simplify addFromLibrary:
```typescript
    function addFromLibrary(kind: AddableNodeKind) {
        if (kind === 'rule') {
            addNodeAt(kind, { x: 280, y: 70 + ruleCount * 145 });
            return;
        }
        addNodeAt(kind, { x: 860, y: 70 + groupCount * 180 });
    }
```

- [ ] **Step 3: Remove updateChain()**

Delete lines 349-358 (entire `updateChain` function).

- [ ] **Step 4: Remove chainCount and references**

Lines 503-504, remove chainCount:
```typescript
    const ruleCount = $derived(flowNodes.filter((node) => node.type === 'rule').length);
```

Lines 632-638, remove the chain library button and palette item (the "Waypoints" button):
```svelte
<!-- DELETE: Chain palette card and the Waypoints button -->
```

Line 686, remove the Waypoints toolbar button:
```svelte
<!-- DELETE: <Button variant="ghost" size="icon" icon={Waypoints} ... /> -->
```

- [ ] **Step 5: Remove onupdatechain from inspector props**

Lines 714-731, remove `onupdatechain`:
```svelte
            <OrchestrationInspector
                node={selectedNode}
                edge={selectedEdge}
                flowNodes={flowNodes}
                flowEdges={flowEdges}
                {inventoryNodes}
                {subscriptions}
                {groups}
                issues={validation.issues}
                busy={!!busy}
                onupdaterule={updateRule}
                onupdategroup={updateGroup}
                onsettarget={updateRuleTarget}
                onsetendtarget={updateEndTarget}
                ondelete={deleteElement}
                onselectnode={selectNode}
            />
```

- [ ] **Step 6: Remove unneeded icons from imports**

Line 22, remove `Waypoints` from the lucide import if no longer used elsewhere (check: it IS used as the chain palette icon — remove it):
```typescript
    import {
        // ... other icons
        // Waypoints,  ← REMOVE
    } from '@lucide/svelte';
```

- [ ] **Step 7: Check compilation**

Run: `cd apps/web && npx svelte-check 2>&1 | head -20`
Expected: Errors reduced (remaining from Canvas and Inspector — next tasks)

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/routes/\(app\)/orchestrate/+page.svelte
git commit -m "refactor(web): remove Chain from orchestration page"
```

---

### Task 9: TypeScript — Clean Canvas

**Files:**
- Modify: `apps/web/src/lib/components/features/OrchestrationCanvas.svelte`

- [ ] **Step 1: Remove chain from allowed node kinds in props**

Line 47 area, update `onaddnode` type:
```typescript
    onaddnode: (kind: 'rule' | 'node_group', position: { x: number; y: number }) => void;
```

- [ ] **Step 2: Remove chain from drop handler**

Lines 69-80 area, update drop handler to exclude chain:
```typescript
    function onDrop(event: DragEvent) {
        const kind = event.dataTransfer?.getData('application/chaos-flow-node') as 'rule' | 'node_group' | null;
        if (!kind) return;
        // ... existing position calculation ...
        onaddnode(kind, position);
    }
```

Actually, re-read the canvas code to be precise. The drop handler should already only handle 'rule' | 'node_group' since we removed 'chain' from AddableNodeKind. No change needed if the handler filters correctly.

- [ ] **Step 3: Remove chain from node type registration**

If the canvas registers `ChainNode` component, remove that registration (check `<SvelteFlow>` nodeTypes prop).

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/components/features/OrchestrationCanvas.svelte
git commit -m "refactor(web): remove Chain from orchestration canvas"
```

---

### Task 10: TypeScript — Complete Inspector (End + Start panels)

**Files:**
- Modify: `apps/web/src/lib/components/features/OrchestrationInspector.svelte`

- [ ] **Step 1: Remove chain-related props and imports**

Line 7, remove `OrchestrationChainData` import. Lines 38, 55, delete `onupdatechain` prop:
```typescript
    let {
        node,
        edge,
        flowNodes,
        flowEdges,
        inventoryNodes,
        subscriptions,
        groups,
        issues,
        busy = false,
        onupdaterule,
        onupdategroup,
        onsettarget,
        onsetendtarget,
        ondelete,
        onselectnode
    }: {
        node: OrchestrationNodeDto | null;
        edge: OrchestrationEdgeDto | null;
        flowNodes: OrchestrationNodeDto[];
        flowEdges: OrchestrationEdgeDto[];
        inventoryNodes: NodeDto[];
        subscriptions: SubscriptionDto[];
        groups: GroupDto[];
        issues: OrchestrationValidationIssue[];
        busy?: boolean;
        onupdaterule: (id: string, patch: Partial<OrchestrationRuleData>) => void;
        onupdategroup: (id: string, patch: Partial<OrchestrationNodeGroupData>) => void;
        onsettarget: (ruleId: string, targetId: string | null) => void;
        onsetendtarget: (targetId: string | null) => void;
        ondelete: (kind: 'node' | 'edge', id: string) => void;
        onselectnode: (id: string) => void;
    } = $props();
```

- [ ] **Step 2: Update computed values to remove chain**

Line 65, simplify `selectedSources`:
```typescript
    const selectedSources = $derived(node?.type === 'node_group' ? node.data.sources : []);
```

Line 79-81, remove chain from `targetOptions`:
```typescript
    const targetOptions = $derived(
        flowNodes.filter((item) => item.type === 'node_group' || item.type === 'builtin')
    );
```

Line 83, update `incomingRules` (remove chain from includes):
```typescript
        if (!node || !['node_group', 'builtin'].includes(node.type)) return [];
```

- [ ] **Step 3: Remove chain references in helper functions**

Lines 151-158, `nodeName`:
```typescript
    function nodeName(item: OrchestrationNodeDto): string {
        if (item.type === 'rule') return item.data.matcher.pattern || t('flow.rule.untitled');
        if (item.type === 'node_group') return item.data.name || t('flow.node.unnamedGroup');
        if (item.type === 'start') return t('flow.start');
        if (item.type === 'end') return t('flow.end');
        return 'DIRECT';
    }
```

Lines 169-193, `toggleSource` and `updateSourceWeight` — remove chain branches:
```typescript
    function toggleSource(option: ResourceOption, checked: boolean) {
        if (node?.type !== 'node_group') return;
        const sources = checked
            ? isSelected(option)
                ? node.data.sources
                : [...node.data.sources, { kind: option.kind, id: option.id, weight: 1 }]
            : node.data.sources.filter(
                    (source) => !(source.kind === option.kind && source.id === option.id)
                );
        patchGroup({ sources });
    }

    function updateSourceWeight(source: OrchestrationSource, weight: number) {
        if (node?.type !== 'node_group') return;
        const normalized = Math.max(1, Math.min(99, Math.floor(weight) || 1));
        const next = selectedSources.map((item) =>
                item.kind === source.kind && item.id === source.id ? { ...item, weight: normalized } : item
        );
        patchGroup({ sources: next });
    }
```

- [ ] **Step 4: Add End node panel**

After the `{:else if node?.type === 'builtin'}` block (line 393), insert before `{:else if edge}`:

```svelte
    {:else if node?.type === 'end'}
        <section class="inspector-section">
            <Field label={t('flow.fallbackTarget')} forId="flow-end-target">
                <select
                    id="flow-end-target"
                    value={selectedTargetId}
                    disabled={busy}
                    onchange={(event) =>
                        onsetendtarget((event.currentTarget as HTMLSelectElement).value || null)}
                >
                    <option value="">{t('flow.rule.noTarget')}</option>
                    {#each targetOptions as target (target.id)}
                        <option value={target.id}>{nodeName(target)}</option>
                    {/each}
                </select>
            </Field>
            <p class="fixed-summary-text">{t('flow.end.description')}</p>
        </section>
```

- [ ] **Step 5: Add Start node panel**

After the End panel, insert before `{:else if edge}`:

```svelte
    {:else if node?.type === 'start'}
        <section class="inspector-section">
            <dl class="overview">
                <div>
                    <dt>{t('flow.rules')}</dt>
                    <dd>{flowEdges.filter((edge) => edge.source === 'start').length}</dd>
                </div>
            </dl>
            <p class="fixed-summary-text">{t('flow.start.description')}</p>
        </section>
```

Note: End and Start panels use existing CSS classes (`fixed-summary`, `overview`). Add a small CSS rule if needed:

```css
    .fixed-summary-text {
        margin: 0;
        color: var(--ink-muted);
        font-size: 0.68rem;
        line-height: 1.5;
    }
```

- [ ] **Step 6: Remove delete button guard for chain nodes**

Line 217, the delete button guard is `node.type !== 'builtin'` — that's correct (chain is gone, so no need to add chain to the guard). No change.

- [ ] **Step 7: Update i18n keys**

Check if `flow.fallbackTarget`, `flow.end.description`, `flow.start.description` exist in locale files. If not, add them in the next task (Task 13). For now, use inline English strings as fallback.

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/lib/components/features/OrchestrationInspector.svelte
git commit -m "feat(web): add End/Start panels to Inspector, remove chain support"
```

---

### Task 11: TypeScript — Unit Tests for V4 Migration

**Files:**
- Create: `apps/web/src/lib/orchestration.test.ts`

- [ ] **Step 1: Write migration tests**

Create `apps/web/src/lib/orchestration.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { migrateDocument, canConnect, validateLocal } from './orchestration';
import type { OrchestrationDocument, OrchestrationNodeDto } from './api';

function makeNodes(nodes: Partial<OrchestrationNodeDto>[]): OrchestrationNodeDto[] {
    return nodes.map((n) => ({
        id: n.id ?? 'test',
        type: n.type ?? 'rule',
        position: n.position ?? { x: 0, y: 0 },
        data: n.data ?? {},
        ...n,
    })) as OrchestrationNodeDto[];
}

describe('migrateDocument V3→V4', () => {
    it('removes chain nodes from V3 document', () => {
        const doc: OrchestrationDocument = {
            version: 3,
            nodes: [
                { id: 'start', type: 'start', position: { x: 0, y: 0 }, data: {} },
                { id: 'end', type: 'end', position: { x: 0, y: 0 }, data: {} },
                { id: 'direct', type: 'builtin', position: { x: 0, y: 0 }, data: { builtin: 'direct' } },
                { id: 'rule-1', type: 'rule', position: { x: 0, y: 0 }, data: { matcher: { kind: 'domain_suffix', pattern: 'example.com' }, priority: 1 } },
                { id: 'chain-1', type: 'chain', position: { x: 0, y: 0 }, data: { name: 'test', hops: [] } },
            ] as OrchestrationNodeDto[],
            edges: [
                { id: 'start-rule-1', source: 'start', target: 'rule-1' },
                { id: 'rule-chain', source: 'rule-1', target: 'chain-1' },
                { id: 'end-direct', source: 'end', target: 'direct' },
            ],
            viewport: { x: 0, y: 0, zoom: 1 },
        };

        const result = migrateDocument(doc);
        expect(result.version).toBe(4);
        expect(result.nodes.some((n) => n.type === 'chain')).toBe(false);
        // Rule should be redirected to direct (chain had no group hop)
        expect(result.edges.some((e) => e.source === 'rule-1' && e.target === 'direct')).toBe(true);
    });

    it('rewrites rule to group when chain has single group hop to flow group', () => {
        const doc: OrchestrationDocument = {
            version: 3,
            nodes: [
                { id: 'start', type: 'start', position: { x: 0, y: 0 }, data: {} },
                { id: 'end', type: 'end', position: { x: 0, y: 0 }, data: {} },
                { id: 'direct', type: 'builtin', position: { x: 0, y: 0 }, data: { builtin: 'direct' } },
                { id: 'group-1', type: 'node_group', position: { x: 0, y: 0 }, data: { name: 'test', policy: 'min_moving_avg', sources: [{ kind: 'node', id: 'n1', weight: 1 }] } },
                { id: 'rule-1', type: 'rule', position: { x: 0, y: 0 }, data: { matcher: { kind: 'domain_suffix', pattern: 'example.com' }, priority: 1 } },
                { id: 'chain-1', type: 'chain', position: { x: 0, y: 0 }, data: { name: 'test', hops: [{ kind: 'group', id: 'group-1', weight: 1 }] } },
            ] as OrchestrationNodeDto[],
            edges: [
                { id: 'start-rule-1', source: 'start', target: 'rule-1' },
                { id: 'rule-chain', source: 'rule-1', target: 'chain-1' },
                { id: 'end-direct', source: 'end', target: 'direct' },
            ],
            viewport: { x: 0, y: 0, zoom: 1 },
        };

        const result = migrateDocument(doc);
        expect(result.nodes.some((n) => n.type === 'chain')).toBe(false);
        expect(result.edges.some((e) => e.source === 'rule-1' && e.target === 'group-1')).toBe(true);
    });
});

describe('canConnect V4', () => {
    it('rejects rule → chain connection', () => {
        const nodes: OrchestrationNodeDto[] = [
            { id: 'rule-1', type: 'rule', position: { x: 0, y: 0 }, data: { matcher: { kind: 'domain_suffix', pattern: 'test' }, priority: 1 } },
            { id: 'chain-1', type: 'chain', position: { x: 0, y: 0 }, data: { name: 'c', hops: [] } },
        ] as OrchestrationNodeDto[];
        const edges: any[] = [];

        expect(canConnect({ source: 'rule-1', target: 'chain-1' }, nodes, edges)).toBe(false);
    });

    it('allows rule → node_group connection', () => {
        const nodes: OrchestrationNodeDto[] = [
            { id: 'rule-1', type: 'rule', position: { x: 0, y: 0 }, data: { matcher: { kind: 'domain_suffix', pattern: 'test' }, priority: 1 } },
            { id: 'group-1', type: 'node_group', position: { x: 0, y: 0 }, data: { name: 'g', policy: 'min_moving_avg', sources: [{ kind: 'node', id: 'n1', weight: 1 }] } },
        ] as OrchestrationNodeDto[];
        const edges: any[] = [];

        expect(canConnect({ source: 'rule-1', target: 'group-1' }, nodes, edges)).toBe(true);
    });
});
```

- [ ] **Step 2: Run TS tests**

Run: `cd apps/web && npx vitest run src/lib/orchestration.test.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/orchestration.test.ts
git commit -m "test(web): add V4 migration and validation unit tests"
```

---

### Task 12: TypeScript — Delete ChainNode and Update Exports

**Files:**
- Delete: `apps/web/src/lib/flow/ChainNode.svelte`
- Modify: `apps/web/src/lib/index.ts`

- [ ] **Step 1: Delete ChainNode.svelte**

```bash
git rm apps/web/src/lib/flow/ChainNode.svelte
```

- [ ] **Step 2: Remove ChainNode export from index.ts**

In `apps/web/src/lib/index.ts`, find and remove:
```typescript
export { default as ChainNode } from './flow/ChainNode.svelte';
```

- [ ] **Step 3: Verify no remaining imports of ChainNode**

Run: `grep -r "ChainNode" apps/web/src/ --include="*.ts" --include="*.svelte"`
Expected: No output

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/flow/ChainNode.svelte apps/web/src/lib/index.ts
git commit -m "refactor(web): delete ChainNode component"
```

---

### Task 13: Locales — Add Inspector End/Start Strings

**Files:**
- Modify: `locales/en.json`
- Modify: `locales/zh-CN.json`

- [ ] **Step 1: Add English strings**

In `locales/en.json`, find the `flow` namespace and add:

```json
    "flow.fallbackTarget": "Fallback Target",
    "flow.end.description": "When no rule matches, traffic is forwarded to this target.",
    "flow.start.description": "All traffic enters here and is distributed to rules by priority."
```

- [ ] **Step 2: Add Chinese strings**

In `locales/zh-CN.json`, find the `flow` namespace and add:

```json
    "flow.fallbackTarget": "回退目标",
    "flow.end.description": "当没有规则匹配时，流量将转发到此目标。",
    "flow.start.description": "所有流量从这里进入，按优先级分发到各规则。"
```

- [ ] **Step 3: Commit**

```bash
git add locales/en.json locales/zh-CN.json
git commit -m "feat(i18n): add End/Start inspector strings"
```

---

### Task 14: Docs — Update MASTER.md Files

**Files:**
- Modify: `design-system/design-system/chaos/MASTER.md`
- Modify: `docs/design-system/MASTER.md`

- [ ] **Step 1: Update orchestration node count**

In both files, find lines mentioning "6 node types" or `FlowNodeKind`. Update to "5 node types". Update any description of Chain nodes to note they were removed in V4.

- [ ] **Step 2: Remove Chain from the node inventory list**

Find the table/list of node types and remove the Chain row.

- [ ] **Step 3: Commit**

```bash
git add design-system/design-system/chaos/MASTER.md docs/design-system/MASTER.md
git commit -m "docs: update MASTER.md for V4 orchestration (no Chain)"
```

---

### Task 15: Integration — Full Build and Manual Smoke Test

**Files:**
- All modified files

- [ ] **Step 1: Full Rust build**

Run: `cargo build --workspace`
Expected: Success

- [ ] **Step 2: Full Rust tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 3: Frontend type check**

Run: `cd apps/web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Frontend build**

Run: `cd apps/web && npm run build`
Expected: Success

- [ ] **Step 5: Commit final integration**

If any fixes needed:
```bash
git add -A
git commit -m "chore: fix integration issues from V4 orchestration changes"
```
