<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		description,
		meta,
		actions
	} = $props<{
		title: string;
		description?: string;
		meta?: string;
		actions?: Snippet;
	}>();
</script>

<header class="page-header">
	<div class="copy">
		{#if meta}<p class="meta">{meta}</p>{/if}
		<h1>{title}</h1>
		{#if description}<p class="description">{description}</p>{/if}
	</div>
	{#if actions}<div class="header-actions">{@render actions()}</div>{/if}
</header>

<style>
	.page-header {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: var(--space-6);
		padding-bottom: var(--space-5);
		border-bottom: 1px solid var(--line-strong);
	}

	.copy {
		min-width: 0;
	}

	.meta {
		margin: 0 0 var(--space-2);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.68rem;
		text-transform: uppercase;
	}

	h1 {
		margin: 0;
		font-size: 1.65rem;
		font-weight: 720;
		line-height: 1.15;
		letter-spacing: 0;
	}

	.description {
		max-width: 44rem;
		margin: var(--space-2) 0 0;
		color: var(--ink-muted);
		font-size: 0.9rem;
	}

	.header-actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		flex-wrap: wrap;
		gap: var(--space-2);
		flex: 0 0 auto;
	}

	@media (max-width: 640px) {
		.page-header {
			align-items: stretch;
			flex-direction: column;
			gap: var(--space-4);
		}

		.header-actions {
			justify-content: flex-start;
		}
	}
</style>
