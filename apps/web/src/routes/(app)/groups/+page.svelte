<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listGroups,
		createGroup,
		updateGroup,
		deleteGroup,
		ApiClientError,
		type GroupDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n';

	let groups = $state<GroupDto[]>([]);
	let error = $state('');
	let message = $state('');
	let busy = $state(false);

	let name = $state('');
	let policy = $state('min_moving_avg');
	let filterTag = $state('');

	let editId = $state<string | null>(null);
	let editName = $state('');
	let editPolicy = $state('');
	let editFilter = $state('');

	async function load() {
		error = '';
		try {
			const res = await listGroups();
			groups = res.groups;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('groups.loadFailed');
		}
	}

	onMount(() => {
		void load();
	});

	async function onCreate() {
		error = '';
		message = '';
		const n = name.trim();
		if (!n) {
			error = t('groups.nameRequired');
			return;
		}
		busy = true;
		try {
			await createGroup({
				name: n,
				policy: policy.trim() || 'min_moving_avg',
				filter_tag: filterTag.trim() || undefined
			});
			name = '';
			filterTag = '';
			message = t('groups.created');
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('groups.saveFailed');
		} finally {
			busy = false;
		}
	}

	function startEdit(g: GroupDto) {
		editId = g.id;
		editName = g.name;
		editPolicy = g.policy;
		editFilter = g.filter_tag ?? '';
	}

	async function onSaveEdit() {
		if (!editId) return;
		error = '';
		message = '';
		const n = editName.trim();
		if (!n) {
			error = t('groups.nameRequired');
			return;
		}
		busy = true;
		try {
			await updateGroup(editId, {
				name: n,
				policy: editPolicy.trim() || 'min_moving_avg',
				filter_tag: editFilter.trim() || null
			});
			editId = null;
			message = t('groups.updated');
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('groups.saveFailed');
		} finally {
			busy = false;
		}
	}

	async function onDelete(id: string) {
		if (!confirm(t('groups.deleteConfirm'))) return;
		error = '';
		try {
			await deleteGroup(id);
			message = t('groups.deleted');
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('groups.deleteFailed');
		}
	}
</script>

<span class="eyebrow">policy · outbounds</span>
<h1 class="page-title">{t('groups.title')}</h1>
<p class="page-sub">{t('groups.subtitle')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="import">
	<label for="g-name">{t('groups.name')}</label>
	<input id="g-name" bind:value={name} disabled={busy} />
	<label for="g-policy">{t('groups.policy')}</label>
	<input id="g-policy" bind:value={policy} disabled={busy} placeholder="min_moving_avg" />
	<label for="g-tag">{t('groups.filterTag')}</label>
	<input id="g-tag" bind:value={filterTag} disabled={busy} placeholder="hk" />
	<button type="button" class="primary" disabled={busy} onclick={onCreate}>{t('common.add')}</button>
</section>

<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>{t('groups.name')}</th>
				<th>{t('groups.policy')}</th>
				<th>{t('groups.filterTag')}</th>
				<th>{t('common.actions')}</th>
			</tr>
		</thead>
		<tbody>
			{#if !groups.length}
				<tr>
					<td colspan="4" class="muted">{t('groups.empty')}</td>
				</tr>
			{:else}
				{#each groups as g (g.id)}
					<tr>
						{#if editId === g.id}
							<td><input bind:value={editName} disabled={busy} /></td>
							<td><input bind:value={editPolicy} disabled={busy} /></td>
							<td><input bind:value={editFilter} disabled={busy} /></td>
							<td class="row-actions">
								<button type="button" class="primary" disabled={busy} onclick={onSaveEdit}
									>{t('common.save')}</button
								>
								<button type="button" class="ghost" disabled={busy} onclick={() => (editId = null)}
									>×</button
								>
							</td>
						{:else}
							<td><strong>{g.name}</strong></td>
							<td><code class="policy">{g.policy}</code></td>
							<td class="tag-cell">{g.filter_tag ?? t('common.emDash')}</td>
							<td class="row-actions">
								<button type="button" onclick={() => startEdit(g)}>Edit</button>
								<button type="button" class="danger" onclick={() => onDelete(g.id)}
									>{t('common.delete')}</button
								>
							</td>
						{/if}
					</tr>
				{/each}
			{/if}
		</tbody>
	</table>
</section>

<style>
	.policy {
		font-size: 0.82rem;
		color: var(--signal);
	}
	.tag-cell {
		font-family: var(--font-mono);
		font-size: 0.85rem;
		color: var(--ink-muted);
	}
</style>
