<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';

	let { data }: NodeProps = $props();
	const label = $derived(String((data as { label?: string })?.label ?? 'Start'));
</script>

<div class="fn start">
	<span class="tag">IN</span>
	<strong>{label}</strong>
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.fn {
		min-width: 9rem;
		padding: 0.65rem 0.85rem;
		border-radius: 10px;
		border: 1px solid var(--line-strong);
		background: linear-gradient(135deg, #0f172a, #14532d);
		box-shadow: 0 0 0 1px rgba(34, 197, 94, 0.2), var(--shadow-md);
		color: var(--ink);
		font-size: 0.85rem;
	}
	.tag {
		display: inline-block;
		font-family: var(--font-mono);
		font-size: 0.65rem;
		letter-spacing: 0.08em;
		color: var(--signal);
		margin-bottom: 0.2rem;
	}
	strong {
		display: block;
		font-family: var(--font-mono);
	}
</style>
