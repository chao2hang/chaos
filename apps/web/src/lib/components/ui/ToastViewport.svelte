<script lang="ts">
	import { onMount } from 'svelte';
	import { Check, CircleAlert, Info, TriangleAlert, X } from '@lucide/svelte';
	import { t } from '$lib/i18n.svelte';
	import { toast, type ToastItem } from '$lib/toast.svelte';

	const icons = {
		info: Info,
		success: Check,
		warning: TriangleAlert,
		error: CircleAlert
	};

	let timers = new Map<string, { item: ToastItem; timer: ReturnType<typeof setTimeout> }>();

	function clearTimer(id: string) {
		const entry = timers.get(id);
		if (entry) clearTimeout(entry.timer);
		timers.delete(id);
	}

	function schedule(item: ToastItem) {
		clearTimer(item.id);
		if (item.duration <= 0) return;
		timers.set(item.id, {
			item,
			timer: setTimeout(() => toast.dismiss(item.id), item.duration)
		});
	}

	function dismiss(id: string) {
		clearTimer(id);
		toast.dismiss(id);
	}

	function runAction(item: ToastItem) {
		item.action?.onclick();
		dismiss(item.id);
	}

	onMount(() => {
		return () => {
			for (const entry of timers.values()) clearTimeout(entry.timer);
		};
	});

	$effect(() => {
		const activeIds = new Set(toast.items.map((item) => item.id));
		for (const id of timers.keys()) {
			if (!activeIds.has(id)) clearTimer(id);
		}
		for (const item of toast.items) {
			if (timers.get(item.id)?.item !== item) schedule(item);
		}
	});
</script>

<section class="toast-viewport" aria-label="Notifications">
	{#each toast.items as item (item.id)}
		{@const Icon = icons[item.tone]}
		<article class="toast toast--{item.tone}" role={item.tone === 'error' ? 'alert' : 'status'}>
			<Icon class="toast__icon" size={18} strokeWidth={1.9} aria-hidden="true" />
			<div class="toast__content">
				<strong>{item.title}</strong>
				{#if item.description}<p>{item.description}</p>{/if}
				{#if item.action}
					<button class="toast__action" type="button" onclick={() => runAction(item)}>{item.action.label}</button>
				{/if}
			</div>
			<button class="toast__dismiss" type="button" aria-label={t('common.dismiss')} title={t('common.dismiss')} onclick={() => dismiss(item.id)}>
				<X size={16} strokeWidth={1.8} aria-hidden="true" />
			</button>
		</article>
	{/each}
</section>

<style>
	.toast-viewport {
		position: fixed;
		top: var(--space-5);
		right: var(--space-5);
		z-index: 100;
		display: flex;
		width: min(24rem, calc(100vw - (var(--space-5) * 2)));
		flex-direction: column;
		gap: var(--space-2);
		pointer-events: none;
	}

	.toast {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		gap: var(--space-3);
		align-items: start;
		padding: var(--space-3);
		border: 1px solid var(--line-strong);
		border-left: 3px solid var(--ink);
		border-radius: var(--radius-md);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		pointer-events: auto;
	}

	.toast--error, .toast--warning { background: var(--danger-surface); }
	.toast__icon { margin-top: 0.1rem; }
	.toast__content { min-width: 0; }
	.toast__content strong { display: block; font-size: 0.82rem; line-height: 1.35; }
	.toast__content p { margin: 0.2rem 0 0; color: var(--ink-muted); font-size: 0.75rem; line-height: 1.45; }
	.toast__action { margin-top: var(--space-2); padding: 0; border: 0; background: transparent; color: var(--ink); font-size: 0.75rem; font-weight: 700; text-decoration: underline; text-underline-offset: 0.2em; }
	.toast__dismiss { display: grid; width: 1.5rem; height: 1.5rem; margin: -0.2rem -0.2rem 0 0; padding: 0; border: 0; border-radius: var(--radius-sm); place-items: center; background: transparent; color: var(--ink-muted); }
	.toast__dismiss:hover { background: var(--surface-hover); color: var(--ink); }

	@media (max-width: 900px) {
		.toast-viewport { top: calc(var(--topbar-height) + var(--space-4)); right: var(--space-4); width: min(24rem, calc(100vw - (var(--space-4) * 2))); }
	}
</style>
