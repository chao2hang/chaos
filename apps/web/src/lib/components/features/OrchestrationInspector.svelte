<script lang="ts">
	import { AlertTriangle, Link2, LockKeyhole, Trash2, X } from '@lucide/svelte';
	import type {
		GroupDto,
		NodeDto,
		OrchestrationEdgeDto,
		OrchestrationNodeDto,
		OrchestrationNodeGroupData,
		OrchestrationRuleData,
		OrchestrationRuleMatcher,
		OrchestrationSource,
		OrchestrationValidationIssue,
		SubscriptionDto
	} from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';
	import SegmentedControl from '$lib/components/ui/SegmentedControl.svelte';
	import NodePickList from '$lib/components/features/NodePickList.svelte';

	type SourceTab = OrchestrationSource['kind'];
	type ResourceOption = { kind: SourceTab; id: string; name: string; meta: string };

	let {
		node,
		edge,
		flowNodes,
		flowEdges,
		inventoryNodes,
		subscriptions,
		groups,
		issues,
		busy = false,
		onupdaterule,
		onupdategroup,
		onsettarget,
		onsetendtarget,
		ondelete,
		onselectnode
	}: {
		node: OrchestrationNodeDto | null;
		edge: OrchestrationEdgeDto | null;
		flowNodes: OrchestrationNodeDto[];
		flowEdges: OrchestrationEdgeDto[];
		inventoryNodes: NodeDto[];
		subscriptions: SubscriptionDto[];
		groups: GroupDto[];
		issues: OrchestrationValidationIssue[];
		busy?: boolean;
		onupdaterule: (id: string, patch: Partial<OrchestrationRuleData>) => void;
		onupdategroup: (id: string, patch: Partial<OrchestrationNodeGroupData>) => void;
		onsettarget: (ruleId: string, targetId: string | null) => void;
		onsetendtarget: (targetId: string | null) => void;
		ondelete: (kind: 'node' | 'edge', id: string) => void;
		onselectnode: (id: string) => void;
	} = $props();

	let sourceTab = $state<SourceTab>('node');
	let query = $state('');

	const selectedSources = $derived(node?.type === 'node_group' ? node.data.sources : []);
	const selectedIssues = $derived(
		issues.filter(
			(issue) =>
				(node && issue.node_id === node.id) ||
				(edge && issue.edge_id === edge.id) ||
				(!node && !edge && !issue.node_id && !issue.edge_id)
		)
	);
	const edgeSource = $derived(edge ? flowNodes.find((item) => item.id === edge.source) ?? null : null);
	const edgeTarget = $derived(edge ? flowNodes.find((item) => item.id === edge.target) ?? null : null);
	const selectedTargetId = $derived(
		node?.type === 'rule' || node?.type === 'end'
			? flowEdges.find((item) => item.source === node.id)?.target ?? ''
			: ''
	);
	const targetOptions = $derived(
		flowNodes.filter((item) => item.type === 'node_group' || item.type === 'builtin')
	);

	const fallbackLabel = $derived.by(() => {
		const endEdge = flowEdges.find((item) => item.source === 'end');
		if (!endEdge) return 'DIRECT';
		const target = flowNodes.find((item) => item.id === endEdge.target);
		if (!target) return endEdge.target;
		if (target.type === 'builtin') return 'DIRECT';
		if (target.type === 'node_group') return target.data.name || t('flow.node.unnamedGroup');
		return endEdge.target;
	});
	const incomingRules = $derived.by(() => {
		if (!node || (node.type !== 'node_group' && node.type !== 'builtin')) return [];
		const sourceIds = new Set(
			flowEdges.filter((item) => item.target === node.id).map((item) => item.source)
		);
		return flowNodes.filter(
			(item): item is Extract<OrchestrationNodeDto, { type: 'rule' }> =>
				item.type === 'rule' && sourceIds.has(item.id)
		);
	});
	const groupOptions = $derived.by((): ResourceOption[] => {
		const currentGroupId = node?.type === 'node_group' ? node.id : null;
		const ownedRuntimeIds = new Set(
			flowNodes.flatMap((item) =>
				item.type === 'node_group' && item.data.runtime_group_id ? [item.data.runtime_group_id] : []
			)
		);
		const draftGroups: ResourceOption[] = flowNodes
			.filter(
				(item): item is Extract<OrchestrationNodeDto, { type: 'node_group' }> =>
					item.type === 'node_group' && item.id !== currentGroupId
			)
			.map((item) => ({
				kind: 'group',
				id: item.id,
				name: item.data.name || t('flow.node.unnamedGroup'),
				meta: t('flow.source.draftGroupMeta', { count: item.data.sources.length })
			}));
		const storedGroups: ResourceOption[] = groups
			.filter((item) => !ownedRuntimeIds.has(item.id))
			.map((item) => ({
				kind: 'group',
				id: item.id,
				name: item.name,
				meta: `${item.policy} / ${t('groups.memberCount', { count: item.members.length })}`
			}));
		return [...draftGroups, ...storedGroups];
	});
	const sourceTabs = $derived([
		{ value: 'node', label: t('flow.source.nodes'), count: inventoryNodes.length },
		{ value: 'subscription', label: t('flow.source.subscriptions'), count: subscriptions.length },
		{ value: 'group', label: t('flow.source.groups'), count: groupOptions.length }
	]);
	const resourceOptions = $derived.by((): ResourceOption[] => {
		const normalized = query.trim().toLowerCase();
		let options: ResourceOption[] = [];
		if (sourceTab === 'node') {
			options = inventoryNodes.map((item) => ({
				kind: 'node',
				id: item.id,
				name: item.name,
				meta: [item.protocol, item.address].filter(Boolean).join(' / ') || t('common.unknown')
			}));
		}
		if (sourceTab === 'subscription') {
			options = subscriptions.map((item) => ({
				kind: 'subscription',
				id: item.id,
				name: item.tag || t('subscriptions.untagged'),
				meta: t('subscriptions.nodeCountShort', { count: item.node_count })
			}));
		}
		if (sourceTab === 'group') {
			options = groupOptions;
		}
		if (!normalized) return options;
		return options.filter((item) => `${item.name} ${item.meta}`.toLowerCase().includes(normalized));
	});

	function nodeName(item: OrchestrationNodeDto): string {
		if (item.type === 'rule') return item.data.matcher.pattern || t('flow.rule.untitled');
		if (item.type === 'node_group') return item.data.name || t('flow.node.unnamedGroup');
		return 'DIRECT';
	}

	function patchMatcher(patch: Partial<OrchestrationRuleMatcher>) {
		if (node?.type !== 'rule') return;
		onupdaterule(node.id, { matcher: { ...node.data.matcher, ...patch } });
	}

	function patchGroup(patch: Partial<OrchestrationNodeGroupData>) {
		if (node?.type === 'node_group') onupdategroup(node.id, patch);
	}

	function isSelected(option: ResourceOption): boolean {
		return selectedSources.some((source) => source.kind === option.kind && source.id === option.id);
	}

	function toggleSource(option: ResourceOption, checked: boolean) {
		if (node?.type !== 'node_group') return;
		const sources = checked
			? isSelected(option)
				? node.data.sources
				: [...node.data.sources, { kind: option.kind, id: option.id, weight: 1 }]
			: node.data.sources.filter(
					(source) => !(source.kind === option.kind && source.id === option.id)
				);
		patchGroup({ sources });
	}

	function updateSourceWeight(source: OrchestrationSource, weight: number) {
		if (node?.type !== 'node_group') return;
		const normalized = Math.max(1, Math.min(99, Math.floor(weight) || 1));
		patchGroup({
			sources: node.data.sources.map((item) =>
				item.kind === source.kind && item.id === source.id ? { ...item, weight: normalized } : item
			)
		});
	}

	function sourceName(source: OrchestrationSource): string {
		if (source.kind === 'node') return inventoryNodes.find((item) => item.id === source.id)?.name ?? source.id;
		if (source.kind === 'subscription') {
			return subscriptions.find((item) => item.id === source.id)?.tag || t('subscriptions.untagged');
		}
		const draftGroup = flowNodes.find(
			(item) =>
				item.type === 'node_group' &&
				(item.id === source.id || item.data.runtime_group_id === source.id)
		);
		if (draftGroup?.type === 'node_group') return draftGroup.data.name || t('flow.node.unnamedGroup');
		return groups.find((item) => item.id === source.id)?.name ?? source.id;
	}
