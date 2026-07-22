# Global Toast Feedback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every page-inline operation `Notice` banner with the existing global Toast stack so success, failure, validation, and durable status feedback appear top-right site-wide.

**Architecture:** Tune default durations on `toast` tone helpers; call `toast.success` / `toast.error` / `toast.warning` / `toast.info` at former `message`/`error` assignment sites; remove local banner state and markup; drive durable conditions (`needs-republish`, auth expired, home API, routing readonly) with stable ids and `duration: 0`, dismissing when the condition clears. No new notify facade.

**Tech Stack:** Svelte 5 + SvelteKit (`apps/web`), `$lib/toast.svelte.ts`, `ToastViewport` (already in root layout), design system doc, verify with `pnpm --dir apps/web check`.

**Spec:** `docs/superpowers/specs/2026-07-22-global-toast-feedback-design.md`

## Global Constraints

- Reuse existing i18n strings; no new locale keys required.
- Do not introduce a third-party toast library.
- Do not replace `ConfirmDialog` with Toast.
- Keep `Notice.svelte` in the tree (secondary/optional); remove page call sites only.
- Stable id `needs-republish` is **shared** across dashboard / subscriptions / orchestrate (page may keep its own copy key for `title`).
- Error and warning default `duration: 0` (manual dismiss); success defaults to 4000 ms; info/default 5000 ms.
- Match surrounding code style (orchestrate already uses `toast.warning({ id: 'flow-validation', ... })`).
- YAGNI: no route-change queue clear, no animation redesign, no notify wrapper.

---

## File map

| Path | Role |
|------|------|
| `apps/web/src/lib/toast.svelte.ts` | Duration defaults per tone helper |
| `apps/web/src/lib/toast.defaults.test.ts` | Pure unit tests for default duration resolution |
| `docs/design-system/MASTER.md` | Toast as default async feedback; Notice demoted |
| `apps/web/src/routes/(app)/orchestrate/+page.svelte` | Success/error/needsRepublish → toast |
| `apps/web/src/routes/(app)/dashboard/+page.svelte` | Same pattern |
| `apps/web/src/routes/(app)/dns/+page.svelte` | Same |
| `apps/web/src/routes/(app)/groups/+page.svelte` | Same |
| `apps/web/src/routes/(app)/nodes/+page.svelte` | Same |
| `apps/web/src/routes/(app)/subscriptions/+page.svelte` | Same + durable republish |
| `apps/web/src/routes/(app)/settings/+page.svelte` | Same |
| `apps/web/src/routes/(app)/routing/+page.svelte` | Error + read-only info |
| `apps/web/src/routes/login/+page.svelte` | Error + expired |
| `apps/web/src/routes/setup/+page.svelte` | Error / validation |
| `apps/web/src/routes/+page.svelte` | Home API unreachable |

---

### Task 1: Toast default durations + unit tests

**Files:**
- Modify: `apps/web/src/lib/toast.svelte.ts`
- Create: `apps/web/src/lib/toast.defaults.test.ts`

**Interfaces:**
- Consumes: existing `ToastOptions` / `ToastItem` / `toast` API
- Produces:
  - `resolveDuration(tone, duration?) → number` exported for tests (or test via `toast` after show if pure helper preferred)
  - Defaults: success `4000`, info/unspecified `5000`, warning `0`, error `0`
  - Explicit `duration` in options always wins

- [ ] **Step 1: Write the failing test**

```ts
// apps/web/src/lib/toast.defaults.test.ts
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { resolveDuration } from './toast.svelte.ts';

describe('resolveDuration', () => {
	it('uses success default 4000 when omitted', () => {
		assert.equal(resolveDuration('success'), 4000);
	});
	it('uses info default 5000 when omitted', () => {
		assert.equal(resolveDuration('info'), 5000);
	});
	it('uses warning and error default 0 when omitted', () => {
		assert.equal(resolveDuration('warning'), 0);
		assert.equal(resolveDuration('error'), 0);
	});
	it('honors explicit duration including zero', () => {
		assert.equal(resolveDuration('success', 0), 0);
		assert.equal(resolveDuration('error', 8000), 8000);
	});
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --experimental-strip-types --test apps/web/src/lib/toast.defaults.test.ts`  
Expected: FAIL (export / defaults not present)

