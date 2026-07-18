<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, login, setToken, health, ApiClientError } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n';
	import LocaleSwitcher from '$lib/LocaleSwitcher.svelte';

	let username = $state('admin');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);
	let ready = $state(false);
	let apiOk = $state(false);

	onMount(async () => {
		try {
			await health();
			apiOk = true;
			const status = await authStatus();
			if (!status.initialized) {
				await goto('/setup');
				return;
			}
			ready = true;
		} catch (e) {
			error =
				e instanceof ApiClientError ? apiErrorText(e) : t('auth.login.apiDown');
			ready = true;
		}
	});

	async function onSubmit(e: Event) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const res = await login(username.trim(), password);
			setToken(res.token);
			await goto('/dashboard');
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('auth.login.failed');
		} finally {
			busy = false;
		}
	}
</script>

<main class="auth">
	<div class="auth-card panel">
		<div class="auth-top">
			<span class="eyebrow">chaos · control plane</span>
			<LocaleSwitcher />
		</div>
		<h1 class="page-title">{t('auth.login.title')}</h1>
		<p class="page-sub">{t('auth.login.subtitle')}</p>

		{#if !ready}
			<p class="muted">{t('auth.login.checkingApi')}</p>
		{:else}
			{#if !apiOk && error}
				<p class="error" role="alert">{error}</p>
			{/if}
			<form class="form-stack" onsubmit={onSubmit}>
				<label>
					{t('auth.login.username')}
					<input bind:value={username} autocomplete="username" required disabled={busy} />
				</label>
				<label>
					{t('auth.login.password')}
					<input
						type="password"
						bind:value={password}
						autocomplete="current-password"
						required
						minlength="8"
						disabled={busy}
					/>
				</label>
				{#if error && apiOk}
					<p class="error" role="alert">{error}</p>
				{/if}
				<button type="submit" class="primary" disabled={busy || !apiOk}
					>{busy ? t('auth.login.submitting') : t('auth.login.submit')}</button
				>
			</form>
			<p class="footer muted"><a href="/setup">{t('auth.login.firstRun')}</a></p>
		{/if}
	</div>
</main>

<style>
	.auth {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem 1rem;
	}
	.auth-card {
		width: 100%;
		max-width: 24rem;
	}
	.auth-top {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		margin-bottom: 0.25rem;
	}
	.footer {
		margin: 1.25rem 0 0;
		font-size: 0.9rem;
	}
	form button {
		margin-top: 0.35rem;
		width: 100%;
		padding: 0.65rem 0.9rem;
	}
</style>
