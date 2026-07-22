<script lang="ts">
import { AlertTriangle, Link2, LockKeyhole, Plus, Trash2, X } from '@lucide/svelte';
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
		import {
			GEOIP_OPTIONS,
			GEOSITE_OPTIONS,
			parseGeoCodes,
			serializeGeoCodes
		} from '$lib/geoOptions';
		import {
			compareRulesByEvaluationOrder,
			parseDomainList,
			serializeDomainList
		} from '$lib/orchestration';
		import Button from '$lib/components/ui/Button.svelte';
		import Field from '$lib/components/ui/Field.svelte';
		import MultiSelect from '$lib/components/ui/MultiSelect.svelte';
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
			onselectnode,
			onmigratedomains
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
			/**
			 * Move selected domains into another domain rule.
			 * Pass `toRuleId: null` to create a new domain_suffix rule first.
			 * Optional `outboundId` is applied when creating a new rule.
			 */
			onmigratedomains?: (
				fromRuleId: string,
				toRuleId: string | null,
				domains: string[],
				outboundId?: string | null
			) => void;
		} = $props();

		let sourceTab = $state<SourceTab>('node');
		let query = $state('');
		let domainDrafts = $state<string[]>(['']);
		let selectedDomains = $state<string[]>([]);
		/** Existing rule id, or `__new__` to create a new domain rule. */
		let migrateTargetId = $state('');
		let migrateOutboundId = $state('');
		let domainEditorRuleId = $state<string | null>(null);

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
			return flowNodes
				.filter(
					(item): item is Extract<OrchestrationNodeDto, { type: 'rule' }> =>
						item.type === 'rule' && sourceIds.has(item.id)
				)
				.sort(compareRulesByEvaluationOrder);
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
			if (item.type === 'rule') {
				const pattern = item.data.matcher.pattern || t('flow.rule.untitled');
				if (
					item.data.matcher.kind === 'domain_suffix' ||
					item.data.matcher.kind === 'domain_full' ||
					item.data.matcher.kind === 'domain_keyword'
				) {
					const parts = parseDomainList(pattern);
					if (parts.length > 1) return `${parts[0]} +${parts.length - 1}`;
					return parts[0] || pattern;
				}
				return pattern;
			}
			if (item.type === 'node_group') return item.data.name || t('flow.node.unnamedGroup');
			return 'DIRECT';
		}

		function patchMatcher(patch: Partial<OrchestrationRuleMatcher>) {
			if (node?.type !== 'rule') return;
			onupdaterule(node.id, { matcher: { ...node.data.matcher, ...patch } });
		}

		const isDomainMatcher = $derived(
			node?.type === 'rule' &&
				(node.data.matcher.kind === 'domain_suffix' ||
					node.data.matcher.kind === 'domain_full' ||
					node.data.matcher.kind === 'domain_keyword')
		);

		const selectedGeoCodes = $derived(
			node?.type === 'rule' && (node.data.matcher.kind === 'geoip' || node.data.matcher.kind === 'geosite')
				? parseGeoCodes(node.data.matcher.pattern)
				: []
		);

		const geoSelectOptions = $derived.by(() => {
			const catalog =
				node?.type === 'rule' && node.data.matcher.kind === 'geosite'
					? GEOSITE_OPTIONS
					: GEOIP_OPTIONS;
			const base = catalog.map((option) => {
				const translated = t(option.labelKey);
				return {
					value: option.code,
					label: translated === option.labelKey ? option.code : translated,
					meta: option.code
				};
			});
			// Preserve unknown custom codes so old drafts remain editable.
			for (const code of selectedGeoCodes) {
				if (!base.some((option) => option.value === code)) {
					base.push({
						value: code,
						label: code,
						meta: t('flow.matcher.customCodes')
					});
				}
			}
			return base;
		});

		function setGeoCodes(next: string[]) {
			if (node?.type !== 'rule') return;
			patchMatcher({ pattern: serializeGeoCodes(next) });
		}

