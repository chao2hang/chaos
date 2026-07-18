<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, setupAdmin, setToken, ApiClientError } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n';
	import LocaleSwitcher from '$lib/LocaleSwitcher.svelte';

	let username = $state('admin');
	let password = $state('');
	let confirmPw = $state('');
	let error = $state('');
	let busy = $state(false);
	let ready = $state(false);

	onMount(async () => {
		try {
			const status = await authStatus();
			if (status.initialized) {
				await goto('/login');
				return;
			}
			ready = true;
		} catch (e) {
			error =
				e instanceof ApiClientError ? apiErrorText(e) : t('auth.setup.apiDown');
			ready = true;
		}
	});

	async function onSubmit(e: Event) {
		e.preventDefault();
		error = '';
		if (password.length < 8) {
			error = t('auth.setup.passwordTooShort');
			return;
		}
		if (password !== confirmPw) {
			error = t('auth.setup.passwordMismatch');
			return;
		}
		busy = true;
		try {
			const res = await setupAdmin(username.trim(), password);
			setToken(res.token);
			await goto('/dashboard');
		} catch (err) {
			if (err instanceof ApiClientError && err.code === 'already_initialized') {
				await goto('/login');
				return;
			}
			error = err instanceof ApiClientError ? apiErrorText(err) : t('auth.setup.failed');
		} finally {
			busy = false;
		}
	}
</script>

<main class="auth">
	<div class="auth-card panel">
		<div class="auth-top">
			<span class="eyebrow">chaos · first run</span>
			<LocaleSwitcher />
		</div>
		<h1 class="page-title">{t('auth.setup.title')}</h1>
		<p class="page-sub">{t('auth.setup.subtitle')}</p>

		{#if !ready}
			<p class="muted">{t('auth.setup.checking')}</p>
		{:else}
			<form class="form-stack" onsubmit={onSubmit}>
				<label>
					{t('auth.setup.username')}
					<input bind:value={username} autocomplete="username" required disabled={busy} />
				</label>
				<label>
					{t('auth.setup.password')}
					<input
						type="password"
						bind:value={password}
						autocomplete="new-password"
						required
						minlength="8"
						disabled={busy}
					/>
				</label>
				<label>
					{t('auth.setup.confirmPassword')}
					<input
						type="password"
						bind:value={confirmPw}
						autocomplete="new-password"
						required
						minlength="8"
						disabled={busy}
					/>
				</label>
				{#if error}
					<p class="error" role="alert">{error}</p>
				{/if}
				<button type="submit" class="primary" disabled={busy}
					>{busy ? t('auth.setup.submitting') : t('auth.setup.submit')}</button
				>
			</form>
			<p class="footer muted"><a href="/login">{t('auth.setup.alreadyInitialized')}</a></p>
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
