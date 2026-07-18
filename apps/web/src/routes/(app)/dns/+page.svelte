<script lang="ts">
	import { onMount } from 'svelte';
	import {
		getDns,
		putDns,
		ApiClientError,
		type DnsRuleDto,
		type DnsUpstreamDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';

	let upstreams = $state<DnsUpstreamDto[]>([]);
	let rules = $state<DnsRuleDto[]>([]);
	let fallback = $state('alidns');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);

	async function load() {
		error = '';
		try {
			const doc = await getDns();
			upstreams = doc.upstreams;
			rules = doc.rules;
			fallback = doc.fallback;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('dns.loadFailed');
		}
	}

	onMount(() => {
		void load();
	});

	function addUpstream() {
		upstreams = [...upstreams, { name: '', address: '' }];
	}

	function removeUpstream(i: number) {
		upstreams = upstreams.filter((_, idx) => idx !== i);
	}

	function addRule() {
		rules = [...rules, { expression: '', upstream: fallback || 'alidns', enabled: true }];
	}

	function removeRule(i: number) {
		rules = rules.filter((_, idx) => idx !== i);
	}

	async function onSave() {
		error = '';
		message = '';
		if (!fallback.trim()) {
			error = t('dns.fallbackRequired');
			return;
		}
		if (!upstreams.some((u) => u.name.trim() && u.address.trim())) {
			error = t('dns.upstreamRequired');
			return;
		}
		busy = true;
		try {
			const doc = await putDns({
				upstreams: upstreams.map((u) => ({
					name: u.name.trim(),
					address: u.address.trim()
				})),
				rules: rules.map((r) => ({
					expression: r.expression.trim(),
					upstream: r.upstream.trim(),
					enabled: r.enabled
				})),
				fallback: fallback.trim()
			});
			upstreams = doc.upstreams;
			rules = doc.rules;
			fallback = doc.fallback;
			message = t('dns.saved');
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('dns.saveFailed');
		} finally {
			busy = false;
		}
	}
</script>

<span class="eyebrow">resolve · upstreams</span>
<h1 class="page-title">{t('dns.title')}</h1>
<p class="page-sub">{t('dns.subtitle')}</p>
<p class="page-hint">{t('dns.hint')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="import">
	<label for="dns-fallback">{t('dns.fallback')}</label>
	<input id="dns-fallback" bind:value={fallback} disabled={busy} />
</section>

<h2 class="section-label">{t('dns.upstreams')}</h2>
<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>{t('dns.name')}</th>
				<th>{t('dns.address')}</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each upstreams as u, i (i)}
				<tr>
					<td><input bind:value={u.name} disabled={busy} /></td>
					<td><input class="wide" bind:value={u.address} disabled={busy} /></td>
					<td>
						<button type="button" class="danger" disabled={busy} onclick={() => removeUpstream(i)}
							>{t('common.delete')}</button
						>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</section>
<button type="button" class="mb" disabled={busy} onclick={addUpstream}>{t('dns.addUpstream')}</button>

<h2 class="section-label">{t('dns.rules')}</h2>
<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>{t('common.enabled')}</th>
				<th>{t('dns.expression')}</th>
				<th>{t('dns.upstream')}</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each rules as r, i (i)}
				<tr>
					<td><input type="checkbox" bind:checked={r.enabled} disabled={busy} /></td>
					<td><input class="wide" bind:value={r.expression} disabled={busy} /></td>
					<td><input bind:value={r.upstream} disabled={busy} /></td>
					<td>
						<button type="button" class="danger" disabled={busy} onclick={() => removeRule(i)}
							>{t('common.delete')}</button
						>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</section>

<div class="actions">
	<button type="button" disabled={busy} onclick={addRule}>{t('dns.addRule')}</button>
	<button type="button" class="primary" disabled={busy} onclick={onSave}>
		{busy ? t('common.saving') : t('common.save')}
	</button>
</div>

<style>
	.section-label {
		margin: 1.5rem 0 0.65rem;
		font-size: 0.75rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--ink-dim);
		font-family: var(--font-mono);
	}
	input.wide {
		width: 100%;
		min-width: 14rem;
		font-family: var(--font-mono);
		font-size: 0.85rem;
	}
	input[type='checkbox'] {
		width: 1rem;
		height: 1rem;
		accent-color: var(--signal);
	}
	.mb {
		margin: 0.65rem 0 0.25rem;
	}
</style>