const domainRuleOptions = $derived(
				flowNodes.filter(
					(item): item is Extract<OrchestrationNodeDto, { type: 'rule' }> =>
						item.type === 'rule' &&
						item.id !== node?.id &&
						(item.data.matcher.kind === 'domain_suffix' ||
							item.data.matcher.kind === 'domain_full' ||
							item.data.matcher.kind === 'domain_keyword')
				)
			);

			const migrateOutboundOptions = $derived(
				flowNodes.filter((item) => item.type === 'node_group' || item.type === 'builtin')
			);

			const canMigrate = $derived.by(() => {
				if (busy || !selectedDomains.length || !migrateTargetId) return false;
				if (migrateTargetId === '__new__') return true;
				return domainRuleOptions.some((item) => item.id === migrateTargetId);
			});

			// Keep multi-row domain editor in sync when switching rules / external pattern updates.
			$effect(() => {
				if (node?.type !== 'rule' || !isDomainMatcher) {
					domainEditorRuleId = null;
					return;
				}
				const pattern = node.data.matcher.pattern;
				const ruleId = node.id;
				if (domainEditorRuleId !== ruleId) {
					domainEditorRuleId = ruleId;
					const parts = parseDomainList(pattern);
					domainDrafts = parts.length ? parts : [''];
					selectedDomains = [];
					migrateTargetId = domainRuleOptions.length ? '' : '__new__';
					migrateOutboundId = '';
					return;
				}
				const serializedDraft = serializeDomainList(domainDrafts);
				const serializedPattern = serializeDomainList(parseDomainList(pattern));
				if (serializedDraft !== serializedPattern) {
					const parts = parseDomainList(pattern);
					domainDrafts = parts.length ? parts : [''];
					selectedDomains = selectedDomains.filter((d) => parts.includes(d));
				}
			});

		function commitDomainDrafts(next: string[]) {
			if (node?.type !== 'rule' || !isDomainMatcher) return;
			domainDrafts = next.length ? next : [''];
			patchMatcher({ pattern: serializeDomainList(domainDrafts) });
			const kept = new Set(parseDomainList(domainDrafts.join(',')));
			selectedDomains = selectedDomains.filter((d) => kept.has(d));
		}

		function setDomainRow(index: number, value: string) {
			const next = [...domainDrafts];
			next[index] = value;
			commitDomainDrafts(next);
		}

		function addDomainRow() {
			commitDomainDrafts([...domainDrafts, '']);
		}

		function removeDomainRow(index: number) {
			const removed = parseDomainList(domainDrafts[index] ?? '')[0];
			const next = domainDrafts.filter((_, i) => i !== index);
			commitDomainDrafts(next.length ? next : ['']);
			if (removed) selectedDomains = selectedDomains.filter((d) => d !== removed);
		}

		function toggleDomainSelected(domain: string, checked: boolean) {
			const normalized = parseDomainList(domain)[0];
			if (!normalized) return;
			if (checked) {
				if (!selectedDomains.includes(normalized)) {
					selectedDomains = [...selectedDomains, normalized];
				}
			} else {
				selectedDomains = selectedDomains.filter((d) => d !== normalized);
			}
		}

