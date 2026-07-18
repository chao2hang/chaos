<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';

	let { data }: NodeProps = $props();
	const d = $derived(
		data as {
			expression?: string;
			outbound?: string;
			enabled?: boolean;
			index?: number;
			onChange?: (patch: { expression?: string; enabled?: boolean }) => void;
		}
	);
</script>

<div class="fn rule" class:off={d.enabled === false}>
	<Handle type="target" position={Position.Left} id="in" />
	<span class="tag">RULE {((d.index ?? 0) + 1).toString().padStart(2, '0')}</span>
	<label>
		match
		<input
			value={d.expression ?? ''}
			onclick={(e) => e.stopPropagation()}
			onpointerdown={(e) => e.stopPropagation()}
			onchange={(e) =>
				d.onChange?.({ expression: (e.currentTarget as HTMLInputElement).value })}
		/>
	</label>
	<label class="chk">
		<input
			type="checkbox"
			checked={d.enabled !== false}
			onclick={(e) => e.stopPropagation()}
			onpointerdown={(e) => e.stopPropagation()}
			onchange={(e) =>
				d.onChange?.({ enabled: (e.currentTarget as HTMLInputElement).checked })}
		/>
		on
	</label>
	<span class="to mono">→ {d.outbound ?? '?'}</span>
	<Handle type="source" position={Position.Right} id="out" />
</div>

<style>
	.fn {
		min-width: 13rem;
		padding: 0.6rem 0.75rem;
		border-radius: 10px;
		border: 1px solid #334155;
		background: linear-gradient(160deg, #0f172a, #1e293b);
		box-shadow: var(--shadow-md);
		color: var(--ink);
	}
	.fn.off {
		opacity: 0.55;
	}
	.tag {
		display: block;
		font-family: var(--font-mono);
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		color: #fbbf24;
		margin-bottom: 0.35rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--ink-dim);
		font-family: var(--font-mono);
	}
	label input {
		width: 100%;
		padding: 0.3rem 0.4rem;
		font-size: 0.75rem;
		font-family: var(--font-mono);
		background: var(--bg-input);
		border: 1px solid var(--line);
		border-radius: 6px;
		color: var(--ink);
	}
	.chk {
		flex-direction: row;
		align-items: center;
		gap: 0.35rem;
		margin-top: 0.35rem;
		text-transform: none;
		font-size: 0.75rem;
		color: var(--ink-muted);
	}
	.to {
		display: block;
		margin-top: 0.35rem;
		font-size: 0.72rem;
		color: var(--signal);
	}
	.mono {
		font-family: var(--font-mono);
	}
</style>
