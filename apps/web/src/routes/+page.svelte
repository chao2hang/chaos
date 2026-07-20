<script lang="ts">
	import { onMount } from 'svelte';
	import { RefreshCw } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { authStatus, ApiClientError } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import AppLogo from '$lib/components/ui/AppLogo.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Button from '$lib/components/ui/Button.svelte';

	let message = $state('');
	let busy = $state(false);

	async function checkStatus() {
		busy = true;
		message = '';
		try {
			const status = await authStatus();
			if (!status.initialized) {
				await goto('/setup');
				return;
			}
			const token = typeof localStorage !== 'undefined' ? localStorage.getItem('token') : null;
			await goto(token ? '/dashboard' : '/login');
		} catch (cause) {
			const detail = cause instanceof ApiClientError ? cause.message : t('error.request_failed');
			message = t('home.apiUnreachable', { error: detail });
		} finally {
			busy = false;
		}
	}

	onMount(() => {
		void checkStatus();
	});
</script>

<main class="boot">
	<div class="boot-content">
		<AppLogo />
		{#if message}
			<Notice tone="error" {message} />
			<Button icon={RefreshCw} loading={busy} onclick={() => void checkStatus()}>{t('common.retry')}</Button>
		{:else}
			<LoadingState label={t('home.loading')} />
		{/if}
	</div>
</main>

<style>
	.boot {
		display: grid;
		min-height: 100vh;
		place-items: center;
		padding: var(--space-4);
	}

	.boot-content {
		display: flex;
		width: min(28rem, 100%);
		flex-direction: column;
		align-items: center;
		gap: var(--space-6);
	}

	.boot-content :global(.notice) {
		width: 100%;
	}
</style>
