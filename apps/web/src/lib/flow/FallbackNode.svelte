<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';

	let { data }: NodeProps = $props();
	const d = $derived(data as { outbound?: string });
</script>

<div class="fn fb">
	<Handle type="target" position={Position.Left} id="in" />
	<span class="tag">FALLBACK</span>
	<strong>{d.outbound ?? 'proxy'}</strong>
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.fn {
		min-width: 9rem;
		padding: 0.65rem 0.85rem;
		border-radius: 10px;
		border: 1px solid #7f1d1d;
		background: linear-gradient(135deg, #0f172a, #450a0a);
		box-shadow: var(--shadow-md);
		color: var(--ink);
	}
	.tag {
		display: block;
		font-family: var(--font-mono);
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		color: #f87171;
		margin-bottom: 0.2rem;
	}
	strong {
		font-family: var(--font-mono);
		font-size: 0.9rem;
	}
</style>
