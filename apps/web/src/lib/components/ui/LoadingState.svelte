<script lang="ts">
	import { LoaderCircle } from '@lucide/svelte';
	let { label = 'Loading' } = $props<{ label?: string }>();
</script>

<div class="loading" role="status">
	<LoaderCircle size={18} strokeWidth={1.7} aria-hidden="true" />
	<span>{label}</span>
</div>

<style>
	.loading {
		display: flex;
		min-height: 9rem;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		color: var(--ink-muted);
		font-size: 0.78rem;
	}

	.loading :global(svg) {
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
