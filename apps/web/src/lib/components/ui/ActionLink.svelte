<script lang="ts">
	import type { Component, Snippet } from 'svelte';

	let {
		href,
		children,
		icon: Icon,
		variant = 'secondary',
		size = 'md',
		class: className = ''
	} = $props<{
		href: string;
		children: Snippet;
		icon?: Component;
		variant?: 'primary' | 'secondary' | 'ghost';
		size?: 'sm' | 'md';
		class?: string;
	}>();
</script>

<a class="action-link {variant} {size} {className}" {href}>
	{#if Icon}<Icon size={size === 'sm' ? 14 : 16} strokeWidth={1.8} aria-hidden="true" />{/if}
	<span>{@render children()}</span>
</a>

<style>
	.action-link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.45rem;
		min-height: 2.25rem;
		padding: 0.48rem 0.75rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink);
		font-size: 0.8rem;
		font-weight: 650;
		line-height: 1;
		text-decoration: none;
		white-space: nowrap;
		transition:
			background 130ms ease,
			border-color 130ms ease,
			color 130ms ease;
	}

	.action-link:hover {
		border-color: var(--ink);
		background: var(--surface-subtle);
		text-decoration: none;
	}

	.primary {
		border-color: var(--surface-inverse);
		background: var(--surface-inverse);
		color: var(--ink-inverse);
	}

	.primary:hover {
		background: #303030;
		color: var(--ink-inverse);
	}

	.ghost {
		border-color: transparent;
		background: transparent;
		color: var(--ink-muted);
	}

	.ghost:hover {
		border-color: transparent;
		background: var(--surface-subtle);
		color: var(--ink);
	}

	.sm {
		min-height: 1.9rem;
		padding: 0.36rem 0.55rem;
		font-size: 0.74rem;
	}
</style>
