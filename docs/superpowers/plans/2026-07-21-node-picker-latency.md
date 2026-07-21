# Node Picker Latency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every proxy-node multi-select list (Groups members, GroupDraftEditor, Orchestration node source tab) shows cached latency and supports single-node + “test visible” actions, reusing existing latency APIs.

**Architecture:** Extract pure merge/generation helpers plus a small reactive `createLatencySession` factory; build one `NodePickList` feature component that owns search, latency column, and test buttons; parents only pass membership/source callbacks. No backend changes.

**Tech Stack:** Svelte 5 + SvelteKit (`apps/web`), existing `$lib/api` (`listLatency` / `testLatency`), `$lib/latency` tone helpers, monorepo `locales/*.json`, verify with `node --experimental-strip-types --test` (pure logic) and `pnpm --dir apps/web check`.

**Spec:** `docs/superpowers/specs/2026-07-21-node-picker-latency-design.md`

## Global Constraints

- Only leaf **proxy nodes** get test controls (not subscription/group rows).
- Bulk action tests **visible (search-filtered)** node ids, not “checked members”.
- Do not auto-run latency on open; load cache via `GET /latency` only.
- Same thresholds as Nodes page: good `<200`, warn `200–500`, bad `>500` or not alive.
- While a test is in flight in a session, disable all test buttons in that session; keep checkboxes usable unless parent `busy` is true.
- Ignore stale responses when the picker unmounts or the session is reset (generation counter).
- Reuse existing i18n keys where possible; add `nodes.testVisible` in en + zh-CN.
- No REST/API contract changes.
- Match surrounding UI density/tokens; YAGNI: no sort-by-latency, no progress streaming.

---

## File map

| Path | Role |
|------|------|
| `apps/web/src/lib/latencySessionCore.ts` | Pure helpers: merge map, empty-ids guard, generation check |
| `apps/web/src/lib/latencySessionCore.test.ts` | Node test runner coverage for pure helpers |
| `apps/web/src/lib/latencySession.svelte.ts` | Reactive session factory used by UI |
| `apps/web/src/lib/components/features/NodePickList.svelte` | Shared node picker + latency UX |
| `apps/web/src/routes/(app)/groups/+page.svelte` | Wire `NodePickList` into member editor |
| `apps/web/src/lib/components/features/GroupDraftEditor.svelte` | Same list UX (keep in sync) |
| `apps/web/src/lib/components/features/OrchestrationInspector.svelte` | Node tab uses pick list / session |
| `locales/en.json`, `locales/zh-CN.json` | `nodes.testVisible` (+ count variant if used) |
| Optional later (not required): `nodes/+page.svelte` adopt session | Avoids duplicate merge only |

---

### Task 1: Pure latency session core + unit tests

**Files:**
- Create: `apps/web/src/lib/latencySessionCore.ts`
- Create: `apps/web/src/lib/latencySessionCore.test.ts`

**Interfaces:**
- Consumes: `LatencyDto` shape `{ id, latency_ms, alive, tested_at, message }` (mirror `$lib/api` fields; tests may use inline type)
- Produces:
  - `mergeLatencyMap(prev, results) → Record<string, LatencyDto>`
  - `shouldAcceptGeneration(active, response) → boolean`
  - `normalizeTestIds(ids: string[] | null | undefined) → string[] | null`  
    - `null` / `undefined` mean “all nodes” (API omit/null); empty array means “test none” (caller must not call API)

- [ ] **Step 1: Write the failing test file**

