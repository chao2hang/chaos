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

<h1>{t('groups.title')}</h1>
<p class="muted">{t('groups.subtitle')}</p>

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
								<button type="button" disabled={busy} onclick={() => (editId = null)}>…</button>
							</td>
						{:else}
							<td><strong>{g.name}</strong></td>
							<td><code>{g.policy}</code></td>
							<td>{g.filter_tag ?? t('common.emDash')}</td>
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
	h1 {
		margin: 0 0 0.25rem;
		font-size: 1.5rem;
	}
	.muted {
		color: #555;
		font-size: 0.95rem;
	}
	.error {
		color: #b42318;
		font-size: 0.9rem;
	}
	.ok {
		color: #027a48;
		font-size: 0.9rem;
	}
	.import {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 1.25rem 0;
		background: #fff;
		border: 1px solid #e5e7eb;
		border-radius: 8px;
		padding: 1rem;
	}
	label {
		font-size: 0.9rem;
		font-weight: 600;
	}
	input {
		font: inherit;
		padding: 0.5rem 0.6rem;
		border: 1px solid #ccc;
		border-radius: 6px;
	}
	button {
		font: inherit;
		padding: 0.45rem 0.75rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		background: #fff;
		cursor: pointer;
		align-self: flex-start;
	}
	button.primary {
		background: #1a56db;
		border-color: #1a56db;
		color: #fff;
	}
	button.danger {
		color: #b42318;
		border-color: #f3b0a8;
	}
	button:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}
	.table-wrap {
		overflow-x: auto;
		background: #fff;
		border: 1px solid #e5e7eb;
		border-radius: 8px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.9rem;
	}
	th,
	td {
		text-align: left;
		padding: 0.55rem 0.75rem;
		border-bottom: 1px solid #eee;
		vertical-align: top;
	}
	th {
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: #555;
		background: #fafafa;
	}
	.row-actions {
		display: flex;
		gap: 0.35rem;
		white-space: nowrap;
	}
	code {
		font-size: 0.85em;
	}
</style>
