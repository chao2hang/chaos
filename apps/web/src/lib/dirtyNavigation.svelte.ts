/**
 * Shared dirty-draft navigation / logout confirmation (no window.confirm).
 *
 * Pages mark dirty via document.documentElement.dataset.chaosUnsaved = 'true'.
 * beforeNavigate must call cancel() synchronously, then open the dialog;
 * on confirm we set the allow flag and goto the destination.
 */

import { goto } from '$app/navigation';

let open = $state(false);
let pendingHref: string | null = null;
let pendingResolve: ((ok: boolean) => void) | null = null;

export function isDiscardDialogOpen() {
	return open;
}

/** Allow next navigation without prompting (e.g. after explicit discard). */
export function allowDirtyNavigationOnce() {
	if (typeof sessionStorage === 'undefined') return;
	sessionStorage.setItem('chaos_allow_dirty_navigation', '1');
}

export function consumeAllowDirtyNavigation(): boolean {
	if (typeof sessionStorage === 'undefined') return false;
	if (sessionStorage.getItem('chaos_allow_dirty_navigation') === '1') {
		sessionStorage.removeItem('chaos_allow_dirty_navigation');
		return true;
	}
	return false;
}

export function isDocumentUnsaved(): boolean {
	return (
		typeof document !== 'undefined' && document.documentElement.dataset.chaosUnsaved === 'true'
	);
}

/**
 * Use inside beforeNavigate. Cancels the navigation synchronously and opens ConfirmDialog.
 * On confirm, navigates to `href`.
 */
export function interceptDirtyNavigation(options: {
	dirty: boolean;
	cancel: () => void;
	href?: string | null;
}) {
	const { dirty, cancel, href = null } = options;
	if (!dirty || typeof window === 'undefined') return;
	if (consumeAllowDirtyNavigation()) return;
	cancel();
	pendingHref = href;
	pendingResolve = null;
	open = true;
}

/** Open dialog for non-navigation actions (logout, close form). */
export function requestDiscard(): Promise<boolean> {
	if (open && pendingResolve) {
		// Already open for navigation — treat as reject for this waiter.
		return Promise.resolve(false);
	}
	return new Promise((resolve) => {
		pendingHref = null;
		pendingResolve = resolve;
		open = true;
	});
}

export function confirmDiscard() {
	open = false;
	const href = pendingHref;
	const resolve = pendingResolve;
	pendingHref = null;
	pendingResolve = null;
	if (href) {
		allowDirtyNavigationOnce();
		void goto(href);
		return;
	}
	resolve?.(true);
}

export function cancelDiscard() {
	open = false;
	const resolve = pendingResolve;
	pendingHref = null;
	pendingResolve = null;
	resolve?.(false);
}