```ts
// apps/web/src/lib/latencySessionCore.test.ts
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
	mergeLatencyMap,
	shouldAcceptGeneration,
	normalizeTestIds
} from './latencySessionCore.ts';

type LatencyDto = {
	id: string;
	latency_ms: number | null;
	alive: boolean;
	tested_at: string;
	message: string | null;
};

function sample(id: string, ms: number | null, alive = true): LatencyDto {
	return { id, latency_ms: ms, alive, tested_at: 't', message: null };
}

describe('mergeLatencyMap', () => {
	it('merges by id without dropping other keys', () => {
		const prev = { a: sample('a', 10), b: sample('b', 20) };
		const next = mergeLatencyMap(prev, [sample('b', 99), sample('c', 5)]);
		assert.equal(next.a.latency_ms, 10);
		assert.equal(next.b.latency_ms, 99);
		assert.equal(next.c.latency_ms, 5);
	});
});

describe('shouldAcceptGeneration', () => {
	it('accepts matching generation only', () => {
		assert.equal(shouldAcceptGeneration(3, 3), true);
		assert.equal(shouldAcceptGeneration(3, 2), false);
	});
});

describe('normalizeTestIds', () => {
	it('keeps nullish as null (all nodes)', () => {
		assert.equal(normalizeTestIds(null), null);
		assert.equal(normalizeTestIds(undefined), null);
	});
	it('returns empty array as empty (caller skips request)', () => {
		assert.deepEqual(normalizeTestIds([]), []);
	});
	it('returns non-empty list as-is', () => {
		assert.deepEqual(normalizeTestIds(['x', 'y']), ['x', 'y']);
	});
});
```

- [ ] **Step 2: Run tests — expect FAIL (module missing)**

Run:

```bash
cd /home/chaos/projects/personal/chaos && node --experimental-strip-types --test apps/web/src/lib/latencySessionCore.test.ts
```

Expected: FAIL with cannot find module / ERR_MODULE_NOT_FOUND for `latencySessionCore.ts`.

- [ ] **Step 3: Implement pure helpers**

```ts
// apps/web/src/lib/latencySessionCore.ts
export type LatencyLike = {
	id: string;
	latency_ms: number | null;
	alive: boolean;
	tested_at: string;
	message: string | null;
};

export function mergeLatencyMap<T extends LatencyLike>(
	prev: Record<string, T>,
	results: T[]
): Record<string, T> {
	const next = { ...prev };
	for (const result of results) next[result.id] = result;
	return next;
}

export function shouldAcceptGeneration(active: number, response: number): boolean {
	return active === response;
}

/** null = test all; [] = test none (skip HTTP); non-empty = those ids */
export function normalizeTestIds(ids: string[] | null | undefined): string[] | null {
	if (ids === null || ids === undefined) return null;
	return ids;
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd /home/chaos/projects/personal/chaos && node --experimental-strip-types --test apps/web/src/lib/latencySessionCore.test.ts
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/latencySessionCore.ts apps/web/src/lib/latencySessionCore.test.ts
git commit -m "feat(web): pure latency map helpers for node pickers"
```

---

### Task 2: Reactive latency session factory

**Files:**
- Create: `apps/web/src/lib/latencySession.svelte.ts`
- Modify: none required beyond imports of core + api

**Interfaces:**
- Consumes: `mergeLatencyMap`, `shouldAcceptGeneration`, `normalizeTestIds`; `listLatency`, `testLatency`, `ApiClientError` from `$lib/api`; `apiErrorText`, `t` from `$lib/i18n.svelte`
- Produces: `createLatencySession()` returning:
  - `latencyById: Record<string, LatencyDto>` (readable via getters / `$state` fields on returned object)
  - `testing: string | null` (`'visible' | nodeId | null`)
  - `error: string`
  - `message: string`
  - `load(): Promise<void>`
  - `test(ids: string[] | null, marker: string): Promise<void>` — if `normalizeTestIds` yields `[]`, no-op without setting testing
  - `dispose(): void` — bumps generation so in-flight merges are ignored
  - `clearNotices(): void`

Implementation shape (Svelte 5 runes in `.svelte.ts`):

