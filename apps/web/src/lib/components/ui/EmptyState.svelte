<script lang="ts">
	import { Inbox } from '@lucide/svelte';
	import type { Component, Snippet } from 'svelte';

	let {
		title,
		description,
		icon: Icon = Inbox,
		actions
	} = $props<{
		title: string;
		description?: string;
		icon?: Component;
		actions?: Snippet;
	}>();
</script>

<div class="empty-state">
	<Icon size={21} strokeWidth={1.5} aria-hidden="true" />
	<div>
		<h3>{title}</h3>
		{#if description}<p>{description}</p>{/if}
	</div>
	{#if actions}<div class="empty-actions">{@render actions()}</div>{/if}
</div>

<style>
	.empty-state {
		display: flex;
		min-height: 10rem;
		align-items: center;
		justify-content: center;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-8);
		color: var(--ink-muted);
		text-align: center;
	}

	h3 {
		margin: 0;
		color: var(--ink);
		font-size: 0.88rem;
		font-weight: 680;
	}

	p {
		max-width: 28rem;
		margin: var(--space-1) 0 0;
		font-size: 0.78rem;
	}

	.empty-actions {
		display: flex;
		justify-content: center;
		gap: var(--space-2);
	}
</style>