- [ ] **Step 3: Implement duration defaults**

Update `apps/web/src/lib/toast.svelte.ts` to:

```ts
export type ToastTone = 'info' | 'success' | 'warning' | 'error';

export type ToastOptions = {
	id?: string;
	title: string;
	description?: string;
	tone?: ToastTone;
	duration?: number;
	action?: { label: string; onclick: () => void };
};

export type ToastItem = ToastOptions & {
	id: string;
	tone: ToastTone;
	duration: number;
};

const DEFAULT_DURATION = 5000;
const SUCCESS_DURATION = 4000;
const PERSISTENT_DURATION = 0;
const MAX_VISIBLE_TOASTS = 4;

export function resolveDuration(tone: ToastTone, duration?: number): number {
	if (duration !== undefined) return duration;
	if (tone === 'success') return SUCCESS_DURATION;
	if (tone === 'warning' || tone === 'error') return PERSISTENT_DURATION;
	return DEFAULT_DURATION;
}

let nextId = 0;
let items = $state<ToastItem[]>([]);

function create(options: ToastOptions): string {
	const id = options.id ?? `toast-${++nextId}`;
	const tone = options.tone ?? 'info';
	const item: ToastItem = {
		...options,
		id,
		tone,
		duration: resolveDuration(tone, options.duration)
	};
	const existing = items.findIndex((toast) => toast.id === id);
	items =
		existing === -1
			? [...items.slice(-(MAX_VISIBLE_TOASTS - 1)), item]
			: items.map((toast) => (toast.id === id ? item : toast));
	return id;
}

function dismiss(id: string) {
	if (!items.some((toast) => toast.id === id)) return;
	items = items.filter((toast) => toast.id !== id);
}

function clear() {
	if (!items.length) return;
	items = [];
}

export const toast = {
	get items() {
		return items;
	},
	show: create,
	dismiss,
	clear,
	info: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'info' }),
	success: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'success' }),
	warning: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'warning' }),
	error: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'error' })
};
```

Note: if importing `$state` module under node test fails, keep `resolveDuration` in a tiny pure file `toastDuration.ts` re-exported from `toast.svelte.ts` and test only that file. Prefer pure extract if strip-types chokes on `.svelte.ts` runes.

- [ ] **Step 4: Run tests to verify they pass**