```ts
// apps/web/src/lib/latencySession.svelte.ts
import {
	listLatency,
	testLatency,
	ApiClientError,
	type LatencyDto
} from '$lib/api';
import { apiErrorText, t } from '$lib/i18n.svelte';
import {
	mergeLatencyMap,
	normalizeTestIds,
	shouldAcceptGeneration
} from '$lib/latencySessionCore';

export function createLatencySession() {
	let latencyById = $state<Record<string, LatencyDto>>({});
	let testing = $state<string | null>(null);
	let error = $state('');
	let message = $state('');
	let generation = 0;

	function dispose() {
		generation += 1;
		testing = null;
	}

	function clearNotices() {
		error = '';
		message = '';
	}

	async function load() {
		const gen = generation;
		error = '';
		try {
			const response = await listLatency();
			if (!shouldAcceptGeneration(generation, gen)) return;
			const map: Record<string, LatencyDto> = {};
			for (const result of response.results) map[result.id] = result;
			latencyById = map;
		} catch (cause) {
			if (!shouldAcceptGeneration(generation, gen)) return;
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed');
		}
	}

	async function test(ids: string[] | null, marker: string) {
		const normalized = normalizeTestIds(ids);
		if (Array.isArray(normalized) && normalized.length === 0) return;

		const gen = generation;
		testing = marker;
		error = '';
		message = '';
		try {
			const response = await testLatency(normalized);
			if (!shouldAcceptGeneration(generation, gen)) return;
			latencyById = mergeLatencyMap(latencyById, response.results);
			const alive = response.results.filter((result) => result.alive).length;
			message = t('dashboard.latencyFinished', {
				alive,
				total: response.results.length
			});
		} catch (cause) {
			if (!shouldAcceptGeneration(generation, gen)) return;
			error =
				cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed');
		} finally {
			if (shouldAcceptGeneration(generation, gen)) testing = null;
		}
	}

	return {
		get latencyById() {
			return latencyById;
		},
		get testing() {
			return testing;
		},
		get error() {
			return error;
		},
		get message() {
			return message;
		},
		load,
		test,
		dispose,
		clearNotices
	};
}

export type LatencySession = ReturnType<typeof createLatencySession>;
```

- [ ] **Step 1: Create the file with the factory above**

- [ ] **Step 2: Typecheck**

Run:

```bash
cd /home/chaos/projects/personal/chaos && pnpm --dir apps/web check
```

