<script lang="ts">
	import { Boxes, Plus, Trash2, X } from '@lucide/svelte';
	import type { GroupDto, GroupMemberDto, NodeDto } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import NodePickList from '$lib/components/features/NodePickList.svelte';

	const policies = ['min_moving_avg', 'min_avg10', 'min', 'random', 'fixed'];

	let {
		groups = $bindable(),
		selectedId = $bindable(),
		nodes,
		busy = false,
		isDirty,
		oncreate,
		ondelete,
		onrename
	}: {
		groups: GroupDto[];
		selectedId: string | null;
		nodes: NodeDto[];
		busy?: boolean;
		isDirty: (id: string) => boolean;
		oncreate: () => void;
		ondelete: (group: GroupDto) => void;
		onrename?: (id: string, previous: string, next: string) => void;
	} = $props();

	let groupQuery = $state('');

	function patchGroup(id: string, patch: Partial<GroupDto>) {
		const current = groups.find((group) => group.id === id);
		if (!current) return;
		if (patch.name !== undefined && patch.name !== current.name) {
			onrename?.(id, current.name, patch.name);
		}
		groups = groups.map((group) => (group.id === id ? { ...group, ...patch } : group));
	}

	function memberFromNode(node: NodeDto): GroupMemberDto {
		return {
			node_id: node.id,
			weight: 1,
			sort_order: 0,
			name: node.name,
			tag: node.tag,
			protocol: node.protocol,
			address: node.address
		};
	}

	function toggleMember(groupId: string, node: NodeDto, checked: boolean) {
		const group = groups.find((item) => item.id === groupId);
		if (!group) return;
		const members = group.members ?? [];
		const nextMembers = checked
			? members.some((member) => member.node_id === node.id)
				? members
				: [...members, memberFromNode(node)]
			: members.filter((member) => member.node_id !== node.id);
		patchGroup(groupId, { members: nextMembers });
	}

	function setWeight(groupId: string, nodeId: string, value: number) {
		const weight = Math.max(1, Math.min(99, Math.floor(value) || 1));
		const group = groups.find((item) => item.id === groupId);
		if (!group) return;
		patchGroup(groupId, {
			members: (group.members ?? []).map((member) =>
				member.node_id === nodeId ? { ...member, weight } : member
			)
		});
	}

	function selectAllFiltered() {
		if (!selectedGroup) return;
		const memberMap = new Map((selectedGroup.members ?? []).map((member) => [member.node_id, member]));
		// Search lives inside NodePickList; select-all applies to the full node pool.
		for (const node of nodes) {
			if (!memberMap.has(node.id)) memberMap.set(node.id, memberFromNode(node));
		}
		patchGroup(selectedGroup.id, { members: Array.from(memberMap.values()) });
	}

	function clearMembers() {
		if (selectedGroup) patchGroup(selectedGroup.id, { members: [] });
	}

	const filteredGroups = $derived.by(() => {
		const normalized = groupQuery.trim().toLowerCase();
		if (!normalized) return groups;
		return groups.filter((group) =>
			[group.name, group.policy, group.filter_tag]
				.filter(Boolean)
				.some((value) => String(value).toLowerCase().includes(normalized))
		);
	});

	const selectedGroup = $derived(groups.find((group) => group.id === selectedId) ?? null);
	const duplicateName = $derived(
		selectedGroup
			? groups.some(
					(group) =>
						group.id !== selectedGroup.id &&
						group.name.trim().toLowerCase() === selectedGroup.name.trim().toLowerCase()
				)
			: false
	);
</script>

