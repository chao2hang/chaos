<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		Boxes,
		LayoutDashboard,
		LogOut,
		Menu,
		Network,
		RadioTower,
		Route,
		Server,
		Workflow,
		X
	} from '@lucide/svelte';
	import { setToken } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import AppLogo from '$lib/components/ui/AppLogo.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import LocaleSwitcher from '$lib/LocaleSwitcher.svelte';

	let { children } = $props();
	let ready = $state(false);
	let navOpen = $state(false);

	onMount(() => {
		const token = typeof localStorage !== 'undefined' ? localStorage.getItem('token') : null;
		if (!token) {
			void goto('/login');
			return;
		}
		ready = true;
	});

	function logout() {
		if (
			typeof document !== 'undefined' &&
			document.documentElement.dataset.chaosUnsaved === 'true'
		) {
			if (!window.confirm(t('common.discardDescription'))) return;
			sessionStorage.setItem('chaos_allow_dirty_navigation', '1');
		}
		setToken(null);
		void goto('/login');
	}

	function closeNav() {
		navOpen = false;
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') closeNav();
	}

	const path = $derived(page.url.pathname);

	const navGroups = $derived([
		{
			label: t('nav.section.overview'),
			links: [
				{ href: '/dashboard', label: t('nav.dashboard'), icon: LayoutDashboard },
				{ href: '/orchestrate', label: t('nav.orchestrate'), icon: Workflow }
			]
		},
		{
			label: t('nav.section.resources'),
			links: [
				{ href: '/nodes', label: t('nav.nodes'), icon: Server },
				{ href: '/subscriptions', label: t('nav.subscriptions'), icon: RadioTower }
			]
		},
		{
			label: t('nav.section.policies'),
			links: [
				{ href: '/groups', label: t('nav.groups'), icon: Boxes },
				{ href: '/routing', label: t('nav.routing'), icon: Route },
				{ href: '/dns', label: t('nav.dns'), icon: Network }
			]
		}
	]);
</script>

<svelte:window onkeydown={onKeydown} />

{#if ready}
	<div class="shell" class:nav-open={navOpen}>
		<aside class="sidebar" aria-label={t('nav.primary')}>
			<div class="brand-row">
				<a class="brand" href="/dashboard" onclick={closeNav} aria-label="chaos">
					<AppLogo />
				</a>
				<Button
					class="close-nav"
					variant="ghost"
					size="icon"
					icon={X}
					aria-label={t('nav.close')}
					title={t('nav.close')}
					onclick={closeNav}
				/>
			</div>

			<nav class="side-nav">
				{#each navGroups as group (group.label)}
					<div class="nav-group">
						<p>{group.label}</p>
						{#each group.links as link (link.href)}
							{@const Icon = link.icon}
							<a href={link.href} class:active={path.startsWith(link.href)} onclick={closeNav}>
								<Icon size={16} strokeWidth={1.7} aria-hidden="true" />
								<span>{link.label}</span>
							</a>
						{/each}
					</div>
				{/each}
			</nav>

			<footer class="side-footer">
				<LocaleSwitcher />
				<Button variant="ghost" icon={LogOut} full onclick={logout}>{t('nav.logout')}</Button>
			</footer>
		</aside>

		{#if navOpen}
			<button class="scrim" type="button" aria-label={t('nav.close')} onclick={closeNav}></button>
		{/if}

		<div class="main-column">
			<header class="mobile-bar">
				<Button
					variant="ghost"
					size="icon"
					icon={Menu}
					aria-label={t('nav.menu')}
					title={t('nav.menu')}
					onclick={() => (navOpen = true)}
				/>
				<a href="/dashboard" aria-label="chaos"><AppLogo /></a>
				<span class="bar-spacer" aria-hidden="true"></span>
			</header>

			<main class="content">
				{@render children()}
			</main>
		</div>
	</div>
{:else}
	<main class="boot"><LoadingState label={t('nav.checkingSession')} /></main>
{/if}

<style>
	.shell {
		min-height: 100vh;
		display: grid;
		grid-template-columns: var(--sidebar-width) minmax(0, 1fr);
	}

	.sidebar {
		position: sticky;
		top: 0;
		z-index: 30;
		display: flex;
		height: 100vh;
		min-height: 0;
		flex-direction: column;
		padding: var(--space-4) var(--space-3);
		border-right: 1px solid var(--line);
		background: var(--surface);
	}

	.brand-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		min-height: 2.75rem;
		padding: 0 var(--space-2) var(--space-4);
	}

	.brand {
		display: inline-flex;
		padding: 0.4rem 0;
		text-decoration: none;
	}

	:global(.close-nav) {
		display: none;
	}

	.side-nav {
		display: flex;
		min-height: 0;
		flex: 1;
		flex-direction: column;
		gap: var(--space-5);
		overflow-y: auto;
	}

	.nav-group {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.nav-group > p {
		margin: 0 0 var(--space-1);
		padding: 0 var(--space-3);
		color: var(--ink-faint);
		font-size: 0.65rem;
		font-weight: 650;
		text-transform: uppercase;
	}

	.nav-group a {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: 2.25rem;
		padding: 0.48rem var(--space-3);
		border: 1px solid transparent;
		border-radius: var(--radius-md);
		color: var(--ink-muted);
		font-size: 0.82rem;
		font-weight: 590;
		text-decoration: none;
		transition:
			background 120ms ease,
			color 120ms ease,
			border-color 120ms ease;
	}

	.nav-group a:hover {
		background: var(--surface-subtle);
		color: var(--ink);
	}

	.nav-group a.active {
		border-color: var(--surface-inverse);
		background: var(--surface-inverse);
		color: var(--ink-inverse);
	}

	.side-footer {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-top: var(--space-3);
		border-top: 1px solid var(--line);
	}

	.side-footer :global(.button) {
		justify-content: flex-start;
	}

	.main-column {
		min-width: 0;
	}

	.mobile-bar {
		display: none;
	}

	.content {
		width: 100%;
		max-width: var(--content-width);
		margin: 0 auto;
		padding: var(--space-8) var(--space-8) var(--space-12);
	}

	.scrim {
		display: none;
	}

	.boot {
		min-height: 100vh;
	}

	@media (max-width: 900px) {
		.shell {
			display: block;
		}

		.sidebar {
			position: fixed;
			left: 0;
			top: 0;
			width: min(var(--sidebar-width), 88vw);
			transform: translateX(-102%);
			transition: transform 180ms ease;
			box-shadow: var(--shadow-float);
		}

		.nav-open .sidebar {
			transform: translateX(0);
		}

		:global(.close-nav) {
			display: inline-flex;
		}

		.mobile-bar {
			position: sticky;
			top: 0;
			z-index: 20;
			display: grid;
			grid-template-columns: 2.25rem 1fr 2.25rem;
			align-items: center;
			height: var(--topbar-height);
			padding: 0 var(--space-3);
			border-bottom: 1px solid var(--line);
			background: rgba(255, 255, 255, 0.96);
			backdrop-filter: blur(8px);
		}

		.mobile-bar > a {
			justify-self: center;
			text-decoration: none;
		}

		.scrim {
			position: fixed;
			inset: 0;
			z-index: 25;
			display: block;
			padding: 0;
			border: 0;
			background: var(--overlay);
		}

		.content {
			padding: var(--space-6) var(--space-4) var(--space-10);
		}
	}
</style>
