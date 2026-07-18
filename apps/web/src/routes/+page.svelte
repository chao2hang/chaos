<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { authStatus, ApiClientError } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

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

<main class="boot">
	<div class="panel boot-card">
		<span class="eyebrow">chaos</span>
		<p class="mono-msg">{message || t('home.loading')}</p>
	</div>
</main>

<style>
	.boot {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem 1rem;
	}
	.boot-card {
		max-width: 28rem;
		width: 100%;
	}
	.mono-msg {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 0.9rem;
		color: var(--ink-muted);
		line-height: 1.55;
	}
</style>