<div class="group-workspace">
	<Section title={t('flow.groups')} count={groups.length} flush>
		<div class="group-list-toolbar">
			<SearchInput bind:value={groupQuery} placeholder={t('groups.searchPlaceholder')} />
			<Button
				variant="ghost"
				size="icon"
				icon={Plus}
				aria-label={t('groups.addAction')}
				title={t('groups.addAction')}
				onclick={oncreate}
			/>
		</div>
		<div class="group-list" aria-label={t('flow.groups')}>
			{#each filteredGroups as group (group.id)}
				<button
					type="button"
					class:selected={group.id === selectedId}
					onclick={() => (selectedId = group.id)}
				>
					<span class="group-name">
						<strong>{group.name || t('groups.unnamed')}</strong>
						{#if isDirty(group.id)}<span class="dirty-dot" title={t('common.unsavedChanges')}></span>{/if}
					</span>
					<span class="group-meta">{group.policy} / {group.members?.length ?? 0}</span>
				</button>
			{/each}
			{#if !filteredGroups.length}
				<EmptyState icon={Boxes} title={t('groups.empty')}>
					{#snippet actions()}<Button icon={Plus} onclick={oncreate}>{t('groups.addAction')}</Button>{/snippet}
				</EmptyState>
			{/if}
		</div>
	</Section>

	<div class="group-detail">
		{#if selectedGroup}
			<Section
				title={selectedGroup.name || t('groups.unnamed')}
				description={isDirty(selectedGroup.id) ? t('flow.groupDraftChanged') : t('flow.groupDraftSaved')}
			>
				{#snippet actions()}
					<Button
						variant="ghost"
						size="icon"
						icon={Trash2}
						disabled={busy}
						aria-label={t('groups.deleteNamed', { name: selectedGroup.name })}
						title={t('common.delete')}
						onclick={() => ondelete(selectedGroup)}
					/>
				{/snippet}

				<div class="group-fields">
					<Field
						label={t('groups.name')}
						forId="draft-group-name"
						error={duplicateName ? t('groups.nameDuplicate') : ''}
					>
						<input
							id="draft-group-name"
							type="text"
							value={selectedGroup.name}
							disabled={busy}
							oninput={(event) =>
								patchGroup(selectedGroup.id, { name: (event.currentTarget as HTMLInputElement).value })}
						/>
					</Field>
					<Field label={t('groups.policy')} forId="draft-group-policy">
						<select
							id="draft-group-policy"
							value={selectedGroup.policy}
							disabled={busy}
							onchange={(event) =>
								patchGroup(selectedGroup.id, { policy: (event.currentTarget as HTMLSelectElement).value })}
						>
							{#if !policies.includes(selectedGroup.policy)}
								<option value={selectedGroup.policy}>{selectedGroup.policy}</option>
							{/if}
							{#each policies as option}<option value={option}>{t(`flow.policy.${option}`)}</option>{/each}
						</select>
					</Field>
					<Field label={t('groups.filterTag')} forId="draft-group-filter" optional>
						<input
							id="draft-group-filter"
							type="text"
							value={selectedGroup.filter_tag ?? ''}
							disabled={busy}
							placeholder={t('groups.filterPlaceholder')}
							oninput={(event) =>
								patchGroup(selectedGroup.id, {
									filter_tag: (event.currentTarget as HTMLInputElement).value || null
								})}
						/>
					</Field>
				</div>
			</Section>

			<Section
				title={t('groups.members')}
				description={t('flow.membersDescription')}
				count={selectedGroup.members?.length ?? 0}
			>
{#snippet actions()}
						<Button size="sm" disabled={busy || !nodes.length} onclick={selectAllFiltered}>
							{t('common.selectVisible')}
						</Button>
						<Button
							variant="ghost"
							size="sm"
							icon={X}
							disabled={busy || !(selectedGroup.members?.length)}
							onclick={clearMembers}
						>
							{t('common.clear')}
						</Button>
					{/snippet}

					<NodePickList
						{nodes}
						{busy}
						isChecked={(node) => !!(selectedGroup.members?.some((m) => m.node_id === node.id))}
						onToggle={(node, checked) => toggleMember(selectedGroup.id, node, checked)}
						showWeight={true}
						getWeight={(node) =>
							selectedGroup.members?.find((m) => m.node_id === node.id)?.weight ?? 1}
						onWeight={(node, weight) => setWeight(selectedGroup.id, node.id, weight)}
					/>
				</Section>
		{:else}
			<Section flush>
				<EmptyState icon={Boxes} title={t('flow.selectGroup')} description={t('flow.selectGroupDescription')}>
					{#snippet actions()}<Button icon={Plus} onclick={oncreate}>{t('groups.addAction')}</Button>{/snippet}
				</EmptyState>
			</Section>
		{/if}
	</div>
</div>

<style>
	.group-workspace {
		display: grid;
		grid-template-columns: minmax(13rem, 0.32fr) minmax(0, 1fr);
		align-items: start;
		gap: var(--space-4);
	}

	.group-list-toolbar {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3);
		border-bottom: 1px solid var(--line);
	}

	.group-list-toolbar :global(.search-field) {
		width: 100%;
	}

	.group-list {
		display: flex;
		max-height: 34rem;
		min-height: 12rem;
		flex-direction: column;
		overflow-y: auto;
	}

	.group-list > button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		width: 100%;
		min-height: 3.2rem;
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-bottom: 1px solid var(--line);
		border-radius: 0;
		background: var(--surface);
		color: var(--ink-muted);
		text-align: left;
	}

	.group-list > button:hover {
		background: var(--surface-subtle);
		color: var(--ink);
	}

	.group-list > button.selected {
		background: var(--surface-inverse);
		color: var(--ink-inverse);
	}

	.group-name {
		display: flex;
		min-width: 0;
		align-items: center;
		gap: var(--space-2);
	}

	.group-name strong {
		overflow: hidden;
		font-size: 0.8rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.dirty-dot {
		width: 0.38rem;
		height: 0.38rem;
		border-radius: 50%;
		background: currentColor;
	}

	.group-meta {
		font-family: var(--font-mono);
		font-size: 0.62rem;
		opacity: 0.72;
		white-space: nowrap;
	}

	.group-detail {
		display: flex;
		min-width: 0;
		flex-direction: column;
		gap: var(--space-4);
	}

.group-fields {
			display: grid;
			grid-template-columns: repeat(3, minmax(0, 1fr));
			gap: var(--space-4);
		}

		@media (max-width: 900px) {
			.group-workspace {
				grid-template-columns: 1fr;
			}

			.group-list {
				max-height: 16rem;
			}
		}

		@media (max-width: 640px) {
			.group-fields {
				grid-template-columns: 1fr;
			}
		}
	</style>
