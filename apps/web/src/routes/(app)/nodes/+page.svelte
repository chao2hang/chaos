<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listNodes,
		importNodes,
		deleteNode,
		listLatency,
		testLatency,
		ApiClientError,
		type NodeDto,
		type LatencyDto
	} from '$lib/api';
	import { latencyTone, formatLatencyMs, latencyClass } from '$lib/latency';
	import { apiErrorText, t } from '$lib/i18n.svelte';

	let nodes = $state<NodeDto[]>([]);
	let latencyById = $state<Record<string, LatencyDto>>({});
	let importText = $state('');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);
	let testingId = $state<string | null>(null);

	function mergeLatency(results: LatencyDto[]) {
		const next = { ...latencyById };
		for (const r of results) next[r.id] = r;
		latencyById = next;
	}

	async function load() {
		error = '';
		try {
			const [n, lat] = await Promise.all([listNodes(), listLatency()]);
			nodes = n.nodes;
			const map: Record<string, LatencyDto> = {};
			for (const r of lat.results) map[r.id] = r;
			latencyById = map;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('nodes.loadFailed');
		}
	}

	onMount(() => {
		void load();
	});

	async function onImport() {
		error = '';
		message = '';
		const lines = importText
			.split(/\r?\n/)
			.map((l) => l.trim())
			.filter(Boolean);
		if (!lines.length) {
			error = t('nodes.importEmpty');
			return;
		}
		busy = true;
		try {
			const res = await importNodes(lines.map((link) => ({ link })));
			const ok = res.results.filter((r) => r.ok).length;
			const fail = res.results.length - ok;
			const failSuffix = fail ? t('nodes.importedFailSuffix', { fail }) : '';
			message = t('nodes.imported', { ok, failSuffix });
			if (fail) {
				const errs = res.results
					.filter((r): r is Extract<typeof r, { ok: false }> => !r.ok)
					.map((r) => apiErrorText(r.error))
					.slice(0, 3);
				if (errs.length) error = errs.join('; ');
			}
			importText = '';
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('nodes.importFailed');
		} finally {
			busy = false;
		}
	}

	async function onTestOne(id: string) {
		testingId = id;
		error = '';
		try {
			const res = await testLatency([id]);
			mergeLatency(res.results);
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('nodes.latencyFailed');
		} finally {
			testingId = null;
		}
	}

	async function onDelete(id: string) {
		if (!confirm(t('nodes.deleteConfirm'))) return;
		error = '';
		try {
			await deleteNode(id);
			nodes = nodes.filter((n) => n.id !== id);
			const { [id]: _, ...rest } = latencyById;
			latencyById = rest;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('nodes.deleteFailed');
		}
	}
</script>

<span class="eyebrow">inventory · endpoints</span>
<h1 class="page-title">{t('nodes.title')}</h1>
<p class="page-sub">{t('nodes.subtitle')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="import">
	<label for="links">{t('nodes.linksLabel')}</label>
	<textarea
		id="links"
		rows="5"
		placeholder={t('nodes.linksPlaceholder')}
		bind:value={importText}
		disabled={busy}
	></textarea>
	<button type="button" class="primary" disabled={busy} onclick={onImport}>
		{busy ? t('common.importing') : t('common.import')}
	</button>
</section>

<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>{t('nodes.col.name')}</th>
				<th>{t('nodes.col.protocol')}</th>
				<th>{t('nodes.col.address')}</th>
				<th>{t('nodes.col.latency')}</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#if !nodes.length}
				<tr>
					<td colspan="5" class="muted">{t('nodes.empty')}</td>
				</tr>
			{:else}
				{#each nodes as n (n.id)}
					{@const lat = latencyById[n.id]}
					{@const tone = lat ? latencyTone(lat.latency_ms, lat.alive) : 'unknown'}
					<tr>
						<td>
							<div class="name">{n.name}</div>
							{#if n.tag}<span class="tag">{n.tag}</span>{/if}
						</td>
						<td class="mono-cell">{n.protocol ?? t('common.emDash')}</td>
						<td class="addr">{n.address ?? t('common.emDash')}</td>
						<td class={latencyClass(tone)}>
							{lat ? formatLatencyMs(lat.latency_ms, lat.alive) : t('common.emDash')}
							{#if lat?.message && !lat.alive}
								<span class="msg" title={lat.message}>!</span>
							{/if}
						</td>
						<td class="row-actions">
							<button
								type="button"
								disabled={testingId === n.id}
								onclick={() => onTestOne(n.id)}
							>
								{testingId === n.id ? '…' : t('common.test')}
							</button>
							<button type="button" class="danger" onclick={() => onDelete(n.id)}
								>{t('common.delete')}</button
							>
						</td>
					</tr>
				{/each}
			{/if}
		</tbody>
	</table>
</section>

<style>
	.name {
		font-weight: 600;
	}
	.tag {
		display: inline-block;
		margin-top: 0.15rem;
		font-size: 0.72rem;
		font-family: var(--font-mono);
		color: var(--signal);
		background: var(--signal-soft);
		padding: 0.1rem 0.4rem;
		border-radius: 4px;
	}
	.addr,
	.mono-cell {
		font-family: var(--font-mono);
		font-size: 0.82rem;
		color: var(--ink-muted);
	}
	.msg {
		margin-left: 0.25rem;
		cursor: help;
		color: var(--bad);
	}
</style>
