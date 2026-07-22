<script lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import type { OrchestrationRuleData } from '$lib/api';
	import { GEOIP_OPTIONS, GEOSITE_OPTIONS, parseGeoCodes } from '$lib/geoOptions';
	import { t } from '$lib/i18n.svelte';

	let { data, selected = false }: NodeProps = $props();
	const details = $derived(data as unknown as OrchestrationRuleData);
	const matcherLabel = $derived(
		details.matcher.kind === 'domain_suffix'
			? t('flow.matcher.domainSuffix')
			: details.matcher.kind === 'destination_cidr'
				? t('flow.matcher.destinationCidr')
				: details.matcher.kind === 'geosite'
					? t('flow.matcher.geosite')
					: details.matcher.kind === 'geoip'
						? t('flow.matcher.geoip')
						: details.matcher.kind
	);
	const patternLabel = $derived.by(() => {
		const pattern = details.matcher.pattern || t('flow.rule.emptyPattern');
		if (details.matcher.kind === 'geoip' || details.matcher.kind === 'geosite') {
			const options = details.matcher.kind === 'geosite' ? GEOSITE_OPTIONS : GEOIP_OPTIONS;
			const codes = parseGeoCodes(pattern);
			if (!codes.length) return t('flow.rule.emptyPattern');
			return codes
				.map((code) => {
					const option = options.find((item) => item.code === code);
					if (!option) return code;
					const label = t(option.labelKey);
					return label === option.labelKey ? code : label;
				})
				.join(' · ');
		}
		if (
			details.matcher.kind === 'domain_suffix' ||
			details.matcher.kind === 'domain_full' ||
			details.matcher.kind === 'domain_keyword'
		) {
			const parts = pattern
				.split(/[\s,;]+/)
				.map((part) => part.trim().toLowerCase().replace(/^\.+|\.+$/g, ''))
				.filter(Boolean);
			if (!parts.length) return t('flow.rule.emptyPattern');
			if (parts.length === 1) return parts[0];
			return `${parts[0]} · +${parts.length - 1}`;
		}
		return pattern;
	});
</script>

<div class="rule-node" class:selected>
		<Handle type="target" position={Position.Left} id="in" />
		<header>
			<span>{matcherLabel}</span>
			<small>#{details.priority ?? '?'}</small>
		</header>
		<strong title={details.matcher.pattern || ''}>{patternLabel}</strong>
		<span class="target">{t('flow.rule.to')} {details.target_name || t('flow.rule.connectTarget')}</span>
		<Handle type="source" position={Position.Right} id="out" />
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
			box-shadow: 0 0 0 3px var(--selection-ring);
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
