<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';

	let { data }: NodeProps = $props();
	const d = $derived(data as { name?: string });
</script>

<div class="fn bi">
	<Handle type="target" position={Position.Left} id="in" />
	<span class="tag">OUT</span>
	<strong>{d.name ?? 'direct'}</strong>
</div>

<style>
	.fn {
		min-width: 8rem;
		padding: 0.55rem 0.75rem;
		border-radius: 10px;
		border: 1px solid #334155;
		background: #0f172a;
		color: var(--ink);
	}
	.tag {
		display: block;
		font-family: var(--font-mono);
		font-size: 0.6rem;
		color: var(--ink-dim);
		letter-spacing: 0.08em;
	}
	strong {
		font-family: var(--font-mono);
		font-size: 0.85rem;
	}
</style>
