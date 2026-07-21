<script lang="ts">
	import type { Component, Snippet } from 'svelte';
	import type { HTMLAnchorAttributes } from 'svelte/elements';

	let {
		href,
		children,
		icon: Icon,
		variant = 'secondary',
		size = 'md',
		full = false,
		class: className = '',
		...rest
	}: HTMLAnchorAttributes & {
		children?: Snippet;
		icon?: Component;
		variant?: 'primary' | 'secondary' | 'ghost';
		size?: 'sm' | 'md' | 'lg' | 'icon';
		full?: boolean;
	} = $props();
</script>

<a
	class="action-link ui-command ui-command--{variant} ui-command--{size} {className}"
	class:ui-command--full={full}
	{href}
	{...rest}
>
	{#if Icon}<Icon size={size === 'sm' ? 14 : 16} strokeWidth={1.8} aria-hidden="true" />{/if}
	{#if children}<span class="ui-command__label">{@render children()}</span>{/if}
</a>
