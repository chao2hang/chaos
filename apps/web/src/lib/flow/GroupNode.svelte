<script lang="ts">
	import { Boxes, Layers3, RadioTower, Server } from '@lucide/svelte';
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import type { OrchestrationNodeGroupData } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { data, selected = false }: NodeProps = $props();
	const details = $derived(data as unknown as OrchestrationNodeGroupData);
	const nodeCount = $derived(details.sources?.filter((source) => source.kind === 'node').length ?? 0);
	const subscriptionCount = $derived(
		details.sources?.filter((source) => source.kind === 'subscription').length ?? 0
	);
	const groupCount = $derived(details.sources?.filter((source) => source.kind === 'group').length ?? 0);
</script>

<div class="group-node" class:selected>
	<Handle type="target" position={Position.Left} id="in" />
	<header>
		<div class="node-type"><Boxes size={14} strokeWidth={1.8} aria-hidden="true" /><span>NODE GROUP</span></div>
		<small>{t('flow.ruleCount', { count: details.route_count ?? 0 })}</small>
	</header>
	<strong>{details.name || t('flow.node.unnamedGroup')}</strong>
	<div class="source-counts">
		<span title={t('flow.source.nodes')}><Server size={12} strokeWidth={1.8} />{nodeCount}</span>
		<span title={t('flow.source.subscriptions')}><RadioTower size={12} strokeWidth={1.8} />{subscriptionCount}</span>
		<span title={t('flow.source.groups')}><Layers3 size={12} strokeWidth={1.8} />{groupCount}</span>
	</div>
	<footer>{details.policy || 'min_moving_avg'}</footer>
</div>

<style>
	.group-node {
		width: 13.5rem;
		height: 7.25rem;
		padding: 0.7rem 0.8rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink);
		box-shadow: 0 0 0 0 transparent;
		transition:
			border-color 120ms ease,
			box-shadow 120ms ease;
	}

	.group-node:hover,
	.group-node.selected {
		border-color: var(--ink);
	}

	.group-node.selected {
		box-shadow: 0 0 0 3px rgba(17, 17, 17, 0.14);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding-bottom: 0.45rem;
		border-bottom: 1px solid var(--line);
	}

	.node-type,
	.source-counts,
	.source-counts span {
		display: flex;
		align-items: center;
	}

	.node-type {
		gap: 0.35rem;
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.55rem;
		font-weight: 700;
	}

	small {
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.52rem;
		font-weight: 700;
	}

	strong {
		display: block;
		overflow: hidden;
		margin-top: 0.55rem;
		font-size: 0.82rem;
		font-weight: 680;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.source-counts {
		gap: 0.35rem;
		margin-top: 0.5rem;
	}

	.source-counts span {
		gap: 0.22rem;
		min-width: 2.2rem;
		padding: 0.18rem 0.35rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.58rem;
	}

	footer {
		overflow: hidden;
		margin-top: 0.45rem;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.56rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.group-node :global(.svelte-flow__handle) {
		width: 0.62rem;
		height: 0.62rem;
		border: 2px solid var(--surface);
		background: var(--ink);
	}
</style>