Expected: no new errors from `latencySession.svelte.ts` (existing project errors if any should be noted, not introduced by this file).

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/latencySession.svelte.ts
git commit -m "feat(web): reactive latency session for pickers"
```

---

### Task 3: i18n keys for “test visible”

**Files:**
- Modify: `locales/en.json`
- Modify: `locales/zh-CN.json`

**Interfaces:**
- Produces keys:
  - `nodes.testVisible`: EN `Test visible`, ZH `测试可见节点`
  - `nodes.testVisibleCount`: EN `Test {count} visible`, ZH `测试可见 {count} 项`

- [ ] **Step 1: Add keys next to existing `nodes.testSelected`**

In `locales/en.json` after `"nodes.testSelected"`:

```json
"nodes.testVisible": "Test visible",
"nodes.testVisibleCount": "Test {count} visible",
```

In `locales/zh-CN.json`:

```json
"nodes.testVisible": "测试可见节点",
"nodes.testVisibleCount": "测试可见 {count} 项",
```

- [ ] **Step 2: Confirm JSON validity**

```bash
node -e "JSON.parse(require('fs').readFileSync('locales/en.json','utf8')); JSON.parse(require('fs').readFileSync('locales/zh-CN.json','utf8')); console.log('ok')"
```

Expected: `ok`

- [ ] **Step 3: Commit**

```bash
git add locales/en.json locales/zh-CN.json
git commit -m "i18n: add test-visible labels for node pickers"
```

---

### Task 4: `NodePickList` feature component

**Files:**
- Create: `apps/web/src/lib/components/features/NodePickList.svelte`

**Interfaces:**
- Consumes: `createLatencySession`, `latencyTone` / `formatLatencyMs` / `latencyClass`, `Button`, `SearchInput`, `EmptyState` (optional), `t`, `Gauge` icon, `NodeDto`
- Produces props:

```ts
{
  nodes: NodeDto[];
  busy?: boolean;
  isChecked: (node: NodeDto) => boolean;
  onToggle: (node: NodeDto, checked: boolean) => void;
  showWeight?: boolean;
  getWeight?: (node: NodeDto) => number;
  onWeight?: (node: NodeDto, weight: number) => void;
  emptyTitle?: string;
  emptyDescription?: string;
  /** Extra toolbar actions (e.g. select all / clear) rendered beside Test visible */
  toolbar?: import('svelte').Snippet;
}
```

Also export nothing else; parent reads notices via optional bindable events **or** the component surfaces `error`/`message` with internal `Notice` at top of the list block.

Preferred: component owns session + shows compact Notice inside itself so parents stay thin.

- [ ] **Step 1: Implement component structure**

```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { Gauge } from '@lucide/svelte';
	import type { NodeDto } from '$lib/api';
	import { createLatencySession } from '$lib/latencySession.svelte';
	import { formatLatencyMs, latencyClass, latencyTone } from '$lib/latency';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';

	let {
		nodes,
		busy = false,
		isChecked,
		onToggle,
		showWeight = false,
		getWeight,
		onWeight,
		emptyLabel,
		toolbar
	}: {
		nodes: NodeDto[];
		busy?: boolean;
		isChecked: (node: NodeDto) => boolean;
		onToggle: (node: NodeDto, checked: boolean) => void;
		showWeight?: boolean;
		getWeight?: (node: NodeDto) => number;
		onWeight?: (node: NodeDto, weight: number) => void;
		emptyLabel?: string;
		toolbar?: import('svelte').Snippet;
	} = $props();

	const session = createLatencySession();
	let query = $state('');

	const filteredNodes = $derived.by(() => {
		const normalized = query.trim().toLowerCase();
		if (!normalized) return nodes;
		return nodes.filter((node) =>
			[node.name, node.tag, node.protocol, node.address]
				.filter(Boolean)
				.some((value) => String(value).toLowerCase().includes(normalized))
		);
	});

	onMount(() => {
		void session.load();
		return () => session.dispose();
	});

	function clampWeight(value: number) {
		return Math.max(1, Math.min(99, Math.floor(value) || 1));
	}
</script>

