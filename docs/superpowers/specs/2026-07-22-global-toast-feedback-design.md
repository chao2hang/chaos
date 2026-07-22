# Global Toast feedback

**Date:** 2026-07-22  
**Status:** Approved for implementation (design review)  
**Depends on:** Existing `toast.svelte.ts`, `ToastViewport.svelte` (mounted in root layout), `Notice.svelte`  
**Related:** Design system `docs/design-system/MASTER.md` (async feedback contract)

## Goal

Replace page-inline operation feedback (`Notice` banners such as “配置已保存并应用，共 N 个节点”) with the **global Toast** stack so every route reports success, failure, validation, and durable status the same way: fixed top-right, dismissible, non-blocking.

## Non-goals

- Redesigning Toast visuals, placement, or adding a third-party toast library.
- Replacing `ConfirmDialog` with Toast.
- Adding new i18n keys (reuse existing message strings).
- Removing `Notice.svelte` from the codebase in this slice (keep for rare inline use; mark as secondary in the design system).
- Changing validation focus/action behavior on the orchestration canvas beyond duration defaults (existing `flow-validation` toast stays).

## Current baseline

| Piece | Behavior |
|---|---|
| `apps/web/src/lib/toast.svelte.ts` | Global queue; `show` / tone helpers; max 4; same `id` replaces |
| `ToastViewport.svelte` | Root layout fixed viewport; auto-dismiss timers; optional action |
| Pages | Local `error` / `message` + top-of-page `<Notice>` on dashboard, dns, groups, nodes, subscriptions, settings, routing, orchestrate, login, setup, home |
| Orchestrate | Already uses `toast.warning` for validation; success/error still inline `Notice` |

## Approach (chosen)

**Direct use of the existing `toast` API** — no notify wrapper, no dual-state `$effect` bridge.

1. Tune default durations on tone helpers.
2. Call `toast.*` at each former `message =` / `error =` site.
3. Remove page-level success/error banner state and `<Notice>` markup used only for those feedbacks.
4. Drive durable conditions with **stable ids** + `duration: 0`, dismiss when the condition clears.

Rejected alternatives:

- **Notify facade:** Extra indirection without new behavior.
- **Keep `error`/`message` and sync via `$effect`:** Double state, easy to double-fire.

## Feedback classification and duration

| Kind | Tone | Default duration | Examples |
|---|---|---|---|
| Success | `success` | **4000** ms | Draft saved, applied, deleted, imported |
| Info | `info` | **5000** ms | Transient informational copy |
| Warning | `warning` | **0** (manual dismiss) | Validation tray, needs republish |
| Error | `error` | **0** (manual dismiss) | Save/load/delete/login failure, form validation failures that previously used page `Notice` |

Call sites may override `duration` (e.g. existing orchestration validation toast at 8000).

### `toast.svelte.ts` changes

- Keep `DEFAULT_DURATION = 5000` for plain `show` / `info` when unspecified.
- `toast.success(...)` defaults `duration` to `4000` when omitted.
- `toast.warning(...)` and `toast.error(...)` default `duration` to `0` when omitted.
- Existing fields (`title`, optional `description`, optional `action`, `id`) unchanged.
- `MAX_VISIBLE_TOASTS = 4` unchanged; same-id replace still preferred for durable toasts.

## Stable ids

| Id | Owner / condition |
|---|---|
| `needs-republish` | Shared across dashboard, subscriptions, orchestrate (and any other page that surfaces the same need). Show while true; `toast.dismiss('needs-republish')` when false after apply/reload. |
| `auth-expired` | Login page session-expired banner. |
| `home-api` | Home page API unreachable. |
| `flow-validation` | Already used on orchestrate; keep. |
| `routing-readonly` | Routing page read-only notice, if still shown after migration. |

Pattern:

```ts
if (needsRepublish) {
	toast.warning({
		id: 'needs-republish',
		title: t('flow.needsRepublish'), // or page-specific equivalent string already used
		duration: 0
	});
} else {
	toast.dismiss('needs-republish');
}
```

One semantic → one id site-wide so navigations do not stack duplicate republish toasts.

## Title / description / action

- Prefer a single short string in `title` (maps 1:1 from former `Notice` `message`).
- Put multi-line API detail in `description` when useful.
- `action` only when the user must jump (existing “view issue”); ordinary success/error toasts have no action.

## Page migration list

For each page: drop `Notice` imports and banner markup; remove `message` / `error` state when used only for banners; replace assignments with `toast.success` / `toast.error` / `toast.info` / `toast.warning`.

| Route | Migrate |
|---|---|
| `routes/(app)/orchestrate/+page.svelte` | Success/error to toast; `needsRepublish` stable id; keep validation toast |
| `routes/(app)/dashboard/+page.svelte` | Latency/apply/stop/geo messages + needs republish |
| `routes/(app)/dns/+page.svelte` | Save/load/validation |
| `routes/(app)/groups/+page.svelte` | CRUD feedback |
| `routes/(app)/nodes/+page.svelte` | Import/latency/delete |
| `routes/(app)/subscriptions/+page.svelte` | Import/refresh/delete + needs republish |
| `routes/(app)/settings/+page.svelte` | User create/delete |
| `routes/(app)/routing/+page.svelte` | Error + read-only info |
| `routes/login/+page.svelte` | Login failure, expired session |
| `routes/setup/+page.svelte` | Setup/API/password failures |
| `routes/+page.svelte` | API unreachable |

### Form fields

- `Field` `error=` / invalid styling may remain for highlight if already present.
- Copy that previously lived only in a page-level `Notice` for the same failure becomes a toast; do not leave a duplicate full-width Notice.
- Per product choice for this slice: validation failures that were page Notices also use toast (error, duration 0).

## Design system

Update `docs/design-system/MASTER.md`:

- **Async operations:** Success and failure feedback use **Toast** (global). Error and warning toasts default to manual dismiss; success auto-dismisses.
- **Component contracts:** Document `Toast` / `ToastViewport` (global notifications: info, success, warning, error, dismissible, optional action). Demote `Notice` to optional inline feedback, not the default for operation results.
- Feature components still must not invent independent toast systems; they call `toast` or receive callbacks from pages.

## Accessibility and behavior

- Keep existing roles: `alert` for error, `status` otherwise.
- Manual dismiss control with `aria-label` / `title` unchanged.
- Queue cap 4; durable same-id updates do not grow the queue.
- Route changes: transient toasts may remain until timeout (viewport is root-level). Durable ids are maintained by the page that owns the condition.
- Neutral styling only; no new accent colors.

## Testing / verification

- Orchestrate save draft and publish-and-apply → top-right success toast; no full-width banner under the header.
- Force API failure → error toast stays until dismiss.
- After config change, `needs_republish` → warning with id `needs-republish`; after successful apply → dismissed.
- Login failure and setup password mismatch → toast; no form-top Notice strip.
- Run `pnpm --dir apps/web check` (and build if local convention requires it).
- Spot-check dashboard, dns, groups, nodes, subscriptions, settings, routing.

## Implementation notes

- Match surrounding code: existing `toast.warning({ id: 'flow-validation', ... })` style on orchestrate.
- Prefer minimal diffs per page; no drive-by refactors.
- Shared `needs-republish` copy may keep page-specific i18n keys (`dashboard.needsRepublish`, `flow.needsRepublish`, etc.) but **same toast id**.
- Do not clear the entire toast queue on navigation.

## Out of scope follow-ups (optional later)

- Delete unused `Notice` export if zero call sites remain.
- Shared helper for “sync durable toast from boolean” if repetition becomes noisy after migration.
