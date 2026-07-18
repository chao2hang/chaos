<script lang="ts">
	import { SUPPORTED_LOCALES, i18n, setLocale, t, type LocaleId } from '$lib/i18n';

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
		gap: 0.35rem;
		font-size: 0.85rem;
		color: #555;
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
		font-size: 0.85rem;
		padding: 0.3rem 0.4rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		background: #fff;
		color: #1a1a1a;
	}
</style>
