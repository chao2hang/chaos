<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, Shield, Trash2, UserPlus } from '@lucide/svelte';
	import {
		listUsers,
		createUser,
		deleteUser,
		checkUpdate,
		getUpdateStatus,
		applyUpdate,
		ApiClientError,
		type UserDto,
		type VersionInfo,
		type UpdateStatus
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';
	import { toast } from '$lib/toast.svelte';

	let users = $state<UserDto[]>([]);
	let loaded = $state(false);
	let createOpen = $state(false);
	let creating = $state(false);
	let newUsername = $state('');
	let newPassword = $state('');
	let newRole = $state('user');
	let deleteTarget = $state<UserDto | null>(null);
	let deleting = $state(false);
	let versionInfo = $state<VersionInfo | null>(null);
	let updateStatus = $state<UpdateStatus>({
		phase: 'idle',
		target_version: null,
		message: null,
		started_at: null,
		finished_at: null
	});
	let checking = $state(false);
	let updating = $state(false);
	let confirmUpdate = $state(false);
	let statusTimer: ReturnType<typeof setInterval> | null = null;

	async function load() {
		try {
			const res = await listUsers();
			users = res.users;
		} catch (cause) {
			if (cause instanceof ApiClientError && cause.code === 'admin_required') {
				toast.error({ title: t('settings.adminRequired') });
			} else {
				toast.error({
					title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.loadFailed')
				});
			}
		} finally {
			loaded = true;
		}
	}

	async function onCreate() {
		if (!newUsername.trim() || newPassword.length < 8) {
			toast.error({ title: t('settings.invalidInput') });
			return;
		}
		creating = true;
		try {
			await createUser(newUsername.trim(), newPassword, newRole);
			toast.success({ title: t('settings.userCreated', { name: newUsername.trim() }) });
			newUsername = '';
			newPassword = '';
			newRole = 'user';
			createOpen = false;
			await load();
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.createFailed')
			});
		} finally {
			creating = false;
		}
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		deleting = true;
		try {
			await deleteUser(deleteTarget.id);
			toast.success({ title: t('settings.userDeleted', { name: deleteTarget.username }) });
			deleteTarget = null;
			await load();
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.deleteFailed')
			});
		} finally {
			deleting = false;
		}
	}

	async function checkForUpdates() {
		checking = true;
		try {
			const res = await checkUpdate();
			versionInfo = res.version;
			updateStatus = res.status;
			if (res.version.update_available) {
				toast.success({ title: t('settings.updateAvailable', { version: res.version.latest ?? '' }) });
			} else {
				toast.success({ title: t('settings.noUpdate') });
			}
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.updateCheckFailed')
			});
		} finally {
			checking = false;
		}
	}

	function stopStatusPolling() {
		if (statusTimer) {
			clearInterval(statusTimer);
			statusTimer = null;
		}
	}

	async function refreshUpdateStatus() {
		try {
			updateStatus = await getUpdateStatus();
			if (['completed', 'rolled_back', 'failed'].includes(updateStatus.phase)) {
				stopStatusPolling();
				updating = false;
				if (updateStatus.phase === 'completed') {
					toast.success({ title: t('settings.updateCompleted') });
					setTimeout(() => window.location.assign('/dashboard'), 2500);
				} else if (updateStatus.phase === 'rolled_back') {
					toast.error({ title: t('settings.updateRolledBack') });
				} else {
					toast.error({ title: t('settings.updateFailed') });
				}
			}
		} catch {
			// The API restarts during update; transient failures are expected.
		}
	}

	async function confirmApplyUpdate() {
		if (!versionInfo?.update_available) return;
		confirmUpdate = false;
		updating = true;
		try {
			const res = await applyUpdate(versionInfo.latest ?? undefined);
			updateStatus = res.status;
			toast.success({ title: t('settings.updating') });
			stopStatusPolling();
			statusTimer = setInterval(() => void refreshUpdateStatus(), 2500);
		} catch (cause) {
			updating = false;
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.updateFailed')
			});
		}
	}

	function formatDate(iso: string): string {
		try {
			return new Date(iso).toLocaleDateString();
		} catch {
			return iso;
		}
	}

	onMount(() => {
		void load();
		void checkForUpdates();
		return stopStatusPolling;
	});
</script>

<AppPage>
	<PageHeader title={t('settings.title')} description={t('settings.subtitle')} meta="chaos / settings">
		{#snippet actions()}
			<Button variant="primary" icon={createOpen ? Plus : UserPlus} onclick={() => (createOpen = !createOpen)}>
				{createOpen ? t('common.close') : t('settings.addUser')}
			</Button>
		{/snippet}
</PageHeader>

		<Section
		title={t('settings.systemUpdate')}
		description={t('settings.systemUpdateDescription')}
	>
		<div class="update-panel">
			<div class="update-metrics">
				<div>
					<span class="label">{t('settings.currentVersion')}</span>
					<strong>{versionInfo?.current ?? '—'}</strong>
				</div>
				<div>
					<span class="label">{t('settings.latestVersion')}</span>
					<strong>{versionInfo?.latest ?? '—'}</strong>
				</div>
				<div>
					<span class="label">{t('settings.daeVersion')}</span>
					<strong>{versionInfo?.dae_version ?? '—'}</strong>
				</div>
				<div>
					<span class="label">{t('settings.updateStatus')}</span>
					<strong>{t(`settings.updatePhase.${updateStatus.phase}`, updateStatus.target_version ? { version: updateStatus.target_version } : undefined)}</strong>
				</div>
			</div>
			{#if updateStatus.message}
				<p class="update-message">{updateStatus.message}</p>
			{/if}
			<div class="update-actions">
				<Button variant="secondary" loading={checking} onclick={() => void checkForUpdates()}>
					{t('settings.checkUpdates')}
				</Button>
				<Button
					variant="primary"
					loading={updating}
					disabled={!versionInfo?.update_available || updating}
					onclick={() => (confirmUpdate = true)}
				>
					{t('settings.updateNow')}
				</Button>
			</div>
		</div>
	</Section>

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
	open={confirmUpdate}
	title={t('settings.updateConfirmTitle')}
	description={t('settings.updateRestartWarning', { version: versionInfo?.latest ?? '' })}
	confirmLabel={t('settings.updateNow')}
	cancelLabel={t('common.cancel')}
	busy={updating}
	onconfirm={confirmApplyUpdate}
	oncancel={() => (confirmUpdate = false)}
/>

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
	.update-panel {
		display: grid;
		gap: var(--space-4);
	}

	.update-metrics {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: var(--space-3);
	}

	.update-metrics > div {
		display: grid;
		gap: 0.2rem;
		padding: var(--space-3);
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
	}

	.label,
	.update-message {
		color: var(--ink-muted);
		font-size: 0.74rem;
	}

	.update-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
	}

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
		.form-grid,
		.update-metrics {
			grid-template-columns: 1fr;
		}
	}
</style>
