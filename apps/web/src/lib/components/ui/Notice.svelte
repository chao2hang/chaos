<script module lang="ts">
	type ActiveNotice = {
		element: HTMLElement;
		setOffset: (offset: number) => void;
	};

	let activeNotices: ActiveNotice[] = [];

	function layoutNotices() {
		let offset = 0;
		for (const notice of activeNotices) {
			notice.setOffset(offset);
			offset += notice.element.getBoundingClientRect().height + 8;
		}
	}
</script>

<script lang="ts">
	import { onMount } from 'svelte';
	import { Check, CircleAlert, Info, X } from '@lucide/svelte';
	import { t } from '$lib/i18n.svelte';

	let {
		message,
		tone = 'info',
		ondismiss
	} = $props<{
		message: string;
		tone?: 'info' | 'success' | 'error';
		ondismiss?: () => void;
	}>();

	const Icon = $derived(tone === 'success' ? Check : tone === 'error' ? CircleAlert : Info);
	let noticeElement: HTMLDivElement;
	let stackOffset = $state(0);

	onMount(() => {
		const notice = {
			element: noticeElement,
			setOffset: (offset: number) => (stackOffset = offset)
		};
		const observer = new ResizeObserver(layoutNotices);

		activeNotices = [...activeNotices, notice];
		observer.observe(noticeElement);
		requestAnimationFrame(layoutNotices);

		return () => {
			observer.disconnect();
			activeNotices = activeNotices.filter((entry) => entry !== notice);
			layoutNotices();
		};
	});
</script>

<div
	bind:this={noticeElement}
	class="notice {tone}"
	role={tone === 'error' ? 'alert' : 'status'}
	style:--notice-offset={`${stackOffset}px`}
>
	<Icon size={16} strokeWidth={1.9} aria-hidden="true" />
	<p>{message}</p>
	{#if ondismiss}
		<button type="button" aria-label={t('common.dismiss')} title={t('common.dismiss')} onclick={ondismiss}>
			<X size={15} strokeWidth={1.8} aria-hidden="true" />
		</button>
	{/if}
</div>

<style>
	.notice {
		position: fixed;
		top: var(--space-5);
		right: var(--space-5);
		z-index: 40;
		transform: translateY(var(--notice-offset));
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: start;
		gap: var(--space-2);
		width: min(28rem, calc(100vw - (var(--space-5) * 2)));
		padding: 0.7rem 0.8rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
		box-shadow: var(--shadow-float);
		color: var(--ink);
		font-size: 0.82rem;
	}

	.success {
		border-color: var(--ink);
		background: var(--surface);
	}

	.error {
		border-color: var(--ink);
		background: var(--danger-surface);
	}

	p {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
	}

	button {
		display: inline-grid;
		place-items: center;
		width: 1.5rem;
		height: 1.5rem;
		margin: -0.2rem -0.25rem -0.2rem 0;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
	}

	button:hover {
		background: var(--surface-hover);
		color: var(--ink);
	}

	@media (max-width: 900px) {
		.notice {
			top: calc(var(--topbar-height) + var(--space-4));
			right: var(--space-4);
			width: min(28rem, calc(100vw - (var(--space-4) * 2)));
		}
	}
</style>
