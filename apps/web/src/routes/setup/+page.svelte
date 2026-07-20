<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { RefreshCw, UserRoundCheck } from '@lucide/svelte';
	import { authStatus, clearSessionRedirect, setupAdmin, setToken, ApiClientError } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import AuthShell from '$lib/components/ui/AuthShell.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PasswordInput from '$lib/components/ui/PasswordInput.svelte';

	let username = $state('admin');
	let password = $state('');
	let confirmPassword = $state('');
	let error = $state('');
	let busy = $state(false);
	let ready = $state(false);
	let apiOk = $state(false);

	async function checkStatus() {
		ready = false;
		error = '';
		try {
			const status = await authStatus();
			apiOk = true;
			if (status.initialized) {
				await goto('/login');
				return;
			}
		} catch (cause) {
			apiOk = false;
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.setup.apiDown');
		} finally {
			ready = true;
		}
	}

	onMount(() => {
		clearSessionRedirect();
		void checkStatus();
	});

	async function onSubmit(event: SubmitEvent) {
		event.preventDefault();
		error = '';
		if (password.length < 8) {
			error = t('auth.setup.passwordTooShort');
			return;
		}
		if (password !== confirmPassword) {
			error = t('auth.setup.passwordMismatch');
			return;
		}

		busy = true;
		try {
			const response = await setupAdmin(username.trim(), password);
			setToken(response.token);
			await goto('/dashboard');
		} catch (cause) {
			if (cause instanceof ApiClientError && cause.code === 'already_initialized') {
				await goto('/login');
				return;
			}
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.setup.failed');
		} finally {
			busy = false;
		}
	}

	const confirmError = $derived(
		confirmPassword && password !== confirmPassword ? t('auth.setup.passwordMismatch') : ''
	);
</script>

<AuthShell title={t('auth.setup.title')} description={t('auth.setup.subtitle')}>
	{#if !ready}
		<LoadingState label={t('auth.setup.checking')} />
	{:else}
		<div class="auth-stack">
			{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}

			{#if !apiOk}
				<Button full icon={RefreshCw} onclick={checkStatus}>{t('common.retry')}</Button>
			{:else}
				<form onsubmit={onSubmit}>
					<Field label={t('auth.setup.username')} forId="username">
						<input
							id="username"
							type="text"
							bind:value={username}
							autocomplete="username"
							required
							disabled={busy}
						/>
					</Field>
					<Field
						label={t('auth.setup.password')}
						forId="password"
						hint={t('auth.setup.passwordHint')}
					>
						<PasswordInput
							id="password"
							bind:value={password}
							autocomplete="new-password"
							label={t('auth.setup.password')}
							disabled={busy}
						/>
					</Field>
					<Field
						label={t('auth.setup.confirmPassword')}
						forId="confirm-password"
						error={confirmError}
					>
						<PasswordInput
							id="confirm-password"
							bind:value={confirmPassword}
							autocomplete="new-password"
							label={t('auth.setup.confirmPassword')}
							disabled={busy}
						/>
					</Field>
					<Button
						type="submit"
						variant="primary"
						size="lg"
						icon={UserRoundCheck}
						loading={busy}
						disabled={!!confirmError}
						full
					>
						{busy ? t('auth.setup.submitting') : t('auth.setup.submit')}
					</Button>
				</form>
			{/if}

			<p class="alternate"><a href="/login">{t('auth.setup.alreadyInitialized')}</a></p>
		</div>
	{/if}
</AuthShell>

<style>
	.auth-stack,
	form {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.alternate {
		margin: var(--space-2) 0 0;
		color: var(--ink-muted);
		font-size: 0.78rem;
		text-align: center;
	}
</style>
