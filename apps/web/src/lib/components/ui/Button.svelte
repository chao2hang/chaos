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
	class="button ui-command ui-command--{variant} ui-command--{size} {className}"
	class:ui-command--full={full}
	disabled={disabled || loading}
	aria-busy={loading}
	{...rest}
>
	{#if loading}
		<LoaderCircle class="spinner" size={16} strokeWidth={1.8} aria-hidden="true" />
	{:else if Icon}
		<Icon size={size === 'sm' ? 14 : 16} strokeWidth={1.8} aria-hidden="true" />
	{/if}
	{#if children}<span class="ui-command__label">{@render children()}</span>{/if}
</button>

<style>
	.spinner {
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
