<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { setToken } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import LocaleSwitcher from '$lib/LocaleSwitcher.svelte';

	let { children } = $props();
	let ready = $state(false);
	let navOpen = $state(false);

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

	const links = $derived([
		{ href: '/dashboard', label: t('nav.dashboard'), match: '/dashboard' },
		{ href: '/orchestrate', label: t('nav.orchestrate'), match: '/orchestrate' },
		{ href: '/nodes', label: t('nav.nodes'), match: '/nodes' },
		{ href: '/subscriptions', label: t('nav.subscriptions'), match: '/subscriptions' },
		{ href: '/groups', label: t('nav.groups'), match: '/groups' },
		{ href: '/routing', label: t('nav.routing'), match: '/routing' },
		{ href: '/dns', label: t('nav.dns'), match: '/dns' }
	]);

	function closeNav() {
		navOpen = false;
	}
</script>

{#if ready}
	<div class="shell" class:nav-open={navOpen}>
		<aside class="sidebar" aria-label="Primary">
			<a class="brand" href="/dashboard" onclick={closeNav}>
				<span class="brand-mark" aria-hidden="true"></span>
				<span class="brand-text">chaos</span>
				<span class="brand-tag">dae control</span>
			</a>

			<nav class="side-nav">
				<p class="nav-section">Console</p>
				{#each links as link (link.href)}
					<a
						href={link.href}
						class:active={path.startsWith(link.match)}
						onclick={closeNav}
					>
						<span class="nav-dot" aria-hidden="true"></span>
						{link.label}
					</a>
				{/each}
			</nav>

			<div class="side-foot">
				<LocaleSwitcher />
				<button type="button" class="logout" onclick={logout}>{t('nav.logout')}</button>
			</div>
		</aside>

		{#if navOpen}
			<button type="button" class="scrim" aria-label="Close menu" onclick={closeNav}></button>
		{/if}

		<div class="main-col">
			<header class="topbar">
				<button type="button" class="menu-btn" onclick={() => (navOpen = !navOpen)}>
					Menu
				</button>
				<div class="topbar-meta">
					<span class="live-pill">
						<span class="live-dot" aria-hidden="true"></span>
						control plane
					</span>
				</div>
			</header>
			<main class="content">
				{@render children()}
			</main>
		</div>
	</div>
{:else}
	<main class="boot">
		<span class="eyebrow">{t('nav.checkingSession')}</span>
	</main>
{/if}

<style>
	.shell {
		min-height: 100vh;
		display: grid;
		grid-template-columns: var(--sidebar-w) 1fr;
	}

	.sidebar {
		position: sticky;
		top: 0;
		height: 100vh;
		display: flex;
		flex-direction: column;
		gap: var(--space-md);
		padding: var(--space-md) var(--space-sm);
		background: var(--bg-sidebar);
		border-right: 1px solid var(--line);
		z-index: 30;
	}

	.brand {
		display: grid;
		grid-template-columns: auto 1fr;
		grid-template-rows: auto auto;
		column-gap: 0.65rem;
		row-gap: 0.1rem;
		align-items: center;
		padding: 0.5rem 0.65rem;
		text-decoration: none;
		color: var(--ink);
		border-radius: var(--r-md);
	}
	.brand:hover {
		text-decoration: none;
		background: var(--bg-hover);
		color: var(--ink);
	}
	.brand-mark {
		grid-row: 1 / span 2;
		width: 12px;
		height: 12px;
		border-radius: 3px;
		background: var(--signal);
		box-shadow: 0 0 14px var(--signal);
	}
	.brand-text {
		font-family: var(--font-mono);
		font-weight: 700;
		font-size: 1.05rem;
		letter-spacing: -0.03em;
	}
	.brand-tag {
		font-family: var(--font-mono);
		font-size: 0.65rem;
		color: var(--ink-dim);
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	.side-nav {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 0 0.25rem;
	}
	.nav-section {
		margin: 0.5rem 0.55rem 0.35rem;
		font-family: var(--font-mono);
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--ink-dim);
	}
	.side-nav a {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		padding: 0.55rem 0.7rem;
		border-radius: var(--r-md);
		color: var(--ink-muted);
		text-decoration: none;
		font-size: 0.9rem;
		font-weight: 500;
		border: 1px solid transparent;
		cursor: pointer;
		transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
	}
	.side-nav a:hover {
		background: var(--bg-hover);
		color: var(--ink);
		text-decoration: none;
	}
	.side-nav a.active {
		background: var(--signal-soft);
		color: var(--signal);
		border-color: rgba(34, 197, 94, 0.28);
		font-weight: 600;
	}
	.nav-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: currentColor;
		opacity: 0.45;
		flex-shrink: 0;
	}
	.side-nav a.active .nav-dot {
		opacity: 1;
		box-shadow: 0 0 8px var(--signal);
	}

	.side-foot {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.5rem;
		border-top: 1px solid var(--line);
	}
	.logout {
		width: 100%;
		justify-content: center;
		background: transparent;
		border-color: var(--line);
		color: var(--ink-muted);
		font-weight: 500;
	}
	.logout:hover:not(:disabled) {
		color: var(--ink);
		border-color: var(--line-strong);
	}

	.main-col {
		min-width: 0;
		display: flex;
		flex-direction: column;
		min-height: 100vh;
	}

	.topbar {
		display: none;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		height: var(--header-h);
		padding: 0 var(--space-md);
		border-bottom: 1px solid var(--line);
		background: rgba(2, 6, 23, 0.85);
		backdrop-filter: blur(10px);
		position: sticky;
		top: 0;
		z-index: 20;
	}
	.menu-btn {
		font-size: 0.85rem;
	}
	.live-pill {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--ink-dim);
	}
	.live-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--signal);
		box-shadow: 0 0 8px var(--signal);
	}

	.content {
		flex: 1;
		width: 100%;
		max-width: 72rem;
		margin: 0 auto;
		padding: var(--space-lg) var(--space-lg) var(--space-2xl);
	}

	.scrim {
		display: none;
	}

	.boot {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
	}

	@media (max-width: 900px) {
		.shell {
			grid-template-columns: 1fr;
		}
		.topbar {
			display: flex;
		}
		.sidebar {
			position: fixed;
			left: 0;
			top: 0;
			width: min(var(--sidebar-w), 86vw);
			transform: translateX(-105%);
			transition: transform 0.2s ease;
			box-shadow: var(--shadow-lg);
		}
		.shell.nav-open .sidebar {
			transform: translateX(0);
		}
		.scrim {
			display: block;
			position: fixed;
			inset: 0;
			background: rgba(0, 0, 0, 0.55);
			border: none;
			padding: 0;
			z-index: 25;
			cursor: pointer;
		}
		.content {
			padding: var(--space-md);
		}
	}
</style>
