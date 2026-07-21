<script lang="ts">
	import { onMount } from 'svelte';
	import favicon from '$lib/assets/favicon.svg';
	import ToastViewport from '$lib/components/ui/ToastViewport.svelte';
	import { initLocale } from '$lib/i18n.svelte';
	import { initTheme } from '$lib/theme.svelte';
	import '../app.css';

	let { children } = $props();

	onMount(() => {
		initLocale();
		initTheme();
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>chaos</title>
	<meta name="description" content="chaos — control plane for dae" />
</svelte:head>

{@render children()}
<ToastViewport />
