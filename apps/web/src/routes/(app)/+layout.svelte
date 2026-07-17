<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { setToken } from '$lib/api';

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
			<a class="brand" href="/dashboard">chaos</a>
			<nav>
				<a href="/dashboard" class:active={path.startsWith('/dashboard')}>Dashboard</a>
				<a href="/nodes" class:active={path.startsWith('/nodes')}>Nodes</a>
				<a href="/subscriptions" class:active={path.startsWith('/subscriptions')}
					>Subscriptions</a
				>
			</nav>
			<button type="button" class="logout" onclick={logout}>Log out</button>
		</header>
		<main class="content">
			{@render children()}
		</main>
	</div>
{:else}
	<main class="boot">Checking session…</main>
{/if}

<style>
	.app {
		min-height: 100vh;
		font-family: system-ui, sans-serif;
		color: #1a1a1a;
		background: #f6f7f9;
	}
	.top {
		display: flex;
		align-items: center;
		gap: 1.25rem;
		padding: 0.75rem 1.25rem;
		background: #fff;
		border-bottom: 1px solid #e5e7eb;
	}
	.brand {
		font-weight: 700;
		color: #111;
		text-decoration: none;
		letter-spacing: -0.02em;
	}
	nav {
		display: flex;
		gap: 0.75rem;
		flex: 1;
	}
	nav a {
		color: #555;
		text-decoration: none;
		font-size: 0.95rem;
		padding: 0.25rem 0.4rem;
		border-radius: 4px;
	}
	nav a:hover {
		color: #1a56db;
	}
	nav a.active {
		color: #1a56db;
		font-weight: 600;
	}
	.logout {
		font: inherit;
		font-size: 0.9rem;
		padding: 0.35rem 0.7rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		background: #fff;
		cursor: pointer;
	}
	.logout:hover {
		border-color: #999;
	}
	.content {
		max-width: 56rem;
		margin: 0 auto;
		padding: 1.5rem 1.25rem 3rem;
	}
	.boot {
		font-family: system-ui, sans-serif;
		max-width: 32rem;
		margin: 4rem auto;
		padding: 0 1rem;
	}
</style>