Run: `node --experimental-strip-types --test apps/web/src/lib/toast.defaults.test.ts`  
(or `.../toastDuration.test.ts` if extracted)  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/toast.svelte.ts apps/web/src/lib/toast.defaults.test.ts apps/web/src/lib/toastDuration.ts apps/web/src/lib/toastDuration.test.ts 2>/dev/null || true
git add apps/web/src/lib/toast*.ts
git commit -m "feat(web): default toast durations by tone"
```

---

### Task 2: Design system contract

**Files:**
- Modify: `docs/design-system/MASTER.md`

**Interfaces:**
- Consumes: Task 1 duration rules
- Produces: documented Toast / Notice roles for implementers

- [ ] **Step 1: Update component contracts table**

In the Component Contracts table:

- Change `Notice` row purpose to: `Optional inline feedback (prefer Toast for operation results)` with states `Info, success, error, dismissible`.
- Add row: `| Toast / ToastViewport | Global operation and status notifications | Info, success, warning, error, dismissible, optional action, auto-dismiss by tone |`

- [ ] **Step 2: Update Async operations section**

Replace the Notice bullet with:

```markdown
- Success and failure feedback use global `Toast` (`toast.success` / `toast.error` / `toast.warning` / `toast.info`) via `ToastViewport` in the root layout.
- Success toasts auto-dismiss (~4s). Warning and error toasts default to manual dismiss (`duration: 0`).
- Durable conditions (e.g. needs republish) use a stable toast `id` and are dismissed when the condition clears.
- `Notice` remains available for rare page-local inline copy but is not the default for save/apply/delete outcomes.
- User input is preserved after a failed request.
- A bulk import keeps failed links in the editor for correction and retry.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design-system/MASTER.md
git commit -m "docs(design-system): Toast is default operation feedback"
```

---

### Task 3: Orchestrate page migration

**Files:**
- Modify: `apps/web/src/routes/(app)/orchestrate/+page.svelte`

**Interfaces:**
- Consumes: `toast` from `$lib/toast.svelte`; stable id `needs-republish`; existing `flow-validation`
- Produces: no page-level success/error `Notice`; durable republish toast

- [ ] **Step 1: Remove banner state usage for message/error**

1. Keep `import { toast } from '$lib/toast.svelte'` (already present).
2. Remove `import Notice from '...Notice.svelte'` if unused after step 3.
3. Remove `let error = $state('')` and `let message = $state('')` if nothing else reads them.
4. Delete all `error = ''` / `message = ''` clear lines that only served banners.
5. Replace assignments:

| Before | After |
|--------|--------|
| `message = t('flow.draftSaved')` | `toast.success({ title: t('flow.draftSaved') })` |
| `message = t('flow.applied', { nodes: ... })` | `toast.success({ title: t('flow.applied', { nodes: ... }) })` |
| `message = t('flow.draftSavedApplyFailed')` | `toast.warning({ title: t('flow.draftSavedApplyFailed') })` — draft persisted but apply failed |
| `error = ...` (any) | `toast.error({ title: <same string> })` |

For multi-line validation join that previously set `error = validation.issues...`, use:

```ts
toast.error({
	title: t('flow.issueCount', { count: validation.issues.length }),
	description: validation.issues.map((i) => i.message).join('\n')
});
// or keep the exact previous joined string as title if that was the UX
```

Prefer **exact previous string** as `title` so copy does not change.

- [ ] **Step 2: Durable needsRepublish**

After load / whenever `needsRepublish` is set, and after apply clears it, sync:

```ts
function syncNeedsRepublishToast(active: boolean) {
	if (active) {
		toast.warning({
			id: 'needs-republish',
			title: t('flow.needsRepublish'),
			duration: 0
		});
	} else {
		toast.dismiss('needs-republish');
	}
}
```

Call when:

- document load sets `needsRepublish = document.needs_republish === true` → `syncNeedsRepublishToast(needsRepublish)`
- apply success sets `needsRepublish = false` → `syncNeedsRepublishToast(false)`

Optional: `$effect(() => { syncNeedsRepublishToast(needsRepublish); })` if the flag has multiple writers — only one approach, avoid double-calling create on every unrelated render by using `$effect` **or** explicit calls, not both.

- [ ] **Step 3: Remove Notice markup**

Delete:

```svelte
{#if error}<Notice ... />{/if}
{#if message}<Notice ... />{/if}
{#if needsRepublish}<Notice ... />{/if}
```

Leave validation toast (`showValidationToast`) as-is (already toast).

- [ ] **Step 4: Typecheck**

Run: `pnpm --dir apps/web check`  
Expected: no errors from orchestrate page

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/\(app\)/orchestrate/+page.svelte
git commit -m "feat(web): orchestrate feedback uses global Toast"
```

---

### Task 4: Dashboard + subscriptions (durable republish)

**Files:**
- Modify: `apps/web/src/routes/(app)/dashboard/+page.svelte`
- Modify: `apps/web/src/routes/(app)/subscriptions/+page.svelte`

**Interfaces:**
- Consumes: `toast`, id `needs-republish`
- Produces: same durable toast semantics as orchestrate

- [ ] **Step 1: Dashboard migration**

1. `import { toast } from '$lib/toast.svelte'`; remove `Notice` import when unused.
2. Remove `error` / `message` state.
3. Map:

| Event | Call |
|-------|------|
| load fail | `toast.error({ title: ... })` |
| latency finished | `toast.success({ title: t('dashboard.latencyFinished', ...) })` |
| latency fail | `toast.error` |
| applied | `toast.success({ title: t('dashboard.appliedWithMethod', ...) })` |
| apply fail | `toast.error` |
| stop requested | `toast.success` |
| stop fail | `toast.error` |
| geoip/geosite updated | `toast.success` |
| geo update fail | `toast.error` |

4. Durable republish:

```ts
$effect(() => {
	if (runtime?.needs_republish) {
		toast.warning({
			id: 'needs-republish',
			title: t('dashboard.needsRepublish'),
			duration: 0
		});
	} else if (runtime) {
		// only dismiss when we know runtime is loaded and flag is false
		toast.dismiss('needs-republish');
	}
});
```

If `$effect` + `runtime` null on first paint would incorrectly dismiss another page's toast, **only dismiss when apply/load returns `needs_republish: false`**, and show when true — same as orchestrate explicit sync.

Safer pattern (prefer this):

```ts
function syncNeedsRepublishToast(active: boolean | undefined) {
	if (active) {
		toast.warning({ id: 'needs-republish', title: t('dashboard.needsRepublish'), duration: 0 });
	} else if (active === false) {
		toast.dismiss('needs-republish');
	}
}
// call after loadRuntime / apply with known boolean
```

5. Remove the three `<Notice>` blocks.

- [ ] **Step 2: Subscriptions migration**

Same as dashboard for import/refresh/delete/load/url validation errors → toast.

For `const needsRepublish = $derived(...)`:

```ts
$effect(() => {
	if (needsRepublish) {
		toast.warning({
			id: 'needs-republish',
			title: t('subscriptions.needsRepublish'),
			duration: 0
		});
	} else {
		toast.dismiss('needs-republish');
	}
});
```

Only run dismiss when subscriptions have finished loading (guard with a `loaded` flag if empty list would dismiss too early). Pattern:

```ts
let loaded = $state(false);
// after successful list: loaded = true
$effect(() => {
	if (!loaded) return;
	if (needsRepublish) {
		toast.warning({ id: 'needs-republish', title: t('subscriptions.needsRepublish'), duration: 0 });
	} else {
		toast.dismiss('needs-republish');
	}
});
```

- [ ] **Step 3: Typecheck**

Run: `pnpm --dir apps/web check`  
Expected: pass for these pages

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/\(app\)/dashboard/+page.svelte apps/web/src/routes/\(app\)/subscriptions/+page.svelte
git commit -m "feat(web): dashboard and subscriptions use global Toast"
```

---

### Task 5: Resource pages (dns, groups, nodes, settings, routing)

**Files:**
- Modify: `apps/web/src/routes/(app)/dns/+page.svelte`
- Modify: `apps/web/src/routes/(app)/groups/+page.svelte`
- Modify: `apps/web/src/routes/(app)/nodes/+page.svelte`
- Modify: `apps/web/src/routes/(app)/settings/+page.svelte`
- Modify: `apps/web/src/routes/(app)/routing/+page.svelte`

**Interfaces:**
- Consumes: `toast`
- Produces: no top-of-page Notice for operation feedback

- [ ] **Step 1: Apply the same mechanical migration to each file**

For **each** file:

1. Add `import { toast } from '$lib/toast.svelte'`.
2. Remove `Notice` import if unused.
3. Remove `let error` / `let message` if only used for banners.
4. Replace every `error = <string>` with `toast.error({ title: <string> })`.
5. Replace every `message = <string>` with `toast.success({ title: <string> })`.
6. Delete clear lines `error = ''; message = '';` that only prepared for a new banner (optional keep if other logic depends — prefer delete).
7. Delete `{#if error}<Notice...` / `{#if message}<Notice...` blocks.

**Routing-specific:**

```ts
// load fail
toast.error({ title: ... });

// read-only notice once when page has data
toast.info({
	id: 'routing-readonly',
	title: t('routing.readOnlyNotice'),
	duration: 0
});
// onDestroy or when leaving is optional; duration 0 stays until dismiss — acceptable per spec
```

If read-only should not stick forever across navigation, use `duration: 8000` or dismiss in `onDestroy`:

```ts
import { onDestroy } from 'svelte';
onDestroy(() => toast.dismiss('routing-readonly'));
```

Prefer **dismiss on destroy** so other pages stay clean.

- [ ] **Step 2: Typecheck**

Run: `pnpm --dir apps/web check`  
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/\(app\)/dns/+page.svelte \
  apps/web/src/routes/\(app\)/groups/+page.svelte \
  apps/web/src/routes/\(app\)/nodes/+page.svelte \
  apps/web/src/routes/\(app\)/settings/+page.svelte \
  apps/web/src/routes/\(app\)/routing/+page.svelte
git commit -m "feat(web): resource pages use global Toast feedback"
```

---

### Task 6: Auth + home pages

**Files:**
- Modify: `apps/web/src/routes/login/+page.svelte`
- Modify: `apps/web/src/routes/setup/+page.svelte`
- Modify: `apps/web/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `toast`; ids `auth-expired`, `home-api`
- Produces: no form-top Notice strips for operation feedback

- [ ] **Step 1: Login**

```ts
import { toast } from '$lib/toast.svelte';
// remove Notice import when unused
// remove let error if unused

// on API down / login failed:
toast.error({ title: cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.login.failed') });

// expired session (show once when expired flag true):
$effect(() => {
	if (expired) {
		toast.info({ id: 'auth-expired', title: t('auth.login.expired'), duration: 0 });
	} else {
		toast.dismiss('auth-expired');
	}
});
```

Remove:

```svelte
{#if expired}<Notice ... />{/if}
{#if error}<Notice ... />{/if}
```

Field-level errors (if any) stay on `Field`; page Notice copy moves to toast only.

- [ ] **Step 2: Setup**

Replace password/API/setup failures:

```ts
toast.error({ title: t('auth.setup.passwordTooShort') });
toast.error({ title: t('auth.setup.passwordMismatch') });
toast.error({ title: ... api ... });
```

Remove page `<Notice>` for `error`. Keep `Field` `error={confirmError}` highlight if present.

- [ ] **Step 3: Home**

```ts
import { toast } from '$lib/toast.svelte';

// on unreachable:
toast.error({
	id: 'home-api',
	title: t('home.apiUnreachable', { error: detail }),
	duration: 0
});
// on success probe if any:
toast.dismiss('home-api');
```

Remove `message` state and `<Notice>`.

- [ ] **Step 4: Typecheck**

Run: `pnpm --dir apps/web check`  
Expected: pass

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/login/+page.svelte apps/web/src/routes/setup/+page.svelte apps/web/src/routes/+page.svelte
git commit -m "feat(web): auth and home feedback use global Toast"
```

---

### Task 7: Residual Notice audit + final verification

**Files:**
- Possibly none; or fix any remaining `Notice` operation banners under `apps/web/src`

**Interfaces:**
- Consumes: full migration
- Produces: green check; zero operation-banner Notices left (optional Notice ok only if intentional)

- [ ] **Step 1: Search remaining Notice usage**

Run:

```bash
rg -n "Notice" apps/web/src --glob '*.svelte'
```

Expected: either zero route usages, or only non-operation leftovers. Convert any remaining operation banners to toast following Tasks 3–6 patterns.

Confirm `ToastViewport` still in `apps/web/src/routes/+layout.svelte`.

- [ ] **Step 2: Full check**

Run: `pnpm --dir apps/web check`  
Expected: exit 0

Optional: `pnpm --dir apps/web build` if CI requires it.

- [ ] **Step 3: Manual smoke (if browser available)**

1. Orchestrate save draft / publish → top-right success toast; no full-width banner.
2. Force error (stop API) → sticky error toast until X.
3. Trigger needs_republish → warning id `needs-republish`; apply → dismissed.
4. Login wrong password → error toast.

- [ ] **Step 4: Final commit if any leftover fixes**

```bash
git add -A apps/web docs/design-system
git status
# commit only if dirty:
git commit -m "chore(web): finish global Toast migration cleanup"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| success duration 4000 / error+warning 0 / info 5000 | Task 1 |
| export/use resolveDuration; explicit override | Task 1 |
| MASTER.md Toast default + Notice demoted | Task 2 |
| orchestrate success/error/needsRepublish | Task 3 |
| keep flow-validation toast | Task 3 (no change) |
| dashboard + subscriptions durable id | Task 4 |
| dns/groups/nodes/settings/routing | Task 5 |
| login/setup/home + stable ids | Task 6 |
| residual audit + check | Task 7 |
| no new i18n keys | all tasks |
| no sonner / no ConfirmDialog change | constraints |

## Self-review notes

- No TBD / TODO placeholders in task steps.
- `flow.draftSavedApplyFailed` maps to `toast.warning` (partial success).
- Types: `toast.success({ title: string })` / `toast.error({ title: string })` consistent across tasks.
- Spec coverage table maps every requirement to a task.
