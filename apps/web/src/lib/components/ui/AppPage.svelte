<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		children,
		variant = 'default'
	} = $props<{
		children: Snippet;
		variant?: 'default' | 'editor';
	}>();
</script>

<div class="app-page" class:editor={variant === 'editor'}>
	{@render children()}
</div>

<style>
	.app-page {
		width: min(100%, var(--page-max-width));
		margin: 0 auto;
		display: flex;
		min-width: 0;
		flex-direction: column;
		gap: var(--page-gap);
	}

	.app-page.editor {
		width: 100%;
		max-width: var(--editor-max-width);
	}

	@media (max-width: 700px) {
		.app-page.editor {
			gap: var(--space-4);
		}
	}
</style>
