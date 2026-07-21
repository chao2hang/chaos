# 节点选择处统一测速（Node Picker Latency）

**Date:** 2026-07-21  
**Status:** Approved for implementation (design review)  
**Product:** chaos web console  
**Depends on:** Existing latency API (`POST /api/v1/latency/test`, `GET /api/v1/latency`), probe pipeline (proxy prober / TCP fallback), UI helpers in `apps/web/src/lib/latency.ts`  
**Related:** Nodes page and Dashboard already run latency tests; Groups member editors and Orchestration node-source pickers do not.

## Goal

Wherever the UI lists or multi-selects **proxy nodes** for membership or group sources, the user can:

1. See each node’s **latest cached latency** (same color thresholds as the Nodes page).
2. **Test one node** in place.
3. **Test all currently visible** (search-filtered) nodes in one action.

This makes latency available at the moment of selection (group members, orchestration node_group sources), not only on the Nodes / Dashboard screens.

## Non-goals

- Latency test controls on **subscription** or **group** resource rows (only leaf proxy nodes).
- Auto-run latency when a picker opens.
- Auto-sort or filter pickers by latency (may follow later).
- Changes to probe protocol, prober binary, or REST contract.
- Full rewrite of the Nodes page table (optional later reuse of shared session logic only).
- Routing / DNS outbound dropdowns (those select group names / upstreams, not the node inventory pool).

## Current baseline

| Area | Latency display | In-place test |
|------|-----------------|---------------|
| Nodes page | Yes | Bulk + per-row |
| Dashboard | Summary + list | Test all |
| Groups create/edit member list | No | No |
| `GroupDraftEditor` member list | No | No |
| Orchestration inspector node source tab | No | No |

Backend already accepts optional `ids` on `POST /latency/test` and persists results for `GET /latency`. No API work is required for this feature.

## Decisions (from design review)

| Topic | Choice |
|-------|--------|
| Scope | Every UI that lists/checks **proxy nodes** for membership or sources |
| Interaction | Cached latency on each row + per-row Test + toolbar **Test visible** |
| Bulk target | **Visible** (post-search filter) node ids — not “checked members” |
| Architecture | Shared **latency session** + reusable **`NodePickList`** feature component |
| Backend | No change |

## Approach (chosen)

**Reusable picker with shared session (Approach 1).**

Rejected alternatives:

- **Shared store only, copy-paste UI** — data consistent, row chrome drifts across Groups vs Orchestrate.
- **Groups-only full UX; orchestrate read-only cache** — fails the product goal that every node-selection surface can test.

```text
latencySession  →  list/test/merge/testing marker
NodePickList    →  search, checkboxes, latency column, test buttons, optional weight
Parents         →  members / sources state only
```

---

## Architecture

### Units

| Unit | Responsibility | Depends on |
|------|----------------|------------|
| `apps/web/src/lib/latencySession.svelte.ts` (or equivalent) | Load cache, test by ids, merge into `latencyById`, `testing` marker, generation guard for stale responses, error message helpers | `listLatency`, `testLatency`, `$lib/latency`, i18n |
| `apps/web/src/lib/components/features/NodePickList.svelte` | Node pick list UI: filter, rows, latency tone, per-row test, “test visible”, optional weight fields | `latencySession`, existing `Button` / `SearchInput` / tokens |
| Parent pages/editors | Own membership/source state; pass check/weight callbacks; form `busy` | `NodePickList` |

### `NodePickList` API (conceptual)

**Inputs**

- `nodes: NodeDto[]` — full pool for this editor (parent already loaded inventory).
- `busy?: boolean` — form save / publish in progress; disables check, weight, and test.
- `isChecked(node): boolean`
- `onToggle(node, checked): void`
- Optional weight: `showWeight`, `getWeight(node)`, `onWeight(node, weight)` when checked.
- Optional: controlled `query` / bindable search if parent needs it (default internal search state is fine).

**Internal**

- Search string → `filteredNodes`
- On mount: `loadLatency()` via session
- Per-row latency from `latencyById[node.id]`
- Buttons: test one (`ids: [id]`), test visible (`ids: filteredNodes.map(id)`)

**Out of scope for the list component**

- Persisting groups or orchestration graph
- Deleting nodes
- Subscription/group tabs

### Orchestration specifics

- Use `NodePickList` (or the same row chrome + session) **only on the node source tab**.
- Subscription and group tabs stay unchanged (no bulk node expand/test in this slice).
- Optional low-cost polish: selected-sources summary shows a **read-only** latency badge for `kind === 'node'` from the same cache (no test button in the narrow summary strip).

