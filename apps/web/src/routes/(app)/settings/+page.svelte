<script lang="ts">
	import { onMount } from 'svelte';
	import { Copy, KeyRound, Plus, Shield, Trash2, UserPlus } from '@lucide/svelte';
	import {
		listUsers,
		createUser,
		deleteUser,
		listApiKeys,
		createApiKey,
		revokeApiKey,
		checkUpdate,
		getUpdateStatus,
		applyUpdate,
		ApiClientError,
		type UserDto,
		type ApiKeyDto,
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
	let apiKeys = $state<ApiKeyDto[]>([]);
	let keyOpen = $state(false);
	let creatingKey = $state(false);
	let keyName = $state('');
	let keyScopes = $state<string[]>([
		'orchestration:read',
		'orchestration:validate',
		'orchestration:simulate',
		'orchestration:plan'
	]);
	let newKeySecret = $state<string | null>(null);
	let revokeKeyTarget = $state<ApiKeyDto | null>(null);
	let revokingKey = $state(false);
	const availableKeyScopes = [
		'orchestration:read',
		'orchestration:validate',
		'orchestration:simulate',
		'orchestration:plan',
		'orchestration:publish',
		'diagnostics:read'
	];
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
			const [userRes, keyRes] = await Promise.all([listUsers(), listApiKeys()]);
			users = userRes.users;
			apiKeys = keyRes;
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

	function toggleKeyScope(scope: string) {
		keyScopes = keyScopes.includes(scope)
			? keyScopes.filter((item) => item !== scope)
			: [...keyScopes, scope];
	}

	async function onCreateKey() {
		if (!keyName.trim() || keyScopes.length === 0) {
			toast.error({ title: t('settings.apiKeyInvalid') });
			return;
		}
		creatingKey = true;
		try {
			const created = await createApiKey(keyName.trim(), keyScopes);
			newKeySecret = created.key ?? null;
			keyName = '';
			keyOpen = false;
			toast.success({ title: t('settings.apiKeyCreated') });
			await load();
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.apiKeyCreateFailed')
			});
		} finally {
			creatingKey = false;
		}
	}

	async function copyKey() {
		if (!newKeySecret) return;
		try {
			if (navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(newKeySecret);
			} else {
				const input = document.createElement('textarea');
				input.value = newKeySecret;
				input.setAttribute('readonly', '');
				input.style.position = 'fixed';
				input.style.opacity = '0';
				document.body.appendChild(input);
				input.select();
				if (!document.execCommand('copy')) throw new Error('copy command failed');
				input.remove();
			}
			toast.success({ title: t('settings.apiKeyCopied') });
		} catch {
			toast.error({ title: t('settings.apiKeyCopyFailed') });
		}
	}

	async function confirmRevokeKey() {
		if (!revokeKeyTarget) return;
		revokingKey = true;
		try {
			await revokeApiKey(revokeKeyTarget.id);
			toast.success({ title: t('settings.apiKeyRevoked') });
			revokeKeyTarget = null;
			await load();
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('settings.apiKeyRevokeFailed')
			});
		} finally {
			revokingKey = false;
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

	async function checkForUpdates(force = false) {
		checking = true;
		try {
			const res = await checkUpdate(force);
			versionInfo = res.version;
			updateStatus = res.status;
			if (res.version.error) {
				// GitHub unreachable: not a chaos failure — show a muted hint.
				toast.warning({ title: t('settings.updateCheckUnavailable') });
			} else if (res.version.update_available) {
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
			{#if versionInfo?.error}
				<p class="update-message update-error">{t('settings.updateCheckUnavailable')}</p>
			{/if}
			<div class="update-actions">
				<Button variant="secondary" loading={checking} onclick={() => void checkForUpdates(true)}>
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

	<Section title={t('settings.apiKeysTitle')} description={t('settings.apiKeysDescription')}>
		<div class="key-toolbar">
			<Button variant="secondary" icon={KeyRound} onclick={() => (keyOpen = !keyOpen)}>
				{keyOpen ? t('common.close') : t('settings.createApiKey')}
			</Button>
		</div>
		{#if newKeySecret}
			<div class="secret-panel">
				<strong>{t('settings.apiKeySecretTitle')}</strong>
				<p>{t('settings.apiKeySecretHint')}</p>
				<div class="secret-row"><code>{newKeySecret}</code><Button variant="secondary" size="sm" icon={Copy} onclick={() => void copyKey()}>{t('settings.copyApiKey')}</Button></div>
			</div>
		{/if}
		{#if keyOpen}
			<form class="form-stack key-form" onsubmit={(e) => { e.preventDefault(); void onCreateKey(); }}>
				<Field label={t('settings.apiKeyName')} forId="api-key-name">
					<input id="api-key-name" type="text" bind:value={keyName} disabled={creatingKey} placeholder="orchestration-agent" required />
				</Field>
				<div class="scope-grid">
					{#each availableKeyScopes as scope}
						<label class="scope-option"><input type="checkbox" checked={keyScopes.includes(scope)} onchange={() => toggleKeyScope(scope)} /> <span>{scope}</span></label>
					{/each}
				</div>
				<div class="form-footer"><span>{t('settings.apiKeyScopeHint')}</span><Button type="submit" variant="primary" loading={creatingKey}>{t('common.create')}</Button></div>
			</form>
		{/if}
		{#if apiKeys.length > 0}
			<TableFrame>
				<table>
					<thead><tr><th>{t('settings.apiKeyName')}</th><th>{t('settings.apiKeyPrefix')}</th><th>{t('settings.apiKeyScopes')}</th><th>{t('settings.apiKeyLastUsed')}</th><th class="actions-col">{t('common.actions')}</th></tr></thead>
					<tbody>{#each apiKeys as apiKey (apiKey.id)}<tr>
						<td><strong>{apiKey.name}</strong></td><td><code>{apiKey.key_prefix}…</code></td><td class="scope-list">{apiKey.scopes.join(', ')}</td><td class="date">{apiKey.last_used_at ? formatDate(apiKey.last_used_at) : t('settings.apiKeyNeverUsed')}</td>
						<td><Button variant="ghost" size="icon" icon={Trash2} title={t('settings.revokeApiKey')} onclick={() => (revokeKeyTarget = apiKey)} /></td>
					</tr>{/each}</tbody>
				</table>
			</TableFrame>
		{:else}<p class="empty-hint">{t('settings.apiKeyEmpty')}</p>{/if}
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
	open={!!revokeKeyTarget}
	title={t('settings.revokeApiKeyTitle')}
	description={t('settings.revokeApiKeyDescription', { name: revokeKeyTarget?.name ?? '' })}
	confirmLabel={t('settings.revokeApiKey')}
	cancelLabel={t('common.cancel')}
	busy={revokingKey}
	danger
	onconfirm={confirmRevokeKey}
	oncancel={() => (revokeKeyTarget = null)}
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

	.key-toolbar { display: flex; justify-content: flex-end; }
	.key-form { margin-top: var(--space-4); }
	.scope-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-2); }
	.scope-option { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2); border: 1px solid var(--line); border-radius: var(--radius-sm); font-family: var(--font-mono); font-size: 0.72rem; }
	.secret-panel { display: grid; gap: var(--space-2); margin: var(--space-4) 0; padding: var(--space-3); border: 1px solid var(--line); border-radius: var(--radius-md); background: var(--surface-subtle); }
	.secret-panel p, .empty-hint { margin: 0; color: var(--ink-muted); font-size: 0.78rem; }
	.secret-row { display: flex; align-items: center; gap: var(--space-3); }
	.secret-row code { flex: 1; overflow-wrap: anywhere; padding: var(--space-2); background: var(--surface); font-size: 0.72rem; }
	.scope-list { max-width: 28rem; color: var(--ink-muted); font-family: var(--font-mono); font-size: 0.68rem; }

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

	.update-error {
		color: var(--warning, #b7791f);
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
		.update-metrics,
		.scope-grid {
			grid-template-columns: 1fr;
		}
		.secret-row { align-items: stretch; flex-direction: column; }
	}
</style>