<div class="node-pick-list">
	{#if session.error}
		<Notice tone="error" message={session.error} ondismiss={() => session.clearNotices()} />
	{/if}
	{#if session.message}
		<Notice tone="success" message={session.message} ondismiss={() => session.clearNotices()} />
	{/if}

	<div class="node-pick-toolbar">
		<SearchInput bind:value={query} placeholder={t('flow.searchNodes')} />
		<div class="node-pick-actions">
			{#if toolbar}{@render toolbar()}{/if}
			<Button
				size="sm"
				icon={Gauge}
				loading={session.testing === 'visible'}
				disabled={busy || !!session.testing || !filteredNodes.length}
				onclick={() =>
					void session.test(
						filteredNodes.map((node) => node.id),
						'visible'
					)}
			>
				{filteredNodes.length
					? t('nodes.testVisibleCount', { count: filteredNodes.length })
					: t('nodes.testVisible')}
			</Button>
		</div>
	</div>

	<div class="node-pick-rows">
		{#each filteredNodes as node (node.id)}
			{@const checked = isChecked(node)}
			{@const latency = session.latencyById[node.id]}
			{@const tone = latency ? latencyTone(latency.latency_ms, latency.alive) : 'unknown'}
			<div class="node-pick-row" class:active={checked}>
				<label class="node-pick-check">
					<input
						type="checkbox"
						checked={checked}
						disabled={busy}
						onchange={(event) =>
							onToggle(node, (event.currentTarget as HTMLInputElement).checked)}
					/>
					<span>
						<strong>{node.name}</strong>
						<small>
							{[node.protocol, node.address].filter(Boolean).join(' / ') || t('common.unknown')}
						</small>
					</span>
				</label>
				<span
					class="node-pick-latency {latencyClass(tone)}"
					title={latency?.message ?? ''}
				>
					{latency ? formatLatencyMs(latency.latency_ms, latency.alive) : t('common.emDash')}
				</span>
				{#if showWeight && checked && getWeight && onWeight}
					<label class="node-pick-weight">
						<span>{t('flow.weight')}</span>
						<input
							type="number"
							min="1"
							max="99"
							value={getWeight(node)}
							disabled={busy}
							onchange={(event) =>
								onWeight(
									node,
									clampWeight(Number((event.currentTarget as HTMLInputElement).value))
								)}
						/>
					</label>
				{/if}
				<Button
					size="sm"
					variant="ghost"
					icon={Gauge}
					loading={session.testing === node.id}
					disabled={busy || !!session.testing}
					aria-label={t('nodes.testNode', { name: node.name })}
					title={t('nodes.testNode', { name: node.name })}
					onclick={() => void session.test([node.id], node.id)}
				/>
			</div>
		{/each}
		{#if !filteredNodes.length}
			<div class="node-pick-empty">
				{emptyLabel ??
					(nodes.length ? t('common.noSearchResults') : t('flow.emptyPool'))}
			</div>
		{/if}
	</div>
</div>

<style>
	.node-pick-list {
		display: grid;
		gap: var(--space-3);
		min-width: 0;
	}
	.node-pick-toolbar {
		display: grid;
		gap: var(--space-2);
	}
	.node-pick-actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		align-items: center;
	}
	.node-pick-rows {
		display: grid;
		gap: 0.35rem;
		max-height: 22rem;
		overflow: auto;
		padding-right: 0.15rem;
	}
	.node-pick-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto auto auto;
		align-items: center;
		gap: var(--space-2);
		padding: 0.45rem 0.55rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm, 6px);
		background: var(--surface);
	}
	.node-pick-row.active {
		border-color: var(--ink);
	}
	.node-pick-check {
		display: flex;
		align-items: flex-start;
		gap: 0.55rem;
		min-width: 0;
	}
	.node-pick-check span {
		display: grid;
		gap: 0.1rem;
		min-width: 0;
	}
	.node-pick-check strong,
	.node-pick-check small {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.node-pick-check small {
		color: var(--muted, #888);
		font-size: 0.8em;
	}
	.node-pick-latency {
		font-variant-numeric: tabular-nums;
		font-size: 0.85em;
		white-space: nowrap;
	}
	.node-pick-weight {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8em;
	}
	.node-pick-weight input {
		width: 3.5rem;
	}
	.node-pick-empty {
		padding: var(--space-3);
		color: var(--muted, #888);
		text-align: center;
		font-size: 0.9em;
	}
</style>
```

Adjust class names if parent already supplies outer scroll container (e.g. inspector): prefer `max-height` that matches existing member list (~same as groups form).

- [ ] **Step 2: Typecheck**

```bash
pnpm --dir apps/web check
```

Expected: `NodePickList.svelte` clean.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/components/features/NodePickList.svelte
git commit -m "feat(web): NodePickList with latency test actions"
```

---

### Task 5: Wire Groups page member editor

**Files:**
- Modify: `apps/web/src/routes/(app)/groups/+page.svelte`

**Interfaces:**
- Consumes: `NodePickList`
- Produces: same `members` / `toggleMember` / `setMemberWeight` behavior as today

- [ ] **Step 1: Import `NodePickList`**

Add:

```ts
import NodePickList from '$lib/components/features/NodePickList.svelte';
```

- [ ] **Step 2: Replace the member-list block**

Replace the block that currently contains `SearchInput` + `{#each filteredNodes ...}` member rows (approx. the `.member-editor` body) with:

```svelte
<div class="member-editor">
	<div class="member-editor-header">
		<div>
			<strong>{t('groups.members')}</strong>
			<span>{t('groups.memberCount', { count: members.length })}</span>
		</div>
	</div>
	<NodePickList
		{nodes}
		{busy}
		isChecked={(node) => members.some((item) => item.node_id === node.id)}
		onToggle={toggleMember}
		showWeight={true}
		getWeight={(node) => members.find((item) => item.node_id === node.id)?.weight ?? 1}
		onWeight={(node, weight) => setMemberWeight(node.id, weight)}
	/>
</div>
```

- [ ] **Step 3: Remove now-unused `memberQuery` / `filteredNodes` if only used by the old list**

Delete local state and derived that only served the old member search list. Keep other form state.

- [ ] **Step 4: Typecheck**

```bash
pnpm --dir apps/web check
```

- [ ] **Step 5: Manual smoke (if API up)**

1. Open Groups → edit/create → member list shows latency or `—`.
2. Click row gauge → latency updates.
3. Click test visible → multiple rows update.
4. Toggle membership and weight while idle; confirm save still works.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/routes/\(app\)/groups/+page.svelte
git commit -m "feat(groups): latency test in member picker"
```

---

### Task 6: Wire `GroupDraftEditor`

**Files:**
- Modify: `apps/web/src/lib/components/features/GroupDraftEditor.svelte`

**Interfaces:**
- Same as Groups: `NodePickList` with weight; keep `selectAllFiltered` / `clearMembers` via `toolbar` snippet **or** parent actions above the list.

Because `selectAllFiltered` currently depends on local `filteredNodes`, either:

**Option A (recommended):** move “select visible” into parent by accepting a callback from `NodePickList` — out of scope for minimal change.

**Option B (this plan):** Keep section-level Select visible / Clear buttons that operate on **all nodes matching a local query is removed** — instead:

1. Select visible becomes “select all in pool” using `nodes` prop when search is inside `NodePickList` (search is internal).

Simplest correct behavior for this task:

- Toolbar snippet with:
  - `Select all` → select every node in `nodes` (full pool), not search-filtered (document this slight behavior change), **or**
  - Drop “select visible” and only keep Clear + rely on pick list search + checkboxes.

**Plan choice (explicit):** Keep Clear; change Select visible to **select all nodes in the `nodes` prop** labeled with existing `common.selectVisible` only if product accepts; better: add optional `onSelectAll` that selects full pool and label `common.selectAll` if key exists.

Check locales for `common.selectVisible` — already used. Keep button text; implement as “select all currently loaded nodes” (`nodes` array). Document in commit message: search is internal to pick list so select-all applies to full pool.

```svelte
{#snippet actions()}
  <Button size="sm" disabled={busy || !nodes.length} onclick={selectAllFiltered}>
    {t('common.selectVisible')}
  </Button>
  ...
{/snippet}

<!-- members body -->
<NodePickList
  {nodes}
  {busy}
  isChecked={(node) => !!(selectedGroup.members?.some((m) => m.node_id === node.id))}
  onToggle={(node, checked) => toggleMember(selectedGroup.id, node, checked)}
  showWeight={true}
  getWeight={(node) =>
    selectedGroup.members?.find((m) => m.node_id === node.id)?.weight ?? 1}
  onWeight={(node, weight) => setWeight(selectedGroup.id, node.id, weight)}
/>
```

Update `selectAllFiltered` to iterate `nodes` instead of `filteredNodes`; remove local `nodeQuery` / `filteredNodes` if unused.

- [ ] **Step 1: Apply replacement as above**
- [ ] **Step 2: `pnpm --dir apps/web check`**
- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/components/features/GroupDraftEditor.svelte
git commit -m "feat(groups): latency-aware draft member list"
```

---

### Task 7: Wire Orchestration inspector node tab

**Files:**
- Modify: `apps/web/src/lib/components/features/OrchestrationInspector.svelte`

**Interfaces:**
- Consumes: `NodePickList`, existing `toggleSource` / `isSelected` / `inventoryNodes`
- Node tab only; subscription/group tabs unchanged

- [ ] **Step 1: Import `NodePickList`**

- [ ] **Step 2: In the `node_group` resource section, branch on `sourceTab`**

When `sourceTab === 'node'`:

```svelte
<section class="inspector-section resource-section">
	<SegmentedControl bind:value={sourceTab} options={sourceTabs} label={t('flow.inspector.sourceType')} />
	{#if sourceTab === 'node'}
		<NodePickList
			nodes={inventoryNodes}
			{busy}
			isChecked={(node) =>
				selectedSources.some((source) => source.kind === 'node' && source.id === node.id)}
			onToggle={(node, checked) =>
				toggleSource({ kind: 'node', id: node.id, name: node.name, meta: '' }, checked)}
			showWeight={false}
		/>
	{:else}
		<SearchInput bind:value={query} placeholder={t('flow.inspector.searchResources')} />
		<div class="resource-list">
			{#each resourceOptions as option (`${option.kind}:${option.id}`)}
				<label class:selected={isSelected(option)}>
					<input
						type="checkbox"
						checked={isSelected(option)}
						disabled={busy}
						onchange={(event) =>
							toggleSource(option, (event.currentTarget as HTMLInputElement).checked)}
					/>
					<span><strong>{option.name}</strong><small>{option.meta}</small></span>
				</label>
			{/each}
			{#if !resourceOptions.length}
				<div class="compact-empty">{t('common.noSearchResults')}</div>
			{/if}
		</div>
	{/if}
</section>
```

Note: weights for node sources remain editable in **selected sources** strip (existing inputs). Do not duplicate weight in pick list for orchestrate.

- [ ] **Step 3 (optional polish, same commit if small): selected source latency badge**

For each selected source with `kind === 'node'`, if you already have a shared map, show ms — **skip if it requires lifting session state**. Spec marks this optional; **default: skip** to avoid dual sessions. Prefer single session only inside `NodePickList`.

- [ ] **Step 4: Typecheck**

```bash
pnpm --dir apps/web check
```

- [ ] **Step 5: Manual smoke**

1. Orchestrate → select a node_group → Nodes tab → latency + test visible + row test.
2. Switch to Subscriptions tab → no gauge test chrome on sub rows.
3. Toggle node source on/off; weight still works in selected strip; draft still dirty/save as before.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/components/features/OrchestrationInspector.svelte
git commit -m "feat(flow): latency test in node source picker"
```

---

### Task 8: Final verification + spec checklist

**Files:** none new

- [ ] **Step 1: Re-run pure unit tests**

```bash
node --experimental-strip-types --test apps/web/src/lib/latencySessionCore.test.ts
```

Expected: PASS

- [ ] **Step 2: Full web check**

```bash
pnpm --dir apps/web check
```

Expected: no errors introduced by this feature

- [ ] **Step 3: Spec coverage checklist (manual)**

| Spec requirement | Task |
|------------------|------|
| Cached latency on proxy node pickers | 4–7 |
| Per-row test | 4–7 |
| Test visible (filtered) | 4 (`NodePickList`) |
| Groups member editor | 5 |
| GroupDraftEditor | 6 |
| Orchestration node tab | 7 |
| No auto-test on open | 4 (`load` only) |
| No backend change | (all) |
| Shared session / no triple paste | 1–2, 4 |
| Stale generation ignore | 2 |
| i18n en+zh | 3 |

- [ ] **Step 4: Final commit only if uncommitted polish remains; otherwise done**

If only docs note needed, skip empty commit.

---

## Plan self-review

1. **Spec coverage:** Goals, non-goals, architecture, data flow, concurrency, i18n, file list, success criteria each map to Tasks 1–8. Optional selected-source badge intentionally deferred to avoid dual sessions.
2. **Placeholders:** None; code and commands are concrete. GroupDraftEditor select-all behavior change is explicit.
3. **Type consistency:** `createLatencySession` / `LatencySession`, `mergeLatencyMap`, `NodePickList` props consistent across tasks. `testLatency` receives `string[] | null` after normalize; empty array skips call.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-21-node-picker-latency.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — this session with executing-plans and checkpoints  

Which approach?