function migrateSelectedDomains() {
				if (node?.type !== 'rule' || !canMigrate) return;
				const toRuleId = migrateTargetId === '__new__' ? null : migrateTargetId;
				const outboundId =
					migrateTargetId === '__new__' ? migrateOutboundId || null : undefined;
				onmigratedomains?.(node.id, toRuleId, selectedDomains, outboundId);
				const moving = new Set(selectedDomains);
				const remaining = domainDrafts.filter((row) => {
					const domain = parseDomainList(row)[0];
					return !domain || !moving.has(domain);
				});
				// Parent mutates source pattern; keep local drafts aligned.
				domainDrafts = remaining.length ? remaining : [''];
				selectedDomains = [];
				if (migrateTargetId !== '__new__') migrateTargetId = '';
			}

			function selectAllDomains() {
				selectedDomains = parseDomainList(domainDrafts.join(','));
			}

			function clearDomainSelection() {
				selectedDomains = [];
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
					<option value="geosite">{t('flow.matcher.geosite')}</option>
					<option value="geoip">{t('flow.matcher.geoip')}</option>
				</select>
			</Field>
{#if node.data.matcher.kind === 'geosite' || node.data.matcher.kind === 'geoip'}
						<Field
							label={t('flow.rule.pattern')}
							forId="flow-rule-geo-select"
							hint={node.data.matcher.kind === 'geosite'
								? t('flow.matcher.geositeHint')
								: t('flow.matcher.geoipHint')}
						>
							<MultiSelect
								id="flow-rule-geo-select"
								label={t('flow.rule.pattern')}
								values={selectedGeoCodes}
								options={geoSelectOptions}
								placeholder={t('flow.matcher.selectCodes')}
								searchPlaceholder={t('flow.matcher.searchCodes')}
								emptyLabel={t('common.noSearchResults')}
								disabled={busy}
								onChange={setGeoCodes}
							/>
						</Field>
					{:else if isDomainMatcher}
						<div class="domain-list-field">
							<div class="section-title">
								<strong>{t('flow.rule.domains')}</strong>
								<span>{parseDomainList(domainDrafts.join(',')).length}</span>
							</div>
							<p class="domain-hint">{t('flow.rule.domainsHint')}</p>
							<div class="domain-rows" role="list">
								{#each domainDrafts as row, index (index)}
									{@const normalized = parseDomainList(row)[0] ?? ''}
									<div class="domain-row" role="listitem">
										<label class="domain-row__check">
											<input
												type="checkbox"
												disabled={busy || !normalized}
												checked={normalized !== '' && selectedDomains.includes(normalized)}
												aria-label={t('flow.rule.selectDomain')}
												onchange={(event) =>
													toggleDomainSelected(
														row,
														(event.currentTarget as HTMLInputElement).checked
													)}
											/>
										</label>
										<input
											class="domain-row__input"
											id={index === 0 ? 'flow-rule-pattern' : `flow-rule-domain-${index}`}
											type="text"
											value={row}
											disabled={busy}
											placeholder="example.com"
											spellcheck="false"
											autocomplete="off"
											oninput={(event) =>
												setDomainRow(index, (event.currentTarget as HTMLInputElement).value)}
										/>
										<button
											class="domain-row__remove"
											type="button"
											disabled={busy || domainDrafts.length <= 1}
											aria-label={t('flow.rule.removeDomain')}
											title={t('flow.rule.removeDomain')}
											onclick={() => removeDomainRow(index)}
										>
											<X size={14} strokeWidth={1.8} aria-hidden="true" />
										</button>
									</div>
								{/each}
							</div>
<div class="domain-actions">
									<Button
										variant="ghost"
										size="sm"
										icon={Plus}
										disabled={busy}
										onclick={addDomainRow}
									>
										{t('flow.rule.addDomain')}
									</Button>
									{#if parseDomainList(domainDrafts.join(',')).length}
										<Button
											variant="ghost"
											size="sm"
											disabled={busy}
											onclick={selectAllDomains}
										>
											{t('flow.rule.selectAllDomains')}
										</Button>
										{#if selectedDomains.length}
											<Button
												variant="ghost"
												size="sm"
												disabled={busy}
												onclick={clearDomainSelection}
											>
												{t('flow.rule.clearDomainSelection')}
											</Button>
										{/if}
									{/if}
								</div>
								<div class="domain-migrate">
									<div class="section-title">
										<strong>{t('flow.rule.migrateTitle')}</strong>
										<span>{selectedDomains.length}</span>
									</div>
									<p class="domain-hint">{t('flow.rule.migrateHint')}</p>
									<Field label={t('flow.rule.migrateTo')} forId="flow-rule-migrate-target">
										<select
											id="flow-rule-migrate-target"
											value={migrateTargetId}
											disabled={busy || !selectedDomains.length}
											onchange={(event) =>
												(migrateTargetId = (event.currentTarget as HTMLSelectElement).value)}
										>
											<option value="">{t('flow.rule.migratePick')}</option>
											<option value="__new__">{t('flow.rule.migrateNewRule')}</option>
											{#each domainRuleOptions as target (target.id)}
												<option value={target.id}>
													#{target.data.priority ?? '?'} · {nodeName(target)}
												</option>
											{/each}
										</select>
									</Field>
									{#if migrateTargetId === '__new__'}
										<Field
											label={t('flow.rule.migrateNewOutbound')}
											forId="flow-rule-migrate-outbound"
											hint={t('flow.rule.migrateNewOutboundHint')}
										>
											<select
												id="flow-rule-migrate-outbound"
												value={migrateOutboundId}
												disabled={busy || !selectedDomains.length}
												onchange={(event) =>
													(migrateOutboundId = (event.currentTarget as HTMLSelectElement)
														.value)}
											>
												<option value="">{t('flow.rule.noTarget')}</option>
												{#each migrateOutboundOptions as target (target.id)}
													<option value={target.id}>{nodeName(target)}</option>
												{/each}
											</select>
										</Field>
									{/if}
									<Button
										variant="primary"
										size="sm"
										disabled={!canMigrate}
										onclick={migrateSelectedDomains}
									>
										{t('flow.rule.migrateAction', { count: selectedDomains.length || 0 })}
									</Button>
								</div>
							</div>
					{:else}
						<Field label={t('flow.rule.pattern')} forId="flow-rule-pattern">
							<input
								id="flow-rule-pattern"
								type="text"
								value={node.data.matcher.pattern}
								disabled={busy}
								placeholder="192.0.2.0/24"
								oninput={(event) =>
									patchMatcher({ pattern: (event.currentTarget as HTMLInputElement).value })}
							/>
						</Field>
					{/if}
<Field
					label={t('flow.rule.priority')}
					forId="flow-rule-priority"
					hint={t('flow.rule.priorityHint')}
				>
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
	.panel-header { position: sticky; top: 0; z-index: 2; display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); min-height: 4rem; padding: .7rem var(--space-4); border-bottom: 1px solid var(--line-strong); background: var(--surface); }
	.panel-header span, .panel-header strong { display: block; }
	.panel-header span { color: var(--ink-faint); font-family: var(--font-mono); font-size: .58rem; font-weight: 700; text-transform: uppercase; }
	.panel-header strong { overflow: hidden; max-width: 13rem; margin-top: .12rem; font-size: .8rem; text-overflow: ellipsis; white-space: nowrap; }
	.inspector-section, .issue-list { padding: var(--space-4); border-bottom: 1px solid var(--line); }
	.inspector-section { display: flex; flex-direction: column; gap: var(--space-4); }
	.domain-list-field {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.domain-hint {
		margin: 0;
		color: var(--ink-muted);
		font-size: 0.68rem;
		line-height: 1.4;
	}
	.domain-rows {
		display: flex;
		flex-direction: column;
		min-width: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.domain-row {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		min-width: 0;
		min-height: 2.5rem;
		padding: 0.35rem 0.5rem;
		border-bottom: 1px solid var(--line);
	}
	.domain-row:last-child {
		border-bottom: 0;
	}
	.domain-row__check {
		display: inline-grid;
		flex: 0 0 auto;
		place-items: center;
		width: 1.15rem;
		height: 1.15rem;
		margin: 0;
		cursor: pointer;
	}
	.domain-row__check input {
		width: 1rem;
		height: 1rem;
		margin: 0;
	}
	/* Override global input[type=text]{width:100%; min-height:2.5rem} so the row stays single-line. */
	.domain-row__input {
		flex: 1 1 auto;
		width: auto !important;
		min-width: 0 !important;
		min-height: 1.85rem !important;
		height: 1.85rem;
		padding: 0.2rem 0.55rem !important;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-sm);
		background: var(--surface);
		color: var(--ink);
		font-size: 0.78rem;
		line-height: 1.2;
		box-shadow: none;
	}
	.domain-row__input:focus {
		border-color: var(--ink);
		box-shadow: 0 0 0 1px var(--ink);
	}
	.domain-row__remove {
		display: inline-grid;
		flex: 0 0 auto;
		place-items: center;
		width: 1.75rem;
		height: 1.75rem;
		margin: 0;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
	}
	.domain-row__remove:hover:not(:disabled) {
		background: var(--surface-subtle, var(--surface-hover, transparent));
		color: var(--ink);
	}
	.domain-row__remove:disabled {
		opacity: 0.35;
	}
	.domain-actions {
		display: flex;
	}
	.domain-migrate {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-top: var(--space-1);
		border-top: 1px dashed var(--line);
	}
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
