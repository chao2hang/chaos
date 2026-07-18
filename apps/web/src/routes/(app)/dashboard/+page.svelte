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
	import { apiErrorText, t } from '$lib/i18n';

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

<h1>{t('dashboard.title')}</h1>
<p class="muted">{t('dashboard.subtitle')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="cards">
	<div class="card">
		<h2>{t('dashboard.apiHealth')}</h2>
		{#if healthInfo}
			<ul>
				<li>{t('dashboard.field.ok')}: <strong>{String(healthInfo.ok)}</strong></li>
				<li>{t('dashboard.field.apiVersion')}: {healthInfo.api_version}</li>
				<li>
					{t('dashboard.field.daeBinary')}:
					{#if healthInfo.dae_binary}
						<code>{healthInfo.dae_binary}</code>
					{:else}
						<span class="muted">{t('common.none')}</span>
					{/if}
				</li>
				<li
					>{t('dashboard.field.daeBinaryOk')}:
					<strong>{String(healthInfo.dae_binary_ok)}</strong></li
				>
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
					{t('dashboard.field.running')}:
					<strong class={runtime.running ? 'lat-good' : 'lat-bad'}
						>{String(runtime.running)}</strong
					>
				</li>
				<li>{t('dashboard.field.configExists')}: {String(runtime.config_exists)}</li>
				<li>{t('dashboard.field.workDir')}: <code>{runtime.work_dir}</code></li>
				<li>{t('dashboard.field.daeBinaryOk')}: {String(runtime.dae_binary_ok)}</li>
			</ul>
		{:else}
			<p class="muted">{t('dashboard.loading')}</p>
		{/if}
	</div>

	<div class="card">
		<h2>{t('dashboard.latencySummary')}</h2>
		<p>
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
	h1 {
		margin: 0 0 0.25rem;
		font-size: 1.5rem;
	}
	.muted {
		color: #555;
		font-size: 0.95rem;
	}
	.error {
		color: #b42318;
		font-size: 0.9rem;
	}
	.ok {
		color: #027a48;
		font-size: 0.9rem;
	}
	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: 1rem;
		margin: 1.25rem 0;
	}
	.card {
		background: #fff;
		border: 1px solid #e5e7eb;
		border-radius: 8px;
		padding: 1rem 1.1rem;
	}
	.card h2 {
		margin: 0 0 0.6rem;
		font-size: 1rem;
	}
	.card ul {
		margin: 0;
		padding-left: 1.1rem;
		font-size: 0.9rem;
		line-height: 1.55;
	}
	code {
		font-size: 0.85em;
		word-break: break-all;
	}
	.lat-list {
		list-style: none;
		padding: 0;
		margin: 0.5rem 0 0;
	}
	.lat-list li {
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		font-size: 0.85rem;
		padding: 0.15rem 0;
	}
	.id {
		color: #666;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	button {
		font: inherit;
		padding: 0.55rem 0.85rem;
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
	button.ghost {
		background: transparent;
	}
	button:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}
	:global(.lat-good) {
		color: #027a48;
		font-weight: 600;
	}
	:global(.lat-warn) {
		color: #b54708;
		font-weight: 600;
	}
	:global(.lat-bad) {
		color: #b42318;
		font-weight: 600;
	}
	:global(.lat-unknown) {
		color: #667085;
	}
</style>
