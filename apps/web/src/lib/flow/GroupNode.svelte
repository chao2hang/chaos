<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';

	let { data }: NodeProps = $props();
	const d = $derived(
		data as {
			name?: string;
			policy?: string;
			members?: { node_id: string; name?: string | null; weight: number }[];
			onWeight?: (nodeId: string, weight: number) => void;
			onRemove?: (nodeId: string) => void;
		}
	);
</script>

<div class="fn group">
	<Handle type="target" position={Position.Left} id="in" />
	<div class="head">
		<span class="tag">GROUP</span>
		<strong>{d.name ?? 'group'}</strong>
		<span class="policy">{d.policy ?? 'fixed'}</span>
	</div>
	{#if d.members?.length}
		<ul>
			{#each d.members as m (m.node_id)}
				<li>
					<span class="mn">{m.name ?? m.node_id.slice(0, 6)}</span>
					<input
						type="number"
						min="1"
						max="99"
						value={m.weight}
						onclick={(e) => e.stopPropagation()}
						onpointerdown={(e) => e.stopPropagation()}
						onchange={(e) =>
							d.onWeight?.(
								m.node_id,
								Number((e.currentTarget as HTMLInputElement).value)
							)}
					/>
					<button
						type="button"
						class="x"
						onclick={(e) => {
							e.stopPropagation();
							d.onRemove?.(m.node_id);
						}}
						onpointerdown={(e) => e.stopPropagation()}>×</button
					>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="hint">Connect proxies → here</p>
	{/if}
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.fn {
		min-width: 12.5rem;
		padding: 0.65rem 0.75rem;
		border-radius: 12px;
		border: 1px solid rgba(34, 197, 94, 0.35);
		background: linear-gradient(160deg, #0f172a 0%, #052e16 120%);
		box-shadow: 0 0 0 1px rgba(34, 197, 94, 0.12), var(--shadow-md);
		color: var(--ink);
	}
	.head {
		display: grid;
		gap: 0.1rem;
		margin-bottom: 0.45rem;
	}
	.tag {
		font-family: var(--font-mono);
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		color: var(--signal);
	}
	strong {
		font-family: var(--font-mono);
		font-size: 0.95rem;
	}
	.policy {
		font-family: var(--font-mono);
		font-size: 0.68rem;
		color: var(--ink-dim);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		max-height: 9rem;
		overflow: auto;
	}
	li {
		display: grid;
		grid-template-columns: 1fr 3rem auto;
		gap: 0.25rem;
		align-items: center;
		padding: 0.25rem 0.35rem;
		background: rgba(2, 6, 23, 0.55);
		border: 1px solid var(--line);
		border-radius: 6px;
	}
	.mn {
		font-size: 0.75rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	input {
		width: 100%;
		padding: 0.15rem 0.25rem;
		font-size: 0.72rem;
		font-family: var(--font-mono);
		background: var(--bg-input);
		border: 1px solid var(--line);
		border-radius: 4px;
		color: var(--ink);
	}
	.x {
		padding: 0 0.35rem;
		font-size: 0.85rem;
		line-height: 1.2;
		background: transparent;
		border: none;
		color: var(--bad);
		cursor: pointer;
	}
	.hint {
		margin: 0;
		font-size: 0.72rem;
		color: var(--ink-dim);
		font-family: var(--font-mono);
	}
</style>
