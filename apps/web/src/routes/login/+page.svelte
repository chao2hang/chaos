<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, login, setToken, health, ApiClientError } from '$lib/api';

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
				e instanceof ApiClientError
					? e.message
					: 'Cannot reach API. Start chaos-api on :2030.';
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
			error = err instanceof ApiClientError ? err.message : 'Login failed';
		} finally {
			busy = false;
		}
	}
</script>

<main class="shell">
	<h1>Log in</h1>
	<p class="muted">Sign in to the chaos control plane.</p>

	{#if !ready}
		<p>Checking API…</p>
	{:else}
		{#if !apiOk && error}
			<p class="error" role="alert">{error}</p>
		{/if}
		<form onsubmit={onSubmit}>
			<label>
				Username
				<input bind:value={username} autocomplete="username" required disabled={busy} />
			</label>
			<label>
				Password
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
			<button type="submit" disabled={busy || !apiOk}>{busy ? 'Signing in…' : 'Sign in'}</button>
		</form>
		<p class="muted"><a href="/setup">First run? Create admin</a></p>
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
