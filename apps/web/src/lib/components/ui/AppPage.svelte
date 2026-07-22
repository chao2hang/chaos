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
		/* Fill remaining content height; 0 basis avoids min-content blowout. */
		flex: 1 1 0;
		min-height: 0;
		overflow: hidden;
	}

	/* Below the 3-column editor, shell height is content-driven — let .content scroll. */
	@media (max-width: 980px) {
		.app-page.editor {
			flex: 0 0 auto;
			min-height: 0;
			overflow: visible;
		}
	}

	@media (max-width: 700px) {
		.app-page.editor {
			gap: var(--space-4);
		}
	}
</style>
