<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, setupAdmin, setToken, ApiClientError } from '$lib/api';

	let username = $state('admin');
	let password = $state('');
	let confirm = $state('');
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
				e instanceof ApiClientError
					? e.message
					: 'Cannot reach API. Start chaos-api on :2030.';
			ready = true;
		}
	});

	async function onSubmit(e: Event) {
		e.preventDefault();
		error = '';
		if (password.length < 8) {
			error = 'Password must be at least 8 characters.';
			return;
		}
		if (password !== confirm) {
			error = 'Passwords do not match.';
			return;
		}
		busy = true;
		try {
			const res = await setupAdmin(username.trim(), password);
			setToken(res.token);
			// Token stored; login page confirms session until dashboard exists.
			await goto('/login');
		} catch (err) {
			if (err instanceof ApiClientError && err.code === 'already_initialized') {
				await goto('/login');
				return;
			}
			error = err instanceof ApiClientError ? err.message : 'Setup failed';
		} finally {
			busy = false;
		}
	}
</script>

<main class="shell">
	<h1>Initial setup</h1>
	<p class="muted">Create the first admin account for chaos.</p>

	{#if !ready}
		<p>Checking status…</p>
	{:else}
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
					autocomplete="new-password"
					required
					minlength="8"
					disabled={busy}
				/>
			</label>
			<label>
				Confirm password
				<input
					type="password"
					bind:value={confirm}
					autocomplete="new-password"
					required
					minlength="8"
					disabled={busy}
				/>
			</label>
			{#if error}
				<p class="error" role="alert">{error}</p>
			{/if}
			<button type="submit" disabled={busy}>{busy ? 'Creating…' : 'Create admin'}</button>
		</form>
		<p class="muted"><a href="/login">Already initialized? Log in</a></p>
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
