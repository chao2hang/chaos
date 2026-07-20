<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { Boxes, Pencil, Plus, RefreshCw, Trash2, X } from '@lucide/svelte';
	import {
		listGroups,
		listNodes,
		createGroup,
		updateGroup,
		deleteGroup,
		replaceGroupMembers,
		ApiClientError,
		isSessionRedirectPending,
		type GroupDto,
		type GroupMemberDto,
		type NodeDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';

	const policies = ['min_moving_avg', 'fixed', 'random', 'min'];

	let groups = $state<GroupDto[]>([]);
	let nodes = $state<NodeDto[]>([]);
	let query = $state('');
	let error = $state('');
	let message = $state('');
	let loaded = $state(false);
	let formOpen = $state(false);
	let busy = $state(false);
	let editId = $state<string | null>(null);
	let name = $state('');
	let policy = $state('min_moving_avg');
	let filterTag = $state('');
	let deleteTarget = $state<GroupDto | null>(null);
	let memberQuery = $state('');
	let members = $state<GroupMemberDto[]>([]);
	let formSnapshot = $state('');

	function currentFormSnapshot() {
		return JSON.stringify({ editId, name, policy, filterTag, members });
	}

	async function load() {
		error = '';
		try {
			const [groupResult, nodeResult] = await Promise.all([listGroups(), listNodes()]);
			groups = groupResult.groups;
			nodes = nodeResult.nodes;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('groups.loadFailed');
		} finally {
			loaded = true;
		}
	}

	onMount(() => {
		void load();
	});

	beforeNavigate(({ cancel }) => {
		if (!formDirty || typeof window === 'undefined') return;
		if (isSessionRedirectPending()) return;
		if (sessionStorage.getItem('chaos_allow_dirty_navigation') === '1') {
			sessionStorage.removeItem('chaos_allow_dirty_navigation');
			return;
		}
		if (!window.confirm(t('common.discardDescription'))) cancel();
	});

	$effect(() => {
		if (typeof document === 'undefined') return;
		document.documentElement.dataset.chaosUnsaved = formDirty ? 'true' : 'false';
		return () => {
			delete document.documentElement.dataset.chaosUnsaved;
		};
	});

	function openCreate() {
		editId = null;
		name = '';
		policy = 'min_moving_avg';
		filterTag = '';
		members = [];
		memberQuery = '';
		formOpen = true;
		formSnapshot = currentFormSnapshot();
		error = '';
	}

	function openEdit(group: GroupDto) {
		editId = group.id;
		name = group.name;
		policy = group.policy;
		filterTag = group.filter_tag ?? '';
		members = (group.members ?? []).map((member) => ({ ...member }));
		memberQuery = '';
		formOpen = true;
		formSnapshot = currentFormSnapshot();
		error = '';
	}

	function closeForm() {
		if (busy) return;
		if (formDirty && typeof window !== 'undefined' && !window.confirm(t('common.discardDescription'))) return;
		formOpen = false;
		editId = null;
	}

	async function onSave() {
		error = '';
		message = '';
		const normalizedName = name.trim();
		if (!normalizedName) {
			error = t('groups.nameRequired');
			return;
		}
		const duplicate = groups.some(
			(group) => group.id !== editId && group.name.toLowerCase() === normalizedName.toLowerCase()
		);
		if (duplicate) {
			error = t('groups.nameDuplicate');
			return;
		}

		busy = true;
		let savedGroup: GroupDto | null = null;
		try {
			if (editId) {
				savedGroup = await updateGroup(editId, {
					name: normalizedName,
					policy,
					filter_tag: filterTag.trim() || null
				});
				message = t('groups.updated');
			} else {
				savedGroup = await createGroup({
					name: normalizedName,
					policy,
					filter_tag: filterTag.trim() || undefined
				});
				message = t('groups.created');
			}
			await replaceGroupMembers(
				savedGroup.id,
				members.map((member) => ({ node_id: member.node_id, weight: member.weight }))
			);
			closeForm();
			await load();
		} catch (cause) {
			if (savedGroup) {
				editId = savedGroup.id;
				formOpen = true;
				error = t('groups.membersSaveFailed');
			} else {
				error = cause instanceof ApiClientError ? apiErrorText(cause) : t('groups.saveFailed');
			}
		} finally {
			busy = false;
			if (!error) formOpen = false;
		}
	}

	function toggleMember(node: NodeDto, checked: boolean) {
		if (checked) {
			if (members.some((member) => member.node_id === node.id)) return;
			members = [
				...members,
				{
					node_id: node.id,
					weight: 1,
					sort_order: members.length,
					name: node.name,
					tag: node.tag,
					protocol: node.protocol,
					address: node.address
				}
			];
		} else {
			members = members.filter((member) => member.node_id !== node.id);
		}
	}

	function setMemberWeight(nodeId: string, value: number) {
		const weight = Math.max(1, Math.min(99, Math.floor(value) || 1));
		members = members.map((member) => (member.node_id === nodeId ? { ...member, weight } : member));
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		busy = true;
		error = '';
		try {
			const id = deleteTarget.id;
			await deleteGroup(id);
			groups = groups.filter((group) => group.id !== id);
			deleteTarget = null;
			message = t('groups.deleted');
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('groups.deleteFailed');
		} finally {
			busy = false;
		}
	}

	const filteredGroups = $derived.by(() => {
		const normalized = query.trim().toLowerCase();
		if (!normalized) return groups;
		return groups.filter((group) =>
			[group.name, group.policy, group.filter_tag]
				.filter(Boolean)
				.some((value) => String(value).toLowerCase().includes(normalized))
		);
	});
	const filteredNodes = $derived.by(() => {
		const normalized = memberQuery.trim().toLowerCase();
		if (!normalized) return nodes;
		return nodes.filter((node) =>
			[node.name, node.tag, node.protocol, node.address]
				.filter(Boolean)
				.some((value) => String(value).toLowerCase().includes(normalized))
		);
	});
	const formDirty = $derived(formOpen && currentFormSnapshot() !== formSnapshot);
</script>

<div class="page-stack">
	<PageHeader title={t('groups.title')} description={t('groups.subtitle')} meta="policy / groups">
		{#snippet actions()}
			<Button variant="primary" icon={formOpen ? X : Plus} onclick={formOpen ? closeForm : openCreate}>
				{formOpen ? t('common.close') : t('groups.addAction')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}

	{#if formOpen}
		<Section
			title={editId ? t('groups.editTitle') : t('groups.createTitle')}
			description={t('groups.formDescription')}
		>
			<form onsubmit={(event) => { event.preventDefault(); void onSave(); }}>
				<div class="form-grid">
					<Field label={t('groups.name')} forId="group-name">
						<input id="group-name" type="text" bind:value={name} disabled={busy} required />
					</Field>
					<Field label={t('groups.policy')} forId="group-policy">
						<select id="group-policy" bind:value={policy} disabled={busy}>
							{#each policies as option}
								<option value={option}>{option}</option>
							{/each}
						</select>
					</Field>
					<Field label={t('groups.filterTag')} forId="group-filter" optional>
						<input
							id="group-filter"
							type="text"
							bind:value={filterTag}
							disabled={busy}
							placeholder={t('groups.filterPlaceholder')}
						/>
					</Field>
				</div>
				<div class="member-editor">
					<div class="member-editor-header">
						<div>
							<strong>{t('groups.members')}</strong>
							<span>{t('groups.memberCount', { count: members.length })}</span>
						</div>
						<SearchInput bind:value={memberQuery} placeholder={t('flow.searchNodes')} />
					</div>
					<div class="member-list">
						{#each filteredNodes as node (node.id)}
							{@const member = members.find((item) => item.node_id === node.id)}
							<div class:active={!!member} class="member-row">
								<label>
									<input
										type="checkbox"
										checked={!!member}
										disabled={busy}
										onchange={(event) =>
											toggleMember(node, (event.currentTarget as HTMLInputElement).checked)}
									/>
									<span>
										<strong>{node.name}</strong>
										<small>{[node.protocol, node.address].filter(Boolean).join(' / ') || t('common.unknown')}</small>
									</span>
								</label>
								{#if member}
									<label class="member-weight">
										<span>{t('flow.weight')}</span>
										<input
											type="number"
											min="1"
											max="99"
											value={member.weight}
											disabled={busy}
											onchange={(event) =>
												setMemberWeight(node.id, Number((event.currentTarget as HTMLInputElement).value))}
										/>
									</label>
								{/if}
							</div>
						{/each}
						{#if !filteredNodes.length}
							<div class="member-empty">{nodes.length ? t('common.noSearchResults') : t('flow.emptyPool')}</div>
						{/if}
					</div>
				</div>
				<div class="form-actions">
					<Button type="button" variant="ghost" disabled={busy} onclick={closeForm}>{t('common.cancel')}</Button>
					<Button type="submit" variant="primary" loading={busy}>
						{busy ? t('common.saving') : t('common.save')}
					</Button>
				</div>
			</form>
		</Section>
	{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<div class="resource-toolbar">
			<SearchInput bind:value={query} placeholder={t('groups.searchPlaceholder')} label={t('groups.searchPlaceholder')} />
			<div class="toolbar-meta">
				<span>{t('groups.count', { count: groups.length })}</span>
				<Button
					variant="ghost"
					size="icon"
					icon={RefreshCw}
					aria-label={t('common.refresh')}
					title={t('common.refresh')}
					onclick={load}
				/>
			</div>
		</div>

		{#if filteredGroups.length}
			<TableFrame>
				<table>
					<thead>
						<tr>
							<th>{t('groups.name')}</th>
							<th>{t('groups.policy')}</th>
							<th>{t('groups.filterTag')}</th>
							<th>{t('groups.members')}</th>
							<th class="actions-col">{t('common.actions')}</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredGroups as group (group.id)}
							<tr>
								<td data-label={t('groups.name')}><strong>{group.name}</strong></td>
								<td data-label={t('groups.policy')}><code>{group.policy}</code></td>
								<td data-label={t('groups.filterTag')} class="data-meta">{group.filter_tag ?? t('common.none')}</td>
								<td data-label={t('groups.members')} class="member-count">
									{t('groups.memberCount', { count: group.members?.length ?? 0 })}
								</td>
								<td data-label={t('common.actions')}>
									<div class="row-actions">
									<Button variant="ghost" size="sm" icon={Pencil} onclick={() => openEdit(group)}>
										{t('groups.configure')}
									</Button>
										<Button
											variant="ghost"
											size="icon"
											icon={Trash2}
											aria-label={t('groups.deleteNamed', { name: group.name })}
											title={t('common.delete')}
											onclick={() => (deleteTarget = group)}
										/>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</TableFrame>
		{:else}
			<Section flush>
				<EmptyState
					icon={Boxes}
					title={query ? t('common.noSearchResults') : t('groups.empty')}
					description={query ? t('common.tryDifferentSearch') : t('groups.emptyDescription')}
				>
					{#snippet actions()}
						{#if !query}<Button icon={Plus} onclick={openCreate}>{t('groups.addAction')}</Button>{/if}
					{/snippet}
				</EmptyState>
			</Section>
		{/if}
	{/if}
</div>

<ConfirmDialog
	open={!!deleteTarget}
	title={t('groups.deleteTitle')}
	description={t('groups.deleteDescription', { name: deleteTarget?.name ?? '' })}
	confirmLabel={t('common.delete')}
	cancelLabel={t('common.cancel')}
	busy={busy}
	danger
	onconfirm={confirmDelete}
	oncancel={() => (deleteTarget = null)}
/>

<style>
	form {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
	}

	.form-grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-4);
	}

	.form-actions,
	.resource-toolbar,
	.toolbar-meta {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.form-actions {
		justify-content: flex-end;
	}

	.member-editor {
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.member-editor-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-3);
		border-bottom: 1px solid var(--line);
		background: var(--surface-subtle);
	}

	.member-editor-header > div {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}

	.member-editor-header strong { font-size: 0.78rem; }
	.member-editor-header span { color: var(--ink-muted); font-size: 0.68rem; }
	.member-editor-header :global(.search-field) { width: min(20rem, 50%); }

	.member-list {
		display: flex;
		max-height: 24rem;
		flex-direction: column;
		overflow-y: auto;
	}

	.member-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-3);
		min-height: 3.2rem;
		padding: 0.45rem var(--space-3);
		border-bottom: 1px solid var(--line);
	}

	.member-row:last-child { border-bottom: 0; }
	.member-row.active { background: var(--surface-subtle); }
	.member-row > label:first-child { display: flex; min-width: 0; align-items: center; gap: var(--space-3); }
	.member-row > label:first-child > span { display: flex; min-width: 0; flex-direction: column; }
	.member-row strong, .member-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.member-row strong { font-size: 0.75rem; }
	.member-row small { color: var(--ink-muted); font-family: var(--font-mono); font-size: 0.62rem; }
	.member-weight { display: grid; grid-template-columns: auto 3.5rem; align-items: center; gap: var(--space-2); }
	.member-weight span { color: var(--ink-muted); font-size: 0.65rem; }
	.member-weight input { min-height: 2rem; padding: 0.25rem 0.4rem; }
	.member-empty { padding: var(--space-6); color: var(--ink-muted); font-size: 0.75rem; text-align: center; }

	.toolbar-meta > span,
	.member-count {
		color: var(--ink-muted);
		font-size: 0.72rem;
	}

	.actions-col {
		text-align: right;
	}

	@media (max-width: 760px) {
		.form-grid {
			grid-template-columns: 1fr;
		}

		.resource-toolbar {
			align-items: stretch;
			flex-direction: column;
		}

		.toolbar-meta {
			justify-content: flex-end;
		}

		.member-editor-header { align-items: stretch; flex-direction: column; }
		.member-editor-header :global(.search-field) { width: 100%; }

		:global(.table-frame) {
			overflow: visible;
			border: 0;
		}

		table,
		tbody,
		tr,
		td {
			display: block;
			width: 100%;
		}

		thead {
			display: none;
		}

		tbody {
			display: flex;
			flex-direction: column;
			gap: var(--space-3);
		}

		tr {
			padding: var(--space-3);
			border: 1px solid var(--line);
			border-radius: var(--radius-lg);
		}

		td {
			display: grid;
			grid-template-columns: 6.5rem minmax(0, 1fr);
			gap: var(--space-3);
			padding: 0.38rem 0;
			border: 0;
			text-align: right;
		}

		td::before {
			content: attr(data-label);
			color: var(--ink-faint);
			font-size: 0.7rem;
			font-weight: 600;
			text-align: left;
		}

		.row-actions {
			justify-content: flex-end;
		}
	}
</style>
