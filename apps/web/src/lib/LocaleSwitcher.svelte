<script lang="ts">
	import { SUPPORTED_LOCALES, i18n, setLocale, t, type LocaleId } from '$lib/i18n.svelte';

	function onChange(e: Event) {
		const v = (e.currentTarget as HTMLSelectElement).value;
		if (v === 'en' || v === 'zh-CN') setLocale(v as LocaleId);
	}
</script>

<label class="lang">
	<span class="sr">{t('common.language')}</span>
	<select value={i18n.locale} onchange={onChange} aria-label={t('common.language')}>
		{#each SUPPORTED_LOCALES as opt (opt.id)}
			<option value={opt.id}>{opt.label}</option>
		{/each}
	</select>
</label>

<style>
	.lang {
		display: inline-flex;
		align-items: center;
	}
	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
	select {
		font: inherit;
		font-size: 0.8rem;
		font-family: var(--font-mono);
		padding: 0.3rem 0.5rem;
		border: 1px solid var(--line);
		border-radius: var(--r-sm);
		background: var(--bg-input);
		color: var(--ink-muted);
		cursor: pointer;
	}
	select:hover {
		border-color: var(--line-strong);
		color: var(--ink);
	}
	select:focus {
		outline: none;
		border-color: var(--signal);
		box-shadow: var(--shadow-focus);
	}
	option {
		background: var(--bg-panel);
		color: var(--ink);
	}
</style>
