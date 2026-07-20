<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		label,
		forId,
		hint,
		error,
		optional = false,
		children
	} = $props<{
		label: string;
		forId?: string;
		hint?: string;
		error?: string;
		optional?: boolean;
		children: Snippet;
	}>();
</script>

<div class="field-wrap" class:invalid={!!error}>
	<div class="label-row">
		<label for={forId}>{label}</label>
		{#if optional}<span>Optional</span>{/if}
	</div>
	{@render children()}
	{#if error}<p class="field-error">{error}</p>{:else if hint}<p class="hint">{hint}</p>{/if}
</div>

<style>
	.field-wrap {
		display: flex;
		min-width: 0;
		flex-direction: column;
		gap: 0.35rem;
	}

	.label-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
	}

	label {
		color: var(--ink);
		font-size: 0.75rem;
		font-weight: 650;
	}

	.label-row span,
	p {
		margin: 0;
		color: var(--ink-faint);
		font-size: 0.7rem;
	}

	.field-error {
		color: var(--ink);
		text-decoration: underline;
		text-decoration-style: dotted;
	}

	.invalid :global(input),
	.invalid :global(textarea),
	.invalid :global(select) {
		border-color: var(--ink);
		background: var(--danger-surface);
	}
</style>
