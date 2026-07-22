<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { Boxes, ExternalLink, Pencil, Plus, Trash2, X } from '@lucide/svelte';
	import {
		ApiClientError,
		getOrchestration,
		isSessionRedirectPending,
		listNodes,
		listSubscriptions,
		putOrchestration,
		type NodeDto,
		type OrchestrationDocument,
		type OrchestrationEdgeDto,
		type OrchestrationNodeDto,
		type OrchestrationSource,
		type SubscriptionDto
	} from '$lib/api';
	import {
		createGroupNode,
		decorateDocument,
		sanitizeDocument,
		snapshotDocument
	} from '$lib/orchestration';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import { toast } from '$lib/toast.svelte';
	import ActionLink from '$lib/components/ui/ActionLink.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ResourceToolbar from '$lib/components/ui/ResourceToolbar.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';
	import NodePickList from '$lib/components/features/NodePickList.svelte';

	const policies = ['min_moving_avg', 'min', 'random', 'fixed'] as const;
	type GroupNode = Extract<OrchestrationNodeDto, { type: 'node_group' }>;

	let flowNodes = $state.raw<OrchestrationNodeDto[]>([]);
	let flowEdges = $state.raw<OrchestrationEdgeDto[]>([]);
	let viewport = $state({ x: 0, y: 0, zoom: 0.85 });
	let inventoryNodes = $state<NodeDto[]>([]);
	let subscriptions = $state<SubscriptionDto[]>([]);
	let query = $state('');
	let loaded = $state(false);
	let busy = $state(false);
	let formOpen = $state(false);
	let editId = $state<string | null>(null);
	let name = $state('');
	let policy = $state('min_moving_avg');
	let sources = $state<OrchestrationSource[]>([]);
	let formSnapshot = $state('');
	let deleteTarget = $state<GroupNode | null>(null);
	let savedSnapshot = $state('');

	function currentDocument(): OrchestrationDocument {
		return sanitizeDocument(flowNodes, flowEdges, viewport);
	}

	function currentSnapshot(): string {
		return snapshotDocument(currentDocument());
	}

	function formStateSnapshot() {
		return JSON.stringify({ editId, name, policy, sources });
	}

	function groupNodes(): GroupNode[] {
		return flowNodes.filter((node): node is GroupNode => node.type === 'node_group');
	}

	function sourceLabel(source: OrchestrationSource): string {
		if (source.kind === 'node') {
			return inventoryNodes.find((item) => item.id === source.id)?.name ?? source.id.slice(0, 8);
		}
		if (source.kind === 'subscription') {
			return subscriptions.find((item) => item.id === source.id)?.tag || t('subscriptions.untagged');
		}
		const draft = flowNodes.find(
			(item) =>
				item.type === 'node_group' &&
				(item.id === source.id || item.data.runtime_group_id === source.id)
		);
		if (draft?.type === 'node_group') return draft.data.name || t('flow.node.unnamedGroup');
		return source.id.slice(0, 8);
	}

	function policyLabel(value: string): string {
		const key = `flow.policy.${value}`;
		const label = t(key);
		return label === key ? value : label;
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
			const decorated = decorateDocument({
				version: orchestration.version,
				nodes: orchestration.nodes,
				edges: orchestration.edges,
				viewport: orchestration.viewport
			});
			flowNodes = decorated.nodes;
			flowEdges = decorated.edges;
			viewport = decorated.viewport;
			savedSnapshot = currentSnapshot();
			formOpen = false;
			editId = null;
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

	const dirty = $derived(loaded && currentSnapshot() !== savedSnapshot);
	const formDirty = $derived(formOpen && formStateSnapshot() !== formSnapshot);

	beforeNavigate(({ cancel }) => {
		if ((!dirty && !formDirty) || typeof window === 'undefined') return;
		if (isSessionRedirectPending()) return;
		if (sessionStorage.getItem('chaos_allow_dirty_navigation') === '1') {
			sessionStorage.removeItem('chaos_allow_dirty_navigation');
			return;
		}
		if (!window.confirm(t('common.discardDescription'))) cancel();
	});

	$effect(() => {
		if (typeof document === 'undefined') return;
		document.documentElement.dataset.chaosUnsaved = dirty || formDirty ? 'true' : 'false';
		return () => {
			delete document.documentElement.dataset.chaosUnsaved;
		};
	});

	function openCreate() {
		editId = null;
		name = '';
		policy = 'min_moving_avg';
		sources = [];
		formOpen = true;
		formSnapshot = formStateSnapshot();
	}

	function openEdit(group: GroupNode) {
		editId = group.id;
		name = group.data.name ?? '';
		policy = group.data.policy || 'min_moving_avg';
		sources = (group.data.sources ?? []).map((source) => ({ ...source }));
		formOpen = true;
		formSnapshot = formStateSnapshot();
	}

	function closeForm() {
		if (busy) return;
		if (formDirty && typeof window !== 'undefined' && !window.confirm(t('common.discardDescription'))) {
			return;
		}
		formOpen = false;
		editId = null;
	}

	function toggleNodeSource(node: NodeDto, checked: boolean) {
		if (checked) {
			if (sources.some((source) => source.kind === 'node' && source.id === node.id)) return;
			sources = [...sources, { kind: 'node', id: node.id, weight: 1 }];
			return;
		}
		sources = sources.filter((source) => !(source.kind === 'node' && source.id === node.id));
	}

	function setNodeWeight(nodeId: string, value: number) {
		const weight = Math.max(1, Math.min(99, Math.floor(value) || 1));
		sources = sources.map((source) =>
			source.kind === 'node' && source.id === nodeId ? { ...source, weight } : source
		);
	}

	function applyFormToDocument(): boolean {
		const normalizedName = name.trim();
		if (!normalizedName) {
			toast.error({ title: t('groups.nameRequired') });
			return false;
		}
		const duplicate = groupNodes().some(
			(group) =>
				group.id !== editId && group.data.name.trim().toLowerCase() === normalizedName.toLowerCase()
		);
		if (duplicate) {
			toast.error({ title: t('groups.nameDuplicate') });
			return false;
		}

		if (editId) {
			flowNodes = flowNodes.map((node) =>
				node.id === editId && node.type === 'node_group'
					? {
							...node,
							data: {
								...node.data,
								name: normalizedName,
								policy,
								sources: sources.map((source) => ({ ...source }))
							}
						}
					: node
			);
		} else {
			const created = createGroupNode(
				{ x: 520, y: 70 + groupNodes().length * 180 },
				groupNodes().length + 1
			);
			created.data = {
				...created.data,
				name: normalizedName,
				policy,
				sources: sources.map((source) => ({ ...source }))
			};
			flowNodes = [...flowNodes, created];
			editId = created.id;
		}
		return true;
	}

	async function saveDraft() {
		if (formOpen && !applyFormToDocument()) return;
		busy = true;
		try {
			const saved = await putOrchestration(currentDocument());
			const decorated = decorateDocument(saved);
			flowNodes = decorated.nodes;
			flowEdges = decorated.edges;
			viewport = decorated.viewport;
			savedSnapshot = currentSnapshot();
			formOpen = false;
			editId = null;
			toast.success({ title: t('groups.draftSaved') });
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('groups.saveFailed')
			});
		} finally {
			busy = false;
		}
	}

	function confirmDelete() {
		if (!deleteTarget) return;
		const id = deleteTarget.id;
		flowNodes = flowNodes.filter((node) => node.id !== id);
		flowEdges = flowEdges.filter((edge) => edge.source !== id && edge.target !== id);
		if (editId === id) {
			formOpen = false;
			editId = null;
		}
		deleteTarget = null;
		toast.success({ title: t('groups.deletedLocal') });
	}

	const rows = $derived.by(() => {
		const decorated = decorateDocument(currentDocument());
		return decorated.nodes
			.filter((node): node is GroupNode => node.type === 'node_group')
			.map((node) => {
				const nodeSources = node.data.sources ?? [];
				return {
					node,
					name: node.data.name?.trim() || t('flow.node.unnamedGroup'),
					policy: node.data.policy || 'min_moving_avg',
					routeCount: node.data.route_count ?? 0,
					sourceLabels: nodeSources.map((source) => sourceLabel(source)),
					nodeCount: nodeSources.filter((source) => source.kind === 'node').length
				};
			})
			.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
	});

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
	<PageHeader title={t('groups.title')} description={t('groups.subtitle')} meta="node groups / draft">
		{#snippet actions()}
			{#if dirty}<span class="dirty-pill">{t('common.unsavedChanges')}</span>{/if}
			<ActionLink variant="ghost" icon={ExternalLink} href="/orchestrate">
				{t('groups.openOrchestrate')}
			</ActionLink>
			<Button
				variant="ghost"
				icon={formOpen ? X : Plus}
				disabled={busy}
				onclick={formOpen ? closeForm : openCreate}
			>
				{formOpen ? t('common.close') : t('groups.addAction')}
			</Button>
			<Button
				variant="primary"
				loading={busy}
				disabled={!dirty && !formDirty}
				onclick={() => void saveDraft()}
			>
				{t('groups.saveDraft')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if formOpen}
		<Section
			title={editId ? t('groups.editTitle') : t('groups.createTitle')}
			description={t('groups.formDescription')}
		>
			<form
				onsubmit={(event) => {
					event.preventDefault();
					if (!applyFormToDocument()) return;
					formOpen = false;
					formSnapshot = formStateSnapshot();
					toast.success({ title: t('groups.localApplied') });
				}}
			>
				<div class="form-grid">
					<Field label={t('groups.name')} forId="group-name">
						<input id="group-name" type="text" bind:value={name} disabled={busy} required />
					</Field>
					<Field label={t('groups.policy')} forId="group-policy">
						<select id="group-policy" bind:value={policy} disabled={busy}>
							{#each policies as option}
								<option value={option}>{policyLabel(option)}</option>
							{/each}
						</select>
					</Field>
				</div>
				<div class="member-editor">
					<div class="member-editor-header">
						<div>
							<strong>{t('groups.members')}</strong>
							<span>
								{t('groups.memberCount', {
									count: sources.filter((source) => source.kind === 'node').length
								})}
							</span>
						</div>
					</div>
					<NodePickList
						nodes={inventoryNodes}
						{busy}
						isChecked={(node) =>
							sources.some((source) => source.kind === 'node' && source.id === node.id)}
						onToggle={toggleNodeSource}
						showWeight={true}
						getWeight={(node) =>
							sources.find((source) => source.kind === 'node' && source.id === node.id)?.weight ?? 1}
						onWeight={(node, weight) => setNodeWeight(node.id, weight)}
					/>
				</div>
				<div class="form-actions">
					<Button type="button" variant="ghost" disabled={busy} onclick={closeForm}>
						{t('common.cancel')}
					</Button>
					<Button type="submit" variant="secondary" disabled={busy}>{t('groups.applyLocal')}</Button>
					<Button type="button" variant="primary" loading={busy} onclick={() => void saveDraft()}>
						{t('groups.saveDraft')}
					</Button>
				</div>
			</form>
		</Section>
	{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<p class="page-hint">{t('groups.editNotice')}</p>

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
							<th>{t('groups.members')}</th>
							<th>{t('groups.routeCount')}</th>
							<th class="actions-col">{t('common.actions')}</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredRows as row (row.node.id)}
							<tr>
								<td data-label={t('groups.name')}><strong>{row.name}</strong></td>
								<td data-label={t('groups.policy')}><code>{policyLabel(row.policy)}</code></td>
								<td data-label={t('groups.members')}>
									{#if row.sourceLabels.length}
										<div class="member-chips" title={row.sourceLabels.join(' · ')}>
											{#each row.sourceLabels.slice(0, 4) as label, index (`${row.node.id}-${index}`)}
												<span>{label}</span>
											{/each}
											{#if row.sourceLabels.length > 4}
												<span class="more">+{row.sourceLabels.length - 4}</span>
											{/if}
										</div>
									{:else}
										<span class="empty-members">{t('groups.noSources')}</span>
									{/if}
								</td>
								<td data-label={t('groups.routeCount')}>
									{t('groups.routeCountValue', { count: row.routeCount })}
								</td>
								<td class="actions-col" data-label={t('common.actions')}>
									<div class="row-actions">
										<Button
											variant="ghost"
											size="icon"
											icon={Pencil}
											disabled={busy}
											aria-label={t('groups.editNamed', { name: row.name })}
											title={t('groups.editNamed', { name: row.name })}
											onclick={() => openEdit(row.node)}
										/>
										<Button
											variant="ghost"
											size="icon"
											icon={Trash2}
											disabled={busy}
											aria-label={t('groups.deleteNamed', { name: row.name })}
											title={t('groups.deleteNamed', { name: row.name })}
											onclick={() => (deleteTarget = row.node)}
										/>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</TableFrame>
		{:else}
			<EmptyState icon={Boxes} title={t('groups.empty')} description={t('groups.emptyDescription')}>
				{#snippet actions()}
					<Button variant="primary" icon={Plus} onclick={openCreate}>{t('groups.addAction')}</Button>
				{/snippet}
			</EmptyState>
		{/if}
	{/if}
</AppPage>

<ConfirmDialog
	open={!!deleteTarget}
	title={t('groups.deleteTitle')}
	description={t('groups.deleteDescription', {
		name: deleteTarget?.data.name || t('groups.unnamed')
	})}
	confirmLabel={t('common.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={confirmDelete}
	oncancel={() => (deleteTarget = null)}
/>

<style>
	.dirty-pill {
		display: inline-flex;
		align-items: center;
		min-height: 2rem;
		padding: 0 0.65rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.62rem;
		font-weight: 700;
	}
	.page-hint {
		margin: 0 0 var(--space-3);
		color: var(--ink-muted);
		font-size: 0.78rem;
		line-height: 1.45;
	}
	.form-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
	}
	.member-editor {
		margin-top: var(--space-4);
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.member-editor-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-3);
		border-bottom: 1px solid var(--line);
		background: var(--surface-subtle);
	}
	.member-editor-header strong { display: block; font-size: 0.8rem; }
	.member-editor-header span {
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.65rem;
	}
	.form-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		margin-top: var(--space-4);
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
	.member-chips .more { border-style: dashed; color: var(--ink-muted); }
	.empty-members { color: var(--ink-faint); font-size: 0.72rem; }
	.row-actions { display: inline-flex; gap: 0.15rem; }
	.actions-col { width: 6rem; text-align: right; }
	code { font-family: var(--font-mono); font-size: 0.75rem; }
	@media (max-width: 800px) {
		.form-grid { grid-template-columns: 1fr; }
	}
</style>
