<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, ApiClientError } from '$lib/api';

	let message = $state('Loading…');

	onMount(async () => {
		try {
			const status = await authStatus();
			if (!status.initialized) {
				await goto('/setup');
				return;
			}
			const token = typeof localStorage !== 'undefined' ? localStorage.getItem('token') : null;
			if (!token) {
				await goto('/login');
				return;
			}
			await goto('/dashboard');
		} catch (e) {
			const err = e instanceof ApiClientError ? e.message : 'API unreachable';
			message = `Cannot reach API: ${err}. Start chaos-api on :2030 (pnpm dev:api).`;
		}
	});
</script>

<main class="shell">
	<p>{message}</p>
</main>

<style>
	.shell {
		font-family: system-ui, sans-serif;
		max-width: 32rem;
		margin: 4rem auto;
		padding: 0 1rem;
		color: #1a1a1a;
	}
</style>
