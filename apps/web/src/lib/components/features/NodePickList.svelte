<script lang="ts">
	import { onMount } from 'svelte';
	import { Gauge } from '@lucide/svelte';
	import type { NodeDto } from '$lib/api';
	import { createLatencySession } from '$lib/latencySession.svelte';
	import { formatLatencyMs, latencyClass, latencyTone } from '$lib/latency';
	import { sortByLatency } from '$lib/latencySessionCore';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';

	let {
		nodes,
		busy = false,
		isChecked,
		onToggle,
		showWeight = false,
		compact = false,
		getWeight,
		onWeight,
		emptyLabel,
		toolbar
	}: {
		nodes: NodeDto[];
		busy?: boolean;
		isChecked: (node: NodeDto) => boolean;
		onToggle: (node: NodeDto, checked: boolean) => void;
		showWeight?: boolean;
		compact?: boolean;
		getWeight?: (node: NodeDto) => number;
		onWeight?: (node: NodeDto, weight: number) => void;
		emptyLabel?: string;
		toolbar?: import('svelte').Snippet;
	} = $props();

	const session = createLatencySession();
	let query = $state('');

	const filteredNodes = $derived.by(() => {
		const normalized = query.trim().toLowerCase();
		// Depend on the full latency map so reordering reacts after tests finish.
		const latencyMap = session.latencyById;
		const matchingNodes = normalized
			? nodes.filter((node) =>
					[node.name, node.tag, node.protocol, node.address]
						.filter(Boolean)
						.some((value) => String(value).toLowerCase().includes(normalized))
				)
			: nodes;
		return sortByLatency(matchingNodes, (node) => latencyMap[node.id]);
	});

	onMount(() => {
		void session.load();
		return () => session.dispose();
	});

	function clampWeight(value: number) {
		return Math.max(1, Math.min(99, Math.floor(value) || 1));
	}
</script>

<div class="node-pick-list" class:compact>
	{#if session.error}
		<Notice tone="error" message={session.error} ondismiss={() => session.clearNotices()} />
	{/if}
	{#if session.message}
		<Notice tone="success" message={session.message} ondismiss={() => session.clearNotices()} />
	{/if}

	<div class="node-pick-toolbar">
		<SearchInput bind:value={query} placeholder={t('flow.searchNodes')} />
		<div class="node-pick-actions">
			{#if toolbar}{@render toolbar()}{/if}
			<Button
				size="sm"
				icon={Gauge}
				loading={session.testing === 'visible'}
				disabled={busy || !!session.testing || !filteredNodes.length}
				onclick={() =>
					void session.test(
						filteredNodes.map((node) => node.id),
						'visible'
					)}
			>
				{filteredNodes.length
					? t('nodes.testVisibleCount', { count: filteredNodes.length })
					: t('nodes.testVisible')}
			</Button>
		</div>
	</div>

	<div class="node-pick-rows">
		{#each filteredNodes as node (node.id)}
			{@const checked = isChecked(node)}
			{@const latency = session.latencyById[node.id]}
			{@const tone = latency ? latencyTone(latency.latency_ms, latency.alive) : 'unknown'}
			<div class="node-pick-row" class:active={checked}>
				<label class="node-pick-check">
					<input
						type="checkbox"
						checked={checked}
						disabled={busy}
						onchange={(event) =>
							onToggle(node, (event.currentTarget as HTMLInputElement).checked)}
					/>
					<span>
						<strong>{node.name}</strong>
						<small>
							{[node.protocol, node.address].filter(Boolean).join(' / ') || t('common.unknown')}
						</small>
					</span>
				</label>
				<span class="node-pick-latency {latencyClass(tone)}" title={latency?.message ?? ''}>
					{latency ? formatLatencyMs(latency.latency_ms, latency.alive) : t('common.emDash')}
				</span>
				{#if showWeight && checked && getWeight && onWeight}
					<label class="node-pick-weight">
						<span>{t('flow.weight')}</span>
						<input
							type="number"
							min="1"
							max="99"
							value={getWeight(node)}
							disabled={busy}
							onchange={(event) =>
								onWeight(
									node,
									clampWeight(Number((event.currentTarget as HTMLInputElement).value))
								)}
						/>
					</label>
				{/if}
				<Button
					size="sm"
					variant="ghost"
					icon={Gauge}
					loading={session.testing === node.id}
					disabled={busy || !!session.testing}
					aria-label={t('nodes.testNode', { name: node.name })}
					title={t('nodes.testNode', { name: node.name })}
					onclick={() => void session.test([node.id], node.id)}
				/>
			</div>
		{/each}
		{#if !filteredNodes.length}
			<div class="node-pick-empty">
				{emptyLabel ?? (nodes.length ? t('common.noSearchResults') : t('flow.emptyPool'))}
			</div>
		{/if}
	</div>
</div>

<style>
	.node-pick-list {
		display: grid;
		gap: var(--space-3);
		min-width: 0;
	}
	.node-pick-toolbar {
		display: grid;
		gap: var(--space-2);
	}
	.node-pick-actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		align-items: center;
	}
	.node-pick-rows {
		display: grid;
		gap: 0.35rem;
		max-height: 22rem;
		overflow: auto;
		padding-right: 0.15rem;
	}
	.node-pick-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto auto auto;
		align-items: center;
		gap: var(--space-2);
		padding: 0.45rem 0.55rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm, 6px);
		background: var(--surface);
	}
	.node-pick-row.active {
		border-color: var(--ink);
	}
	.node-pick-check {
		display: flex;
		align-items: flex-start;
		gap: 0.55rem;
		min-width: 0;
	}
	.node-pick-check span {
		display: grid;
		gap: 0.1rem;
		min-width: 0;
	}
	.node-pick-check strong,
	.node-pick-check small {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.node-pick-check small {
		color: var(--muted, #888);
		font-size: 0.8em;
	}
	.node-pick-latency {
		font-variant-numeric: tabular-nums;
		font-size: 0.85em;
		white-space: nowrap;
	}
	.node-pick-weight {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8em;
	}
	.node-pick-weight input {
		width: 3.5rem;
	}
	.node-pick-empty {
		padding: var(--space-3);
		color: var(--muted, #888);
		text-align: center;
		font-size: 0.9em;
	}

	.node-pick-list.compact {
		gap: var(--space-2);
	}

	.compact .node-pick-toolbar {
		gap: 0.4rem;
	}

	.compact .node-pick-actions {
		gap: 0.4rem;
	}

	.compact .node-pick-rows {
		max-height: 18rem;
		gap: 0.25rem;
	}

	.compact .node-pick-row {
		gap: 0.4rem;
		padding: 0.4rem 0.45rem;
	}

	.compact .node-pick-check {
		gap: 0.4rem;
	}

	.compact .node-pick-check strong {
		font-size: 0.76rem;
	}

	.compact .node-pick-check small,
	.compact .node-pick-latency {
		font-size: 0.68rem;
	}
</style>
