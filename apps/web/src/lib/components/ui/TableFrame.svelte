<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children } = $props<{ children: Snippet }>();
</script>

<div class="table-frame">{@render children()}</div>

<style>
	.table-frame {
		width: 100%;
		overflow-x: auto;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
	}
</style>
