<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children, responsive = true } = $props<{ children: Snippet; responsive?: boolean }>();
</script>

<div class="table-frame" class:responsive>{@render children()}</div>

<style>
	.table-frame {
		width: 100%;
		overflow-x: auto;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
	}

	.table-frame :global(.actions-col) {
		text-align: right;
	}

	@media (max-width: 720px) {
		.table-frame.responsive {
			overflow: visible;
			border: 0;
			background: transparent;
		}

		.responsive :global(table),
		.responsive :global(tbody),
		.responsive :global(tr),
		.responsive :global(td) {
			display: block;
			width: 100%;
		}

		.responsive :global(thead) {
			display: none;
		}

		.responsive :global(tbody) {
			display: flex;
			flex-direction: column;
			gap: var(--space-3);
		}

		.responsive :global(tr) {
			position: relative;
			padding: var(--space-3);
			border: 1px solid var(--line);
			border-radius: var(--radius-lg);
			background: var(--surface);
		}

		.responsive :global(td) {
			display: grid;
			grid-template-columns: minmax(5.5rem, 0.42fr) minmax(0, 1fr);
			gap: var(--space-3);
			padding: 0.38rem 0;
			border: 0;
			text-align: right;
		}

		.responsive :global(td)::before {
			content: attr(data-label);
			color: var(--ink-faint);
			font-size: 0.7rem;
			font-weight: 600;
			text-align: left;
		}

		.responsive :global(td .row-actions),
		.responsive :global(td .status) {
			justify-self: end;
		}

		.responsive :global(td.select-col) {
			position: absolute;
			right: var(--space-3);
			top: var(--space-3);
			display: block;
			width: auto;
			padding: 0;
		}

		.responsive :global(td.select-col)::before {
			display: none;
		}

		.responsive :global(td.select-col + td) {
			padding-right: 2rem;
		}
	}
</style>