</script>

<aside class="inspector" aria-label={t('flow.inspector.title')}>
	<header class="panel-header">
		<div>
			<span>{t('flow.inspector.title')}</span>
			<strong>{node ? nodeName(node) : edge ? t('flow.inspector.connection') : t('flow.inspector.overview')}</strong>
		</div>
		{#if ((node && node.type !== 'builtin' && node.type !== 'start' && node.type !== 'end') || edge) && !busy}
			<Button
				variant="ghost"
				size="icon"
				icon={Trash2}
				aria-label={t('common.delete')}
				title={t('common.delete')}
				onclick={() => ondelete(node ? 'node' : 'edge', (node ?? edge)!.id)}
			/>
		{/if}
	</header>

	{#if selectedIssues.length}
		<div class="issue-list">
			{#each selectedIssues as issue (`${issue.scope}:${issue.code}:${issue.node_id ?? ''}:${issue.edge_id ?? ''}`)}
				<div class:runtime={issue.scope === 'runtime'}>
					<AlertTriangle size={14} strokeWidth={1.8} aria-hidden="true" />
					<span>{t(`flow.validation.${issue.code}`)}</span>
				</div>
			{/each}
		</div>
	{/if}

	{#if node?.type === 'rule'}
		<section class="inspector-section">
			<Field label={t('flow.rule.matchType')} forId="flow-rule-kind">
				<select
					id="flow-rule-kind"
					value={node.data.matcher.kind}
					disabled={busy}
					onchange={(event) =>
						patchMatcher({ kind: (event.currentTarget as HTMLSelectElement).value as OrchestrationRuleMatcher['kind'] })}
				>
					<option value="domain_suffix">{t('flow.matcher.domainSuffix')}</option>
					<option value="destination_cidr">{t('flow.matcher.destinationCidr')}</option>
				</select>
			</Field>
			<Field label={t('flow.rule.pattern')} forId="flow-rule-pattern">
				<input
					id="flow-rule-pattern"
					type="text"
					value={node.data.matcher.pattern}
					disabled={busy}
					placeholder={node.data.matcher.kind === 'domain_suffix' ? 'example.com' : '192.0.2.0/24'}
					oninput={(event) => patchMatcher({ pattern: (event.currentTarget as HTMLInputElement).value })}
				/>
			</Field>
			<Field label={t('flow.rule.priority')} forId="flow-rule-priority">
				<input
					id="flow-rule-priority"
					type="number"
					min="1"
					max="9999"
					value={node.data.priority ?? 1}
					disabled={busy}
					onchange={(event) =>
						onupdaterule(node.id, {
							priority: Math.min(
								9999,
								Math.max(1, Math.floor(Number((event.currentTarget as HTMLInputElement).value) || 1))
							)
						})}
				/>
			</Field>
			<Field label={t('flow.rule.target')} forId="flow-rule-target">
				<select
					id="flow-rule-target"
					value={selectedTargetId}
					disabled={busy}
					onchange={(event) =>
						onsettarget(node.id, (event.currentTarget as HTMLSelectElement).value || null)}
				>
					<option value="">{t('flow.rule.noTarget')}</option>
					{#each targetOptions as target (target.id)}
						<option value={target.id}>{nodeName(target)}</option>
					{/each}
				</select>
			</Field>
		</section>
	{:else if node?.type === 'node_group'}
		<section class="inspector-section">
			<Field label={t('groups.name')} forId="flow-group-name">
				<input id="flow-group-name" type="text" value={node.data.name} disabled={busy} oninput={(event) => patchGroup({ name: (event.currentTarget as HTMLInputElement).value })} />
			</Field>
			<Field label={t('groups.policy')} forId="flow-group-policy">
				<select id="flow-group-policy" value={node.data.policy} disabled={busy} onchange={(event) => patchGroup({ policy: (event.currentTarget as HTMLSelectElement).value })}>
					{#each ['min_moving_avg', 'min', 'random', 'fixed'] as policy}
						<option value={policy}>{t(`flow.policy.${policy}`)}</option>
					{/each}
				</select>
			</Field>
		</section>

		<section class="inspector-section">
			<div class="section-title"><strong>{t('flow.inspector.selectedSources')}</strong><span>{selectedSources.length}</span></div>
			{#if selectedSources.length}
				<div class="selected-sources">
					{#each selectedSources as source (`${source.kind}:${source.id}`)}
						<div class="selected-source">
							<span class="source-kind">{source.kind.slice(0, 3).toUpperCase()}</span>
							<strong title={sourceName(source)}>{sourceName(source)}</strong>
							<input type="number" min="1" max="99" value={source.weight} disabled={busy} aria-label={t('flow.weight')} onchange={(event) => updateSourceWeight(source, Number((event.currentTarget as HTMLInputElement).value))} />
							<button type="button" disabled={busy} aria-label={t('common.delete')} onclick={() => toggleSource({ kind: source.kind, id: source.id, name: '', meta: '' }, false)}>
								<X size={13} strokeWidth={1.8} aria-hidden="true" />
							</button>
						</div>
					{/each}
				</div>
			{:else}
				<div class="compact-empty">{t('flow.inspector.noSources')}</div>
			{/if}
		</section>

<section class="inspector-section resource-section">
				<SegmentedControl bind:value={sourceTab} options={sourceTabs} label={t('flow.inspector.sourceType')} />
				{#if sourceTab === 'node'}
					<NodePickList
						nodes={inventoryNodes}
						{busy}
						isChecked={(node) =>
							selectedSources.some((source) => source.kind === 'node' && source.id === node.id)}
						onToggle={(node, checked) =>
							toggleSource({ kind: 'node', id: node.id, name: node.name, meta: '' }, checked)}
						showWeight={false}
					/>
				{:else}
					<SearchInput bind:value={query} placeholder={t('flow.inspector.searchResources')} />
					<div class="resource-list">
						{#each resourceOptions as option (`${option.kind}:${option.id}`)}
							<label class:selected={isSelected(option)}>
								<input
									type="checkbox"
									checked={isSelected(option)}
									disabled={busy}
									onchange={(event) =>
										toggleSource(option, (event.currentTarget as HTMLInputElement).checked)}
								/>
								<span><strong>{option.name}</strong><small>{option.meta}</small></span>
							</label>
						{/each}
						{#if !resourceOptions.length}<div class="compact-empty">{t('common.noSearchResults')}</div>{/if}
					</div>
				{/if}
			</section>

		<section class="inspector-section">
			<div class="section-title"><strong>{t('flow.inspector.connectedRules')}</strong><span>{incomingRules.length}</span></div>
			<div class="route-list">
				{#each incomingRules as rule (rule.id)}
					<button type="button" onclick={() => onselectnode(rule.id)}>
						<span>#{rule.data.priority ?? '?'}</span><strong>{nodeName(rule)}</strong>
					</button>
				{/each}
				{#if !incomingRules.length}<div class="compact-empty">{t('flow.inspector.noConnectedRules')}</div>{/if}
			</div>
		</section>
	{:else if node?.type === 'builtin'}
		<section class="inspector-section fixed-summary">
			<LockKeyhole size={18} strokeWidth={1.8} aria-hidden="true" />
			<div><span>{t('flow.direct.fixedLabel')}</span><strong>DIRECT</strong><p>{t('flow.direct.description')}</p></div>
		</section>
		<section class="inspector-section">
			<div class="section-title"><strong>{t('flow.inspector.connectedRules')}</strong><span>{incomingRules.length}</span></div>
			<div class="route-list">
				{#each incomingRules as rule (rule.id)}
					<button type="button" onclick={() => onselectnode(rule.id)}><span>#{rule.data.priority ?? '?'}</span><strong>{nodeName(rule)}</strong></button>
				{/each}
				{#if !incomingRules.length}<div class="compact-empty">{t('flow.inspector.noConnectedRules')}</div>{/if}
			</div>
		</section>
	{:else if node?.type === 'end'}
		<section class="inspector-section">
			<Field label={t('flow.fallbackTarget')} forId="flow-end-target">
				<select
					id="flow-end-target"
					value={selectedTargetId}
					disabled={busy}
					onchange={(event) =>
						onsetendtarget((event.currentTarget as HTMLSelectElement).value || null)}
				>
					<option value="">{t('flow.rule.noTarget')}</option>
					{#each targetOptions as target (target.id)}
						<option value={target.id}>{nodeName(target)}</option>
					{/each}
				</select>
			</Field>
			<p class="fixed-summary-text">{t('flow.end.description')}</p>
		</section>
	{:else if node?.type === 'start'}
		<section class="inspector-section">
			<dl class="overview">
				<div>
					<dt>{t('flow.rules')}</dt>
					<dd>{flowEdges.filter((edge) => edge.source === 'start').length}</dd>
				</div>
			</dl>
			<p class="fixed-summary-text">{t('flow.start.description')}</p>
		</section>
	{:else if edge}
		<section class="inspector-section connection-summary">
			<div><span>{t('flow.edge.from')}</span><strong>{edgeSource ? nodeName(edgeSource) : edge.source}</strong></div>
			<Link2 size={15} strokeWidth={1.8} aria-hidden="true" />
			<div><span>{t('flow.edge.to')}</span><strong>{edgeTarget ? nodeName(edgeTarget) : edge.target}</strong></div>
		</section>
	{:else}
		<section class="inspector-section overview">
			<dl>
				<div><dt>{t('flow.rules')}</dt><dd>{flowNodes.filter((item) => item.type === 'rule').length}</dd></div>
				<div><dt>{t('flow.inspector.nodeGroups')}</dt><dd>{flowNodes.filter((item) => item.type === 'node_group').length}</dd></div>
				<div><dt>{t('flow.inspector.connections')}</dt><dd>{flowEdges.length}</dd></div>
				<div><dt>{t('flow.fallback')}</dt><dd>{fallbackLabel}</dd></div>
			</dl>
		</section>
	{/if}
</aside>

<style>
	.inspector { display: flex; min-width: 0; height: 100%; min-height: 0; flex-direction: column; border-left: 1px solid var(--line); background: var(--surface); overflow-y: auto; }
	.panel-header { position: sticky; top: 0; z-index: 2; display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); min-height: 4rem; padding: .7rem var(--space-4); border-bottom: 1px solid var(--ink); background: var(--surface); }
	.panel-header span, .panel-header strong { display: block; }
	.panel-header span { color: var(--ink-faint); font-family: var(--font-mono); font-size: .58rem; font-weight: 700; text-transform: uppercase; }
	.panel-header strong { overflow: hidden; max-width: 13rem; margin-top: .12rem; font-size: .8rem; text-overflow: ellipsis; white-space: nowrap; }
	.inspector-section, .issue-list { padding: var(--space-4); border-bottom: 1px solid var(--line); }
	.inspector-section { display: flex; flex-direction: column; gap: var(--space-4); }
	.issue-list { display: flex; flex-direction: column; gap: .45rem; background: var(--danger-surface); }
	.issue-list > div { display: flex; align-items: flex-start; gap: var(--space-2); font-size: .7rem; }
	.issue-list > div.runtime { text-decoration: underline dotted; }
	.section-title { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.section-title strong { font-size: .74rem; }
	.section-title span { color: var(--ink-muted); font-family: var(--font-mono); font-size: .65rem; }
	.selected-sources, .resource-list, .route-list { display: flex; flex-direction: column; border: 1px solid var(--line); border-radius: var(--radius-md); overflow: hidden; }
	.selected-source { display: grid; grid-template-columns: 2rem minmax(0, 1fr) 2.8rem 1.6rem; align-items: center; gap: .35rem; min-height: 2.45rem; padding: .35rem .45rem; border-bottom: 1px solid var(--line); }
	.selected-source:last-child { border-bottom: 0; }
	.source-kind { color: var(--ink-faint); font-family: var(--font-mono); font-size: .52rem; font-weight: 700; }
	.selected-source strong { overflow: hidden; font-size: .68rem; text-overflow: ellipsis; white-space: nowrap; }
	.selected-source input { min-height: 1.55rem; padding: .15rem .25rem; font-family: var(--font-mono); font-size: .62rem; }
	.selected-source button { display: grid; place-items: center; width: 1.5rem; height: 1.5rem; padding: 0; border: 0; background: transparent; color: var(--ink-muted); }
	.resource-section :global(.segments), .resource-section :global(.search-field) { width: 100%; }
	.resource-section :global(.segments) { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); }
	.resource-section :global(.segments button) {
		min-width: 0;
		padding-inline: 0.2rem;
		font-size: 0.65rem;
		line-height: 1.15;
		white-space: normal;
		text-align: center;
	}
	.resource-list { max-height: 18rem; overflow-y: auto; }
	.resource-list label { display: grid; grid-template-columns: 1rem minmax(0, 1fr); align-items: center; gap: var(--space-2); min-height: 3rem; padding: .4rem .55rem; border-bottom: 1px solid var(--line); cursor: pointer; }
	.resource-list label:last-child { border-bottom: 0; }
	.resource-list label:hover, .resource-list label.selected { background: var(--surface-subtle); }
	.resource-list strong, .resource-list small { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.resource-list strong { font-size: .7rem; }
	.resource-list small { margin-top: .1rem; color: var(--ink-faint); font-family: var(--font-mono); font-size: .56rem; }
	.compact-empty { padding: var(--space-5) var(--space-3); color: var(--ink-faint); font-size: .7rem; text-align: center; }
	.route-list button { display: grid; grid-template-columns: 2.3rem minmax(0, 1fr); align-items: center; gap: .35rem; min-height: 2.7rem; padding: .45rem .6rem; border: 0; border-bottom: 1px solid var(--line); background: var(--surface); color: var(--ink); text-align: left; }
	.route-list button:last-child { border-bottom: 0; }
	.route-list button:hover { background: var(--surface-subtle); }
	.route-list span { color: var(--ink-faint); font-family: var(--font-mono); font-size: .58rem; }
	.route-list strong { overflow: hidden; font-size: .68rem; text-overflow: ellipsis; white-space: nowrap; }
	.fixed-summary { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: start; }
	.fixed-summary span { color: var(--ink-faint); font-family: var(--font-mono); font-size: .56rem; font-weight: 700; }
	.fixed-summary strong { display: block; margin-top: .12rem; font-family: var(--font-mono); font-size: .78rem; }
	.fixed-summary p { margin: .35rem 0 0; color: var(--ink-muted); font-size: .68rem; line-height: 1.5; }
	.connection-summary { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; }
	.connection-summary > div:last-child { text-align: right; }
	.connection-summary span { display: block; color: var(--ink-faint); font-family: var(--font-mono); font-size: .56rem; font-weight: 700; text-transform: uppercase; }
	.connection-summary strong { display: block; overflow: hidden; margin-top: .12rem; font-size: .72rem; text-overflow: ellipsis; white-space: nowrap; }
	.overview dl { margin: 0; }
	.overview dl div { display: flex; align-items: center; justify-content: space-between; min-height: 2.4rem; border-bottom: 1px solid var(--line); }
	.overview dl div:last-child { border-bottom: 0; }
	.overview dt { color: var(--ink-muted); font-size: .72rem; }
	.overview dd { margin: 0; font-family: var(--font-mono); font-size: .72rem; font-weight: 700; }
	.fixed-summary-text { margin: 0; color: var(--ink-muted); font-size: 0.68rem; line-height: 1.5; }
</style>
