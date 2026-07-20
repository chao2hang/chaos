<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		description,
		count,
		actions,
		children,
		flush = false
	} = $props<{
		title?: string;
		description?: string;
		count?: number | string;
		actions?: Snippet;
		children: Snippet;
		flush?: boolean;
	}>();
</script>

<section class="section" class:flush>
	{#if title || description || actions}
		<header>
			<div class="copy">
				{#if title}
					<h2>{title}{#if count !== undefined}<span class="count">{count}</span>{/if}</h2>
				{/if}
				{#if description}<p>{description}</p>{/if}
			</div>
			{#if actions}<div class="section-actions">{@render actions()}</div>{/if}
		</header>
	{/if}
	<div class="body">{@render children()}</div>
</section>

<style>
	.section {
		min-width: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
	}

	header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-4) var(--space-5);
		border-bottom: 1px solid var(--line);
	}

	h2 {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: 0.9rem;
		font-weight: 690;
		line-height: 1.3;
	}

	.count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.35rem;
		height: 1.2rem;
		padding: 0 0.32rem;
		border-radius: 999px;
		background: var(--surface-subtle);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.65rem;
		font-weight: 600;
	}

	p {
		margin: var(--space-1) 0 0;
		color: var(--ink-muted);
		font-size: 0.78rem;
	}

	.section-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.body {
		padding: var(--space-5);
	}

	.flush .body {
		padding: 0;
	}

	@media (max-width: 640px) {
		header {
			padding: var(--space-4);
		}

		.body {
			padding: var(--space-4);
		}
	}
</style>
