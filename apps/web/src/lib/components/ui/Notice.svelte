<script lang="ts">
	import { Check, CircleAlert, Info, X } from '@lucide/svelte';

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
</script>

<div class="notice {tone}" role={tone === 'error' ? 'alert' : 'status'}>
	<Icon size={16} strokeWidth={1.9} aria-hidden="true" />
	<p>{message}</p>
	{#if ondismiss}
		<button type="button" aria-label="Dismiss" title="Dismiss" onclick={ondismiss}>
			<X size={15} strokeWidth={1.8} aria-hidden="true" />
		</button>
	{/if}
</div>

<style>
	.notice {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: start;
		gap: var(--space-2);
		padding: 0.7rem 0.8rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
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
</style>
