<script lang="ts">
	import { Search, X } from '@lucide/svelte';
	import { t } from '$lib/i18n.svelte';

	let {
		value = $bindable(''),
		placeholder,
		label,
		disabled = false
	} = $props<{ value?: string; placeholder?: string; label?: string; disabled?: boolean }>();
</script>

<label class="search-field">
	<span class="sr-only">{label ?? t('common.search')}</span>
	<Search size={15} strokeWidth={1.8} aria-hidden="true" />
	<input type="search" bind:value placeholder={placeholder ?? t('common.search')} {disabled} />
	{#if value}
		<button type="button" aria-label={t('common.clearSearch')} title={t('common.clearSearch')} onclick={() => (value = '')}>
			<X size={14} strokeWidth={1.8} aria-hidden="true" />
		</button>
	{/if}
</label>

<style>
	.search-field {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-2);
		width: min(20rem, 100%);
		min-height: 2.25rem;
		padding: 0 0.65rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink-muted);
	}

	.search-field:focus-within {
		border-color: var(--ink);
		box-shadow: 0 0 0 1px var(--ink);
		color: var(--ink);
	}

	input {
		width: 100%;
		min-height: 2.1rem;
		padding: 0;
		border: 0;
		background: transparent;
		box-shadow: none;
		font-size: 0.8rem;
	}

	input:hover,
	input:focus {
		border: 0;
		box-shadow: none;
	}

	button {
		display: inline-grid;
		place-items: center;
		width: 1.45rem;
		height: 1.45rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
	}

	button:hover {
		background: var(--surface-subtle);
		color: var(--ink);
	}
</style>
