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

<main class="shell">
	<div class="toolbar">
		<LocaleSwitcher />
	</div>
	<h1>{t('auth.setup.title')}</h1>
	<p class="muted">{t('auth.setup.subtitle')}</p>

	{#if !ready}
		<p>{t('auth.setup.checking')}</p>
	{:else}
		<form onsubmit={onSubmit}>
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
			<button type="submit" disabled={busy}
				>{busy ? t('auth.setup.submitting') : t('auth.setup.submit')}</button
			>
		</form>
		<p class="muted"><a href="/login">{t('auth.setup.alreadyInitialized')}</a></p>
	{/if}
</main>

<style>
	.shell {
		font-family: system-ui, sans-serif;
		max-width: 24rem;
		margin: 4rem auto;
		padding: 0 1rem;
		color: #1a1a1a;
	}
	.toolbar {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 0.75rem;
	}
	.muted {
		color: #555;
		font-size: 0.95rem;
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-top: 1.25rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		font-size: 0.9rem;
		font-weight: 600;
	}
	input {
		font: inherit;
		font-weight: 400;
		padding: 0.5rem 0.6rem;
		border: 1px solid #ccc;
		border-radius: 6px;
	}
	button {
		font: inherit;
		padding: 0.55rem 0.85rem;
		border: none;
		border-radius: 6px;
		background: #1a56db;
		color: #fff;
		cursor: pointer;
		margin-top: 0.25rem;
	}
	button:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}
	.error {
		color: #b42318;
		margin: 0;
		font-size: 0.9rem;
	}
	a {
		color: #1a56db;
	}
</style>
