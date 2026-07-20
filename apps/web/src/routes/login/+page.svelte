<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { LogIn, RefreshCw } from '@lucide/svelte';
	import { authStatus, clearSessionRedirect, login, setToken, health, ApiClientError } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import AuthShell from '$lib/components/ui/AuthShell.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PasswordInput from '$lib/components/ui/PasswordInput.svelte';

	let username = $state('admin');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);
	let ready = $state(false);
	let apiOk = $state(false);
	const expired = $derived(page.url.searchParams.get('expired') === '1');

	function loginTarget(): string {
		const candidate = page.url.searchParams.get('next');
		if (!candidate || !candidate.startsWith('/') || candidate.startsWith('//')) return '/dashboard';
		try {
			const target = new URL(candidate, page.url.origin);
			if (target.origin !== page.url.origin || ['/login', '/setup'].includes(target.pathname)) {
				return '/dashboard';
			}
			return `${target.pathname}${target.search}${target.hash}`;
		} catch {
			return '/dashboard';
		}
	}

	async function checkApi() {
		ready = false;
		error = '';
		try {
			await health();
			apiOk = true;
			const status = await authStatus();
			if (!status.initialized) {
				await goto('/setup');
				return;
			}
		} catch (cause) {
			apiOk = false;
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.login.apiDown');
		} finally {
			ready = true;
		}
	}

	onMount(() => {
		clearSessionRedirect();
		void checkApi();
	});

	async function onSubmit(event: SubmitEvent) {
		event.preventDefault();
		if (!apiOk) return;
		error = '';
		busy = true;
		try {
			const response = await login(username.trim(), password);
			setToken(response.token);
			await goto(loginTarget(), { replaceState: true });
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.login.failed');
		} finally {
			busy = false;
		}
	}
</script>

<AuthShell title={t('auth.login.title')} description={t('auth.login.subtitle')}>
	{#if !ready}
		<LoadingState label={t('auth.login.checkingApi')} />
	{:else}
		<div class="auth-stack">
			{#if expired}<Notice message={t('auth.login.expired')} />{/if}
			{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}

			{#if !apiOk}
				<Button full icon={RefreshCw} onclick={checkApi}>{t('common.retry')}</Button>
			{:else}
				<form onsubmit={onSubmit}>
					<Field label={t('auth.login.username')} forId="username">
						<input
							id="username"
							type="text"
							bind:value={username}
							autocomplete="username"
							required
							disabled={busy}
						/>
					</Field>
					<Field label={t('auth.login.password')} forId="password">
						<PasswordInput
							id="password"
							bind:value={password}
							autocomplete="current-password"
							label={t('auth.login.password')}
							disabled={busy}
						/>
					</Field>
					<Button type="submit" variant="primary" size="lg" icon={LogIn} loading={busy} full>
						{busy ? t('auth.login.submitting') : t('auth.login.submit')}
					</Button>
				</form>
			{/if}

			<p class="alternate"><a href="/setup">{t('auth.login.firstRun')}</a></p>
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
