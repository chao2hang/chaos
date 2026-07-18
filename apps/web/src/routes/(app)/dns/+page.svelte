<script lang="ts">
	import { onMount } from 'svelte';
	import {
		getDns,
		putDns,
		ApiClientError,
		type DnsRuleDto,
		type DnsUpstreamDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n';

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

<h1>{t('dns.title')}</h1>
<p class="muted">{t('dns.subtitle')}</p>
<p class="hint">{t('dns.hint')}</p>

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

<h2>{t('dns.upstreams')}</h2>
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

<h2>{t('dns.rules')}</h2>
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
	h1 {
		margin: 0 0 0.25rem;
		font-size: 1.5rem;
	}
	h2 {
		margin: 1.25rem 0 0.5rem;
		font-size: 1.05rem;
	}
	.muted {
		color: #555;
		font-size: 0.95rem;
	}
	.hint {
		color: #667085;
		font-size: 0.85rem;
	}
	.error {
		color: #b42318;
		font-size: 0.9rem;
	}
	.ok {
		color: #027a48;
		font-size: 0.9rem;
	}
	.import {
		margin: 1rem 0;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		max-width: 24rem;
	}
	label {
		font-size: 0.9rem;
		font-weight: 600;
	}
	input {
		font: inherit;
		padding: 0.45rem 0.55rem;
		border: 1px solid #ccc;
		border-radius: 6px;
	}
	input.wide {
		width: 100%;
		min-width: 14rem;
	}
	.table-wrap {
		overflow-x: auto;
		background: #fff;
		border: 1px solid #e5e7eb;
		border-radius: 8px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.9rem;
	}
	th,
	td {
		text-align: left;
		padding: 0.5rem 0.65rem;
		border-bottom: 1px solid #eee;
		vertical-align: middle;
	}
	th {
		font-size: 0.8rem;
		text-transform: uppercase;
		color: #555;
		background: #fafafa;
	}
	.mb {
		margin: 0.6rem 0 0.25rem;
	}
	.actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 1rem;
	}
	button {
		font: inherit;
		padding: 0.5rem 0.8rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		background: #fff;
		cursor: pointer;
	}
	button.primary {
		background: #1a56db;
		border-color: #1a56db;
		color: #fff;
	}
	button.danger {
		color: #b42318;
		border-color: #f3b0a8;
	}
	button:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}
</style>
