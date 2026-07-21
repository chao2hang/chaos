<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { Boxes, Pencil, Plus, Trash2, X } from '@lucide/svelte';
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
	import ResourceToolbar from '$lib/components/ui/ResourceToolbar.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';
	import NodePickList from '$lib/components/features/NodePickList.svelte';

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
					</div>
					<NodePickList
						{nodes}
						{busy}
						isChecked={(node) => members.some((item) => item.node_id === node.id)}
						onToggle={toggleMember}
						showWeight={true}
						getWeight={(node) => members.find((item) => item.node_id === node.id)?.weight ?? 1}
						onWeight={(node, weight) => setMemberWeight(node.id, weight)}
					/>
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
		<ResourceToolbar
			bind:value={query}
			placeholder={t('groups.searchPlaceholder')}
			meta={t('groups.count', { count: groups.length })}
			refreshLabel={t('common.refresh')}
			onrefresh={load}
		/>

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

	.form-actions {
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

	.member-editor-header strong {
		font-size: 0.78rem;
	}

	.member-editor-header span {
		color: var(--ink-muted);
		font-size: 0.68rem;
	}

	.member-editor :global(.node-pick-list) {
		padding: var(--space-3);
	}

	.member-count {
		color: var(--ink-muted);
		font-size: 0.72rem;
	}

	@media (max-width: 760px) {
		.form-grid {
			grid-template-columns: 1fr;
		}

		.member-editor-header {
			align-items: stretch;
			flex-direction: column;
		}
	}
</style>
