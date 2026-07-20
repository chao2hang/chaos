<script lang="ts">
	import { LoaderCircle } from '@lucide/svelte';
	import type { Component, Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
	type Size = 'sm' | 'md' | 'lg' | 'icon';

	let {
		children,
		icon: Icon,
		variant = 'secondary',
		size = 'md',
		loading = false,
		full = false,
		class: className = '',
		disabled = false,
		...rest
	}: HTMLButtonAttributes & {
		children?: Snippet;
		icon?: Component;
		variant?: Variant;
		size?: Size;
		loading?: boolean;
		full?: boolean;
	} = $props();
</script>

<button
	class="button {variant} {size} {className}"
	class:full
	class:icon-only={size === 'icon'}
	disabled={disabled || loading}
	aria-busy={loading}
	{...rest}
>
	{#if loading}
		<LoaderCircle class="spinner" size={16} strokeWidth={1.8} aria-hidden="true" />
	{:else if Icon}
		<Icon size={size === 'sm' ? 14 : 16} strokeWidth={1.8} aria-hidden="true" />
	{/if}
	{#if children}<span class="label">{@render children()}</span>{/if}
</button>

<style>
	.button {
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
		white-space: nowrap;
		transition:
			background 130ms ease,
			border-color 130ms ease,
			color 130ms ease,
			opacity 130ms ease;
	}

	.button:hover:not(:disabled) {
		border-color: var(--ink);
		background: var(--surface-subtle);
	}

	.primary {
		border-color: var(--surface-inverse);
		background: var(--surface-inverse);
		color: var(--ink-inverse);
	}

	.primary:hover:not(:disabled) {
		background: #303030;
		color: var(--ink-inverse);
	}

	.ghost {
		border-color: transparent;
		background: transparent;
		color: var(--ink-muted);
	}

	.ghost:hover:not(:disabled) {
		border-color: transparent;
		background: var(--surface-subtle);
		color: var(--ink);
	}

	.danger {
		border-color: var(--ink);
		background: var(--danger-surface);
		color: var(--ink);
	}

	.danger:hover:not(:disabled) {
		background: var(--surface-inverse);
		color: var(--ink-inverse);
	}

	.sm {
		min-height: 1.9rem;
		padding: 0.36rem 0.55rem;
		font-size: 0.74rem;
	}

	.lg {
		min-height: 2.65rem;
		padding: 0.68rem 0.95rem;
		font-size: 0.86rem;
	}

	.icon {
		width: 2.25rem;
		min-width: 2.25rem;
		padding: 0;
	}

	.full {
		width: 100%;
	}

	.icon-only :global(.label) {
		display: none;
	}

	.spinner {
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
