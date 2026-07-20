<script lang="ts">
	import { Languages } from '@lucide/svelte';
	import { SUPPORTED_LOCALES, i18n, setLocale, t, type LocaleId } from '$lib/i18n.svelte';

	function onChange(event: Event) {
		const value = (event.currentTarget as HTMLSelectElement).value;
		if (value === 'en' || value === 'zh-CN') setLocale(value as LocaleId);
	}
</script>

<label class="language">
	<span class="sr-only">{t('common.language')}</span>
	<Languages size={15} strokeWidth={1.8} aria-hidden="true" />
	<select value={i18n.locale} onchange={onChange} aria-label={t('common.language')}>
		{#each SUPPORTED_LOCALES as option (option.id)}
			<option value={option.id}>{option.label}</option>
		{/each}
	</select>
</label>

<style>
	.language {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: 2.25rem;
		padding: 0 0.55rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink-muted);
	}

	.language:hover,
	.language:focus-within {
		border-color: var(--line-strong);
		color: var(--ink);
	}

	select {
		width: 100%;
		min-height: 2.1rem;
		padding: 0;
		border: 0;
		background: transparent;
		box-shadow: none;
		color: inherit;
		font-size: 0.75rem;
		font-weight: 590;
	}

	select:hover,
	select:focus {
		border: 0;
		box-shadow: none;
	}
</style>
