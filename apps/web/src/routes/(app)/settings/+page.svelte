<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, Shield, Trash2, UserPlus } from '@lucide/svelte';
	import {
		listUsers,
		createUser,
		deleteUser,
		ApiClientError,
		type UserDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';

	let users = $state<UserDto[]>([]);
	let loaded = $state(false);
	let error = $state('');
	let message = $state('');
	let createOpen = $state(false);
	let creating = $state(false);
	let newUsername = $state('');
	let newPassword = $state('');
	let newRole = $state('user');
	let deleteTarget = $state<UserDto | null>(null);
	let deleting = $state(false);

	async function load() {
		error = '';
		try {
			const res = await listUsers();
			users = res.users;
		} catch (cause) {
			if (cause instanceof ApiClientError && cause.code === 'admin_required') {
				error = t('settings.adminRequired');
			} else {
				error = cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.loadFailed');
			}
		} finally {
			loaded = true;
		}
	}

	async function onCreate() {
		error = '';
		message = '';
		if (!newUsername.trim() || newPassword.length < 8) {
			error = t('settings.invalidInput');
			return;
		}
		creating = true;
		try {
			await createUser(newUsername.trim(), newPassword, newRole);
			message = t('settings.userCreated', { name: newUsername.trim() });
			newUsername = '';
			newPassword = '';
			newRole = 'user';
			createOpen = false;
			await load();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.createFailed');
		} finally {
			creating = false;
		}
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		deleting = true;
		error = '';
		try {
			await deleteUser(deleteTarget.id);
			message = t('settings.userDeleted', { name: deleteTarget.username });
			deleteTarget = null;
			await load();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.deleteFailed');
		} finally {
			deleting = false;
		}
	}

	function formatDate(iso: string): string {
		try {
			return new Date(iso).toLocaleDateString();
		} catch {
			return iso;
		}
	}

	onMount(() => { void load(); });
</script>

<AppPage>
	<PageHeader title={t('settings.title')} description={t('settings.subtitle')} meta="chaos / settings">
		{#snippet actions()}
			<Button variant="primary" icon={createOpen ? Plus : UserPlus} onclick={() => (createOpen = !createOpen)}>
				{createOpen ? t('common.close') : t('settings.addUser')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}

	{#if createOpen}
		<Section title={t('settings.createUserTitle')}>
			<form class="form-stack" onsubmit={(e) => { e.preventDefault(); void onCreate(); }}>
				<div class="form-grid">
					<Field label={t('settings.username')} forId="new-username">
						<input id="new-username" type="text" bind:value={newUsername} disabled={creating} required />
					</Field>
					<Field label={t('settings.password')} forId="new-password">
						<input id="new-password" type="password" bind:value={newPassword} disabled={creating} minlength={8} required />
					</Field>
					<Field label={t('settings.role')} forId="new-role">
						<select id="new-role" bind:value={newRole} disabled={creating}>
							<option value="user">user</option>
							<option value="admin">admin</option>
						</select>
					</Field>
				</div>
				<div class="form-footer">
					<span>{t('settings.passwordHint')}</span>
					<Button type="submit" variant="primary" loading={creating}>{t('common.create')}</Button>
				</div>
			</form>
		</Section>
	{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<Section title={t('settings.users')} count={users.length} flush>
			<TableFrame>
				<table>
					<thead>
						<tr>
							<th>{t('settings.username')}</th>
							<th>{t('settings.role')}</th>
							<th>{t('settings.createdAt')}</th>
							<th class="actions-col">{t('common.actions')}</th>
						</tr>
					</thead>
					<tbody>
						{#each users as user (user.id)}
							<tr>
								<td><strong>{user.username}</strong></td>
								<td>
									<span class="role-badge" class:admin={user.role === 'admin'}>
										{#if user.role === 'admin'}<Shield size={12} />{/if}
										{user.role}
									</span>
								</td>
								<td class="date">{formatDate(user.created_at)}</td>
								<td>
									<div class="row-actions">
										<Button
											variant="ghost"
											size="icon"
											icon={Trash2}
											title={t('common.delete')}
											onclick={() => (deleteTarget = user)}
										/>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</TableFrame>
		</Section>
	{/if}
</AppPage>

<ConfirmDialog
	open={!!deleteTarget}
	title={t('settings.deleteUserTitle')}
	description={t('settings.deleteUserDescription', { name: deleteTarget?.username ?? '' })}
	confirmLabel={t('common.delete')}
	cancelLabel={t('common.cancel')}
	busy={deleting}
	danger
	onconfirm={confirmDelete}
	oncancel={() => (deleteTarget = null)}
/>

<style>
	.form-grid {
		display: grid;
		grid-template-columns: 1fr 1fr auto;
		gap: var(--space-4);
		align-items: end;
	}

	.role-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		padding: 0.15rem 0.5rem;
		border-radius: var(--radius-sm);
		background: var(--surface-subtle);
		font-size: 0.72rem;
		font-weight: 600;
	}

	.role-badge.admin {
		background: var(--surface-inverse);
		color: var(--ink-inverse);
	}

	.date {
		color: var(--ink-muted);
		font-size: 0.74rem;
	}

	@media (max-width: 720px) {
		.form-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
