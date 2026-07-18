<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { setToken } from '$lib/api';
	import { t } from '$lib/i18n';
	import LocaleSwitcher from '$lib/LocaleSwitcher.svelte';

	let { children } = $props();
	let ready = $state(false);

	onMount(() => {
		const token = typeof localStorage !== 'undefined' ? localStorage.getItem('token') : null;
		if (!token) {
			goto('/login');
			return;
		}
		ready = true;
	});

	function logout() {
		setToken(null);
		goto('/login');
	}

	const path = $derived(page.url.pathname);
</script>

{#if ready}
	<div class="app">
		<header class="top">
			<a class="brand" href="/dashboard">
				<span class="brand-mark" aria-hidden="true"></span>
				<span class="brand-text">chaos</span>
			</a>
			<nav>
				<a href="/dashboard" class:active={path.startsWith('/dashboard')}>{t('nav.dashboard')}</a>
				<a href="/nodes" class:active={path.startsWith('/nodes')}>{t('nav.nodes')}</a>
				<a href="/subscriptions" class:active={path.startsWith('/subscriptions')}
					>{t('nav.subscriptions')}</a
				>
				<a href="/groups" class:active={path.startsWith('/groups')}>{t('nav.groups')}</a>
				<a href="/routing" class:active={path.startsWith('/routing')}>{t('nav.routing')}</a>
				<a href="/dns" class:active={path.startsWith('/dns')}>{t('nav.dns')}</a>
			</nav>
			<div class="top-end">
				<LocaleSwitcher />
				<button type="button" class="logout" onclick={logout}>{t('nav.logout')}</button>
			</div>
		</header>
		<main class="content">
			{@render children()}
		</main>
	</div>
{:else}
	<main class="boot">
		<span class="eyebrow">{t('nav.checkingSession')}</span>
	</main>
{/if}

<style>
	.app {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
	}

	.top {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.65rem 1.25rem;
		background: rgba(15, 20, 28, 0.92);
		border-bottom: 1px solid var(--line);
		backdrop-filter: blur(12px);
		position: sticky;
		top: 0;
		z-index: 40;
	}

	.brand {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		text-decoration: none;
		color: var(--ink);
		flex-shrink: 0;
	}
	.brand:hover {
		text-decoration: none;
		color: var(--signal);
	}
	.brand-mark {
		width: 10px;
		height: 10px;
		border-radius: 2px;
		background: var(--signal);
		box-shadow: 0 0 12px var(--signal);
		transform: rotate(45deg);
	}
	.brand-text {
		font-weight: 700;
		font-size: 1.05rem;
		letter-spacing: -0.03em;
		font-family: var(--font-mono);
	}

	nav {
		display: flex;
		flex-wrap: wrap;
		gap: 0.15rem;
		flex: 1;
		min-width: 0;
	}
	nav a {
		color: var(--ink-muted);
		text-decoration: none;
		font-size: 0.88rem;
		font-weight: 500;
		padding: 0.35rem 0.55rem;
		border-radius: var(--r-sm);
		border: 1px solid transparent;
		transition: color 0.12s ease, background 0.12s ease, border-color 0.12s ease;
	}
	nav a:hover {
		color: var(--ink);
		background: var(--bg-hover);
		text-decoration: none;
	}
	nav a.active {
		color: var(--signal);
		background: var(--signal-soft);
		border-color: rgba(45, 212, 191, 0.25);
		font-weight: 600;
	}

	.top-end {
		display: flex;
		align-items: center;
		gap: 0.65rem;
		flex-shrink: 0;
	}

	.logout {
		font-size: 0.85rem;
		padding: 0.35rem 0.7rem;
		background: transparent;
		border-color: var(--line);
		color: var(--ink-muted);
	}
	.logout:hover:not(:disabled) {
		color: var(--ink);
		border-color: var(--line-strong);
		background: var(--bg-hover);
	}

	.content {
		max-width: 64rem;
		width: 100%;
		margin: 0 auto;
		padding: 1.75rem 1.25rem 3.5rem;
		flex: 1;
	}

	.boot {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
	}

	@media (max-width: 720px) {
		.top {
			flex-wrap: wrap;
		}
		nav {
			order: 3;
			width: 100%;
			padding-top: 0.25rem;
		}
	}
</style>
