<script lang="ts">
	import { onMount } from 'svelte';
	import { Boxes, ExternalLink, Layers3, RadioTower, Server } from '@lucide/svelte';
	import {
		ApiClientError,
		getOrchestration,
		listNodes,
		listSubscriptions,
		type NodeDto,
		type OrchestrationDocument,
		type OrchestrationNodeDto,
		type OrchestrationSource,
		type SubscriptionDto
	} from '$lib/api';
	import { decorateDocument } from '$lib/orchestration';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import ActionLink from '$lib/components/ui/ActionLink.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ResourceToolbar from '$lib/components/ui/ResourceToolbar.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';
	import { toast } from '$lib/toast.svelte';

	type OrchestrationGroupRow = {
		id: string;
		name: string;
		policy: string;
		routeCount: number;
		runtimeGroupId: string | null;
		sources: OrchestrationSource[];
		nodeCount: number;
		subscriptionCount: number;
		groupSourceCount: number;
		sourceLabels: string[];
	};

	let document = $state<OrchestrationDocument | null>(null);
	let inventoryNodes = $state<NodeDto[]>([]);
	let subscriptions = $state<SubscriptionDto[]>([]);
	let query = $state('');
	let loaded = $state(false);
	let busy = $state(false);

	function sourceLabel(source: OrchestrationSource, nodes: OrchestrationNodeDto[]): string {
		if (source.kind === 'node') {
			return inventoryNodes.find((item) => item.id === source.id)?.name ?? source.id.slice(0, 8);
		}
		if (source.kind === 'subscription') {
			return (
				subscriptions.find((item) => item.id === source.id)?.tag || t('subscriptions.untagged')
			);
		}
		const draft = nodes.find(
			(item) =>
				item.type === 'node_group' &&
				(item.id === source.id || item.data.runtime_group_id === source.id)
		);
		if (draft?.type === 'node_group') {
			return draft.data.name || t('flow.node.unnamedGroup');
		}
		return source.id.slice(0, 8);
	}

	function buildRows(doc: OrchestrationDocument): OrchestrationGroupRow[] {
		const decorated = decorateDocument(doc);
		const nodes = decorated.nodes;
		return nodes
			.filter((node): node is Extract<OrchestrationNodeDto, { type: 'node_group' }> =>
				node.type === 'node_group'
			)
			.map((node) => {
				const sources = node.data.sources ?? [];
				return {
					id: node.id,
					name: node.data.name?.trim() || t('flow.node.unnamedGroup'),
					policy: node.data.policy || 'min_moving_avg',
					routeCount: node.data.route_count ?? 0,
					runtimeGroupId: node.data.runtime_group_id ?? null,
					sources,
					nodeCount: sources.filter((s) => s.kind === 'node').length,
					subscriptionCount: sources.filter((s) => s.kind === 'subscription').length,
					groupSourceCount: sources.filter((s) => s.kind === 'group').length,
					sourceLabels: sources.map((source) => sourceLabel(source, nodes))
				};
			})
			.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
	}

	async function load() {
		busy = true;
		try {
			const [orchestration, nodeResult, subResult] = await Promise.all([
				getOrchestration(),
				listNodes(),
				listSubscriptions()
			]);
			inventoryNodes = nodeResult.nodes;
			subscriptions = subResult.subscriptions;
			document = decorateDocument(orchestration);
			toast.info({
				id: 'groups-readonly',
				title: t('groups.readOnlyNotice'),
				duration: 0
			});
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('groups.loadFailed')
			});
		} finally {
			busy = false;
			loaded = true;
		}
	}


	onMount(() => {
		void load();
	});

	const rows = $derived(document ? buildRows(document) : []);
	const filteredRows = $derived.by(() => {
		const normalized = query.trim().toLowerCase();
		if (!normalized) return rows;
		return rows.filter((row) =>
			[row.name, row.policy, ...row.sourceLabels]
				.filter(Boolean)
				.some((value) => String(value).toLowerCase().includes(normalized))
		);
	});
</script>

<AppPage>
	<PageHeader title={t('groups.title')} description={t('groups.subtitle')} meta="node groups / flow">
		{#snippet actions()}
			<ActionLink variant="primary" icon={ExternalLink} href="/orchestrate">
				{t('groups.openOrchestrate')}
			</ActionLink>
		{/snippet}
	</PageHeader>

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}

		<ResourceToolbar
			bind:value={query}
			placeholder={t('groups.searchPlaceholder')}
			meta={t('groups.count', { count: rows.length })}
			refreshLabel={t('common.refresh')}
			onrefresh={() => void load()}
			disabled={busy}
		/>

		{#if filteredRows.length}
			<TableFrame>
				<table>
					<thead>
						<tr>
							<th>{t('groups.name')}</th>
							<th>{t('groups.policy')}</th>
							<th>{t('groups.sources')}</th>
							<th>{t('groups.routeCount')}</th>
							<th>{t('groups.members')}</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredRows as group (group.id)}
							<tr>
								<td data-label={t('groups.name')}>
									<strong>{group.name}</strong>
									{#if group.runtimeGroupId}
										<small class="mono-id">{group.runtimeGroupId.slice(0, 8)}</small>
									{/if}
								</td>
								<td data-label={t('groups.policy')}><code>{group.policy}</code></td>
								<td data-label={t('groups.sources')}>
									<div class="source-meta">
										<span title={t('flow.source.nodes')}
											><Server size={12} strokeWidth={1.8} aria-hidden="true" />{group.nodeCount}</span
										>
										<span title={t('flow.source.subscriptions')}
											><RadioTower size={12} strokeWidth={1.8} aria-hidden="true" />{group.subscriptionCount}</span
										>
										<span title={t('flow.source.groups')}
											><Layers3 size={12} strokeWidth={1.8} aria-hidden="true" />{group.groupSourceCount}</span
										>
									</div>
								</td>
								<td data-label={t('groups.routeCount')}>
									{t('groups.routeCountValue', { count: group.routeCount })}
								</td>
								<td data-label={t('groups.members')}>
									{#if group.sourceLabels.length}
										<div class="member-chips" title={group.sourceLabels.join(' · ')}>
											{#each group.sourceLabels.slice(0, 4) as label, index (`${group.id}-${index}`)}
												<span>{label}</span>
											{/each}
											{#if group.sourceLabels.length > 4}
												<span class="more">+{group.sourceLabels.length - 4}</span>
											{/if}
										</div>
									{:else}
										<span class="empty-members">{t('groups.noSources')}</span>
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</TableFrame>
		{:else}
			<EmptyState icon={Boxes} title={t('groups.empty')} description={t('groups.emptyDescription')}>
				{#snippet actions()}
					<ActionLink variant="primary" icon={ExternalLink} href="/orchestrate">
						{t('groups.openOrchestrate')}
					</ActionLink>
				{/snippet}
			</EmptyState>
		{/if}
	{/if}
</AppPage>

<style>
	.mono-id {
		display: block;
		margin-top: 0.15rem;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.62rem;
	}

	.source-meta {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 0.55rem;
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.72rem;
	}

	.source-meta span {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}

	.member-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		max-width: 22rem;
	}

	.member-chips span {
		display: inline-block;
		max-width: 8rem;
		overflow: hidden;
		padding: 0.12rem 0.4rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		font-size: 0.68rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.member-chips .more {
		border-style: dashed;
		color: var(--ink-muted);
	}

	.empty-members {
		color: var(--ink-faint);
		font-size: 0.72rem;
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.75rem;
	}
</style>
