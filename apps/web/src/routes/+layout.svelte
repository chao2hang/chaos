<script lang="ts">
	import { onMount } from 'svelte';
	import favicon from '$lib/assets/favicon.svg';
	import { initLocale } from '$lib/i18n';

	let { children } = $props();

	onMount(() => {
		initLocale();
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

{@render children()}
