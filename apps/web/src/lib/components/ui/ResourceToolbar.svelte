<script lang="ts">
	import { RefreshCw } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import Button from './Button.svelte';
	import SearchInput from './SearchInput.svelte';

	let {
		value = $bindable(''),
		placeholder,
		label,
		meta,
		refreshLabel,
		refreshing = false,
		disabled = false,
		onrefresh,
		actions
	} = $props<{
		value?: string;
		placeholder: string;
		label?: string;
		meta?: string;
		refreshLabel: string;
		refreshing?: boolean;
		disabled?: boolean;
		onrefresh: () => void | Promise<void>;
		actions?: Snippet;
	}>();
</script>

<div class="resource-toolbar">
	<SearchInput bind:value {placeholder} label={label ?? placeholder} />
	<div class="toolbar-context">
		{#if meta}<span class="toolbar-meta">{meta}</span>{/if}
		{#if actions}{@render actions()}{/if}
		<Button
			variant="ghost"
			size="icon"
			icon={RefreshCw}
			loading={refreshing}
			{disabled}
			aria-label={refreshLabel}
			title={refreshLabel}
			onclick={onrefresh}
		/>
	</div>
</div>

<style>
	.resource-toolbar,
	.toolbar-context {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.toolbar-context {
		justify-content: flex-end;
		min-width: 0;
	}

	.toolbar-meta {
		min-width: 0;
		color: var(--ink-muted);
		font-size: 0.72rem;
		text-align: right;
	}

	@media (max-width: 720px) {
		.resource-toolbar {
			align-items: stretch;
			flex-direction: column;
		}

		.toolbar-context {
			justify-content: space-between;
		}

		.toolbar-context :global(.ui-command:first-child:last-child) {
			margin-left: auto;
		}
	}
</style>