### Groups / GroupDraftEditor

- Replace duplicated member list markup with `NodePickList`.
- Parent keeps `members: GroupMemberDto[]` (or draft group members) and weight rules (`1..99`) as today.

### Nodes page

- Not required to migrate to `NodePickList` in this slice (table + multi-select semantics differ).
- Prefer extracting test/merge into `latencySession` so Nodes can adopt later without behavior change.

---

## Data flow

```text
Mount picker
  → GET /api/v1/latency
  → latencyById[id] = LatencyDto

Per-row Test
  → POST /api/v1/latency/test { "ids": ["…"] }
  → merge results into latencyById
  → row shows formatLatencyMs + latencyClass(tone)

Test visible
  → ids = filtered node ids
  → empty ids → button disabled, no request
  → POST { "ids": [...] }
  → merge all; toast/notice alive/total via existing i18n

Toggle / weight
  → parent callbacks only; independent of in-flight tests
```

**Cache semantics:** Server upserts `latency_results` (existing). Other screens that `listLatency` see the same results. No separate client persistence.

**Color thresholds** (unchanged): good `< 200ms`, warn `200–500ms`, bad `> 500ms` or not alive; unknown when never tested.

---

## Concurrency and errors

| Scenario | Behavior |
|----------|----------|
| Test in flight | Disable all test controls in that session (`testing` is `'visible' \| nodeId \| null`), same spirit as Nodes page |
| Form `busy` | Disable check, weight, and test |
| User switches group / selected flow node mid-request | Ignore stale response (generation counter or equivalent) |
| Large visible set | Single batch POST of all visible ids; no client chunking (matches existing API) |
| `listLatency` fails | Picker still usable; latency column `—`; short error + retry path for latency load |
| `testLatency` fails | Keep previous cache; surface error via Notice / existing patterns; clear `testing` |
| Probe failure per node | Backend returns `alive: false` + message; row shows fail tone and title message |
| Partial batch success | Merge each result; finish message uses alive/total (`dashboard.latencyFinished`) |

Tests must not block membership editing beyond disabling the test buttons while a test is running.

---

## i18n

Reuse where possible:

- `nodes.testNode`, `dashboard.latencyFinished`, `nodes.latencyFailed` / `dashboard.latencyFailed`, latency column patterns from Nodes.

Add only if needed:

- `nodes.testVisible` — e.g. EN “Test visible”, ZH “测试可见节点” (and optional count: “Test {count} visible”).

Update both `locales/en.json` and `locales/zh-CN.json`.

---

## File touch list (expected)

```text
apps/web/src/lib/latencySession.svelte.ts          # new
apps/web/src/lib/components/features/NodePickList.svelte  # new
apps/web/src/routes/(app)/groups/+page.svelte
apps/web/src/lib/components/features/OrchestrationInspector.svelte
apps/web/src/lib/components/features/GroupDraftEditor.svelte
locales/en.json
locales/zh-CN.json
docs/superpowers/specs/2026-07-21-node-picker-latency-design.md
```

Backend crates: **no intentional changes**.

---

## Testing plan

| Layer | What |
|-------|------|
| Unit | `latencySession`: merge; empty ids no call; failure preserves map; stale generation discarded if implemented |
| Manual / component | Groups: open editor → cached latency → row test → test visible → toggle works during/after |
| Manual | Orchestrate: select node_group → node tab same behavior; subscription/group tabs have no node test chrome |
| Regression | Nodes page + Dashboard latency still work; group save and orchestration draft/publish unaffected |
| Out of scope | Probe accuracy, prober vs TCP (covered by existing API tests) |

---

## Success criteria

1. Every proxy-node multi-select list in Groups and Orchestration (node tab) shows latency tones and supports single + visible tests.
2. Test/merge logic is not copy-pasted three ways; `NodePickList` + session are the single front-end path for picker latency.
3. Test failure does not discard unsaved membership/source drafts; checkboxes remain usable when only tests are in flight.
4. No REST contract break; existing latency API tests remain green.

---

## Implementation notes

- Match surrounding density and tokens (`member-row` / `node-row` / inspector resource list); prefer restyling shared list once over three one-off layouts.
- Gauge icon and button sizes should follow existing `Button` + Lucide usage on Nodes.
- Do not stream progress (product already treats latency as synchronous batch for MVP).
- YAGNI: no websocket progress, no sort-by-latency, no subscription expand-to-nodes testing in this slice.
