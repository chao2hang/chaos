<script lang="ts">
	import { onMount } from 'svelte';
	import {
		health,
		getRuntime,
		testLatency,
		applyRuntime,
		stopRuntime,
		listLatency,
		ApiClientError,
		type HealthResponse,
		type RuntimeStatus,
		type LatencyDto
	} from '$lib/api';
	import { latencyTone, formatLatencyMs, latencyClass } from '$lib/latency';
	import { apiErrorText, t } from '$lib/i18n.svelte';

	let healthInfo = $state<HealthResponse | null>(null);
	let runtime = $state<RuntimeStatus | null>(null);
	let latency = $state<LatencyDto[]>([]);
	let error = $state('');
	let message = $state('');
	let busy = $state('');

	async function refresh() {
		error = '';
		try {
			const [h, r, lat] = await Promise.all([health(), getRuntime(), listLatency()]);
			healthInfo = h;
			runtime = r;
			latency = lat.results;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('dashboard.loadFailed');
		}
	}

	onMount(() => {
		void refresh();
	});

	async function runAllLatency() {
		busy = 'latency';
		error = '';
		message = '';
		try {
			const res = await testLatency(null);
			latency = res.results;
			const alive = res.results.filter((r) => r.alive).length;
			message = t('dashboard.latencyFinished', {
				alive,
				total: res.results.length
			});
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('dashboard.latencyFailed');
		} finally {
			busy = '';
		}
	}

	async function onApply() {
		busy = 'apply';
		error = '';
		message = '';
		try {
			const res = await applyRuntime();
			message = t('dashboard.applied', {
				nodes: res.nodes,
				path: res.config_path,
				running: String(res.running)
			});
			runtime = await getRuntime();
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('dashboard.applyFailed');
		} finally {
			busy = '';
		}
	}

	async function onStop() {
		busy = 'stop';
		error = '';
		message = '';
		try {
			runtime = await stopRuntime();
			message = t('dashboard.stopRequested');
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('dashboard.stopFailed');
		} finally {
			busy = '';
		}
	}

	const latencySummary = $derived.by(() => {
		const total = latency.length;
		const alive = latency.filter((r) => r.alive).length;
		const tones = { good: 0, warn: 0, bad: 0, unknown: 0 };
		for (const r of latency) {
			tones[latencyTone(r.latency_ms, r.alive)]++;
		}
		return { total, alive, ...tones };
	});
</script>

<span class="eyebrow">runtime · overview</span>
<h1 class="page-title">{t('dashboard.title')}</h1>
<p class="page-sub">{t('dashboard.subtitle')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="kpi-row">
	<div class="card kpi-card">
		<div class="kpi-label">{t('dashboard.field.running')}</div>
		<div class="kpi" class:lat-good={runtime?.running} class:lat-bad={runtime && !runtime.running}>
			{runtime ? (runtime.running ? 'ON' : 'OFF') : '—'}
		</div>
	</div>
	<div class="card kpi-card">
		<div class="kpi-label">{t('dashboard.latencySummary')}</div>
		<div class="kpi">
			{latencySummary.alive}<span class="kpi-suffix">/{latencySummary.total}</span>
		</div>
	</div>
	<div class="card kpi-card">
		<div class="kpi-label">API</div>
		<div class="kpi" class:lat-good={healthInfo?.ok} class:lat-bad={healthInfo && !healthInfo.ok}>
			{healthInfo ? (healthInfo.ok ? 'OK' : 'DOWN') : '—'}
		</div>
	</div>
	<div class="card kpi-card">
		<div class="kpi-label">dae bin</div>
		<div
			class="kpi small"
			class:lat-good={healthInfo?.dae_binary_ok}
			class:lat-bad={healthInfo && !healthInfo.dae_binary_ok}
		>
			{healthInfo ? (healthInfo.dae_binary_ok ? 'OK' : 'MISS') : '—'}
		</div>
	</div>
</section>

<section class="cards">
	<div class="card">
		<h2>{t('dashboard.apiHealth')}</h2>
		{#if healthInfo}
			<ul>
				<li>
					{t('dashboard.field.apiVersion')}:
					<span class="mono">{healthInfo.api_version}</span>
				</li>
				<li>
					{t('dashboard.field.daeBinary')}:
					{#if healthInfo.dae_binary}
						<code>{healthInfo.dae_binary}</code>
					{:else}
						<span class="muted">{t('common.none')}</span>
					{/if}
				</li>
			</ul>
		{:else}
			<p class="muted">{t('dashboard.loading')}</p>
		{/if}
	</div>

	<div class="card">
		<h2>{t('dashboard.runtime')}</h2>
		{#if runtime}
			<ul>
				<li>
					{t('dashboard.field.configExists')}:
					<span class="mono">{String(runtime.config_exists)}</span>
				</li>
				<li>{t('dashboard.field.workDir')}: <code>{runtime.work_dir}</code></li>
			</ul>
		{:else}
			<p class="muted">{t('dashboard.loading')}</p>
		{/if}
	</div>

	<div class="card">
		<h2>{t('dashboard.latencySummary')}</h2>
		<p class="summary-line mono">
			{t('dashboard.aliveLine', {
				alive: latencySummary.alive,
				total: latencySummary.total,
				good: latencySummary.good,
				warn: latencySummary.warn,
				bad: latencySummary.bad
			})}
		</p>
		{#if latency.length}
			<ul class="lat-list">
				{#each latency.slice(0, 8) as r (r.id)}
					<li>
						<code class="id">{r.id.slice(0, 8)}</code>
						<span class={latencyClass(latencyTone(r.latency_ms, r.alive))}>
							{formatLatencyMs(r.latency_ms, r.alive)}
						</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="muted">{t('dashboard.noLatency')}</p>
		{/if}
	</div>
</section>

<div class="actions">
	<button type="button" disabled={!!busy} onclick={runAllLatency}>
		{busy === 'latency' ? t('dashboard.testing') : t('dashboard.testAll')}
	</button>
	<button type="button" class="primary" disabled={!!busy} onclick={onApply}>
		{busy === 'apply' ? t('dashboard.applying') : t('dashboard.apply')}
	</button>
	<button type="button" disabled={!!busy} onclick={onStop}>
		{busy === 'stop' ? t('dashboard.stopping') : t('dashboard.stop')}
	</button>
	<button type="button" class="ghost" disabled={!!busy} onclick={() => refresh()}
		>{t('common.refresh')}</button
	>
</div>

<style>
	.kpi-row {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: var(--space-sm);
		margin: var(--space-md) 0;
	}
	.kpi-card {
		padding: var(--space-md);
	}
	.kpi-card:hover {
		transform: none;
	}
	.kpi-suffix {
		font-size: 1rem;
		color: var(--ink-dim);
		font-weight: 500;
	}
	.kpi.small {
		font-size: 1.35rem;
	}
	.mono {
		font-family: var(--font-mono);
		font-size: 0.9em;
		color: var(--ink-muted);
	}
	.summary-line {
		margin: 0;
		font-size: 0.82rem;
		line-height: 1.5;
		color: var(--ink);
	}
	.lat-list {
		list-style: none;
		padding: 0;
		margin: 0.75rem 0 0;
	}
	.lat-list li {
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		font-size: 0.82rem;
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--line);
	}
	.lat-list li:last-child {
		border-bottom: none;
	}
	.id {
		color: var(--ink-dim);
		font-size: 0.78rem;
	}

	@media (max-width: 900px) {
		.kpi-row {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
