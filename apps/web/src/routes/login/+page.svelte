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
	import PasswordInput from '$lib/components/ui/PasswordInput.svelte';
	import { toast } from '$lib/toast.svelte';

	let username = $state('admin');
	let password = $state('');
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
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.login.apiDown')
			});
		} finally {
			ready = true;
		}
	}

	onMount(() => {
		clearSessionRedirect();
		void checkApi();
	});

	// Toast creation reads and writes the shared toast state. Doing that from a
	// reactive effect makes the effect subscribe to the state it mutates and can
	// trigger Svelte's effect_update_depth_exceeded loop on this page.
	onMount(() => {
		if (expired) {
			toast.info({ id: 'auth-expired', title: t('auth.login.expired'), duration: 0 });
		} else {
			toast.dismiss('auth-expired');
		}
	});

	async function onSubmit(event: SubmitEvent) {
		event.preventDefault();
		if (!apiOk) return;
		busy = true;
		try {
			const response = await login(username.trim(), password);
			setToken(response.token);
			toast.dismiss('auth-expired');
			await goto(loginTarget(), { replaceState: true });
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('auth.login.failed')
			});
		} finally {
			busy = false;
		}
	}
</script>

<AuthShell title={t('auth.login.title')} description={t('auth.login.subtitle')}>
	{#if !ready}
		<LoadingState label={t('auth.login.checkingApi')} />
	{:else}
		<div class="auth-flow">
			{#if !apiOk}
				<Button full icon={RefreshCw} onclick={checkApi}>{t('common.retry')}</Button>
			{:else}
				<form class="form-stack" onsubmit={onSubmit}>
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

			<p class="auth-alternate"><a href="/setup">{t('auth.login.firstRun')}</a></p>
		</div>
	{/if}
</AuthShell>
