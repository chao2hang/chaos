<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, ApiClientError } from '$lib/api';
	import { t } from '$lib/i18n';

	let message = $state('');

	onMount(async () => {
		message = t('home.loading');
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
			const err = e instanceof ApiClientError ? e.message : t('error.request_failed');
			message = t('home.apiUnreachable', { error: err });
		}
	});
</script>

<main class="shell">
	<p>{message || t('home.loading')}</p>
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
