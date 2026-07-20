<script lang="ts">
	import type { Snippet } from 'svelte';
	import AppLogo from './AppLogo.svelte';
	import LocaleSwitcher from '$lib/LocaleSwitcher.svelte';

	let { title, description, children } = $props<{
		title: string;
		description: string;
		children: Snippet;
	}>();
</script>

<div class="auth-shell">
	<header>
		<a href="/" aria-label="chaos"><AppLogo /></a>
		<div class="language"><LocaleSwitcher /></div>
	</header>
	<main>
		<section class="auth-content">
			<h1>{title}</h1>
			<p class="description">{description}</p>
			<div class="auth-body">{@render children()}</div>
		</section>
	</main>
</div>

<style>
	.auth-shell {
		display: grid;
		grid-template-rows: auto 1fr;
		min-height: 100vh;
		background: var(--canvas);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		min-height: 4.5rem;
		padding: 0 var(--space-8);
		border-bottom: 1px solid var(--line);
	}

	header a {
		text-decoration: none;
	}

	.language {
		width: 8.5rem;
	}

	main {
		display: grid;
		place-items: center;
		padding: var(--space-10) var(--space-4);
	}

	.auth-content {
		width: min(22rem, 100%);
	}

	h1 {
		margin: 0;
		font-size: 1.6rem;
		font-weight: 720;
		line-height: 1.2;
		letter-spacing: 0;
	}

	.description {
		margin: var(--space-2) 0 0;
		color: var(--ink-muted);
		font-size: 0.86rem;
	}

	.auth-body {
		margin-top: var(--space-8);
	}

	@media (max-width: 640px) {
		header {
			min-height: 4rem;
			padding: 0 var(--space-4);
		}

		main {
			place-items: start center;
			padding-top: var(--space-12);
		}
	}
</style>
