<script lang="ts">
	import { onMount } from 'svelte';
	import favicon from '$lib/assets/favicon.svg';
	import { initLocale } from '$lib/i18n';
	import '../app.css';

	let { children } = $props();

	onMount(() => {
		initLocale();
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>chaos</title>
	<meta name="description" content="chaos — control plane for dae" />
</svelte:head>

{@render children()}
