<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import type { OrchestrationBuiltinData } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { data, selected = false }: NodeProps = $props();
	const details = $derived(data as unknown as OrchestrationBuiltinData);
</script>

<div class="flow-node builtin-node" class:selected>
	<Handle type="target" position={Position.Left} id="in" />
	<span>BUILT-IN</span>
	<strong>{details.builtin.toUpperCase()}</strong>
	<small>{t('flow.ruleCount', { count: details.route_count ?? 0 })}</small>
</div>

<style>
	.flow-node {
		min-width: 9rem;
		padding: 0.6rem 0.7rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
		color: var(--ink);
		box-shadow: 0 0 0 0 transparent;
	}

	.flow-node:hover,
	.flow-node.selected {
		border-color: var(--ink);
	}

.flow-node.selected {
			box-shadow: 0 0 0 3px var(--selection-ring);
		}

	span,
	small {
		display: block;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.56rem;
		font-weight: 650;
	}

	strong {
		display: block;
		margin-top: 0.12rem;
		font-family: var(--font-mono);
		font-size: 0.72rem;
	}

	small {
		margin-top: 0.25rem;
		font-weight: 500;
	}

	.flow-node :global(.svelte-flow__handle) {
		width: 0.55rem;
		height: 0.55rem;
		border: 2px solid var(--surface);
		background: var(--ink);
	}
</style>
