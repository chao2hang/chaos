<script lang="ts">
	type Option = { value: string; label: string; count?: number };

	let {
		value = $bindable(),
		options,
		label = 'View'
	} = $props<{ value: string; options: Option[]; label?: string }>();
</script>

<div class="segments" role="tablist" aria-label={label}>
	{#each options as option (option.value)}
		<button
			type="button"
			role="tab"
			aria-selected={value === option.value}
			class:active={value === option.value}
			onclick={() => (value = option.value)}
		>
			{option.label}
			{#if option.count !== undefined}<span>{option.count}</span>{/if}
		</button>
	{/each}
</div>

<style>
	.segments {
		display: inline-flex;
		min-width: 0;
		padding: 0.2rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
	}

	button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
		min-height: 1.9rem;
		padding: 0.35rem 0.65rem;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
		font-size: 0.75rem;
		font-weight: 620;
		white-space: nowrap;
	}

	button:hover {
		color: var(--ink);
	}

	button.active {
		background: var(--surface);
		color: var(--ink);
		box-shadow: 0 0 0 1px var(--line);
	}

	span {
		font-family: var(--font-mono);
		font-size: 0.65rem;
		color: var(--ink-faint);
	}

	@media (max-width: 560px) {
		.segments {
			display: grid;
			grid-template-columns: repeat(3, minmax(0, 1fr));
			width: 100%;
		}

		button {
			padding-inline: 0.35rem;
		}
	}
</style>
