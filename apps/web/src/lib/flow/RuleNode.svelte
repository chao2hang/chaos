<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import type { OrchestrationRuleData } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { data, selected = false }: NodeProps = $props();
	const details = $derived(data as unknown as OrchestrationRuleData);
	const matcherLabel = $derived(
		details.matcher.kind === 'domain_suffix'
			? t('flow.matcher.domainSuffix')
			: t('flow.matcher.destinationCidr')
	);
</script>

<div class="rule-node" class:selected>
	<Handle type="source" position={Position.Right} id="out" />
	<header>
		<span>{matcherLabel}</span>
		<small>#{details.priority ?? '?'}</small>
	</header>
	<strong title={details.matcher.pattern}>{details.matcher.pattern || t('flow.rule.emptyPattern')}</strong>
	<span class="target">{t('flow.rule.to')} {details.target_name || t('flow.rule.connectTarget')}</span>
</div>

<style>
	.rule-node {
		width: 14.5rem;
		min-height: 5.65rem;
		padding: 0.65rem 0.75rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink);
		box-shadow: 0 0 0 0 transparent;
		transition:
			border-color 120ms ease,
			box-shadow 120ms ease;
	}

	.rule-node:hover,
	.rule-node.selected {
		border-color: var(--ink);
	}

	.rule-node.selected {
		box-shadow: 0 0 0 3px rgba(17, 17, 17, 0.14);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding-bottom: 0.4rem;
		border-bottom: 1px solid var(--line);
	}

	header span,
	header small,
	.target {
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.56rem;
		font-weight: 700;
	}

	strong,
	.target {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	strong {
		margin-top: 0.5rem;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		font-weight: 680;
	}

	.target {
		margin-top: 0.38rem;
		font-weight: 500;
	}

	.rule-node :global(.svelte-flow__handle) {
		width: 0.6rem;
		height: 0.6rem;
		border: 2px solid var(--surface);
		background: var(--ink);
	}
</style>
