<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';

	let { data }: NodeProps = $props();
	const d = $derived(
		data as { name?: string; protocol?: string | null; address?: string | null }
	);
</script>

<div class="fn proxy">
	<span class="tag">PROXY</span>
	<strong title={d.name}>{d.name ?? 'node'}</strong>
	<span class="meta">{d.protocol ?? '—'} · {d.address ?? '—'}</span>
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.fn {
		min-width: 10rem;
		max-width: 12rem;
		padding: 0.55rem 0.7rem;
		border-radius: 10px;
		border: 1px solid var(--line);
		background: var(--bg-raised);
		box-shadow: var(--shadow-sm);
		color: var(--ink);
	}
	.tag {
		display: block;
		font-family: var(--font-mono);
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		color: var(--info);
		margin-bottom: 0.15rem;
	}
	strong {
		display: block;
		font-size: 0.85rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		display: block;
		margin-top: 0.15rem;
		font-family: var(--font-mono);
		font-size: 0.68rem;
		color: var(--ink-dim);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
