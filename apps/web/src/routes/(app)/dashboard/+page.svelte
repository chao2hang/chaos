<script lang="ts">
	import { onMount } from 'svelte';
	import { Activity, Download, Gauge, Play, Power, RefreshCw, Square } from '@lucide/svelte';
	import {
		health,
		getRuntime,
		testLatency,
		applyRuntime,
		stopRuntime,
		updateGeoIpData,
		listLatency,
		ApiClientError,
		type HealthResponse,
		type RuntimeStatus,
		type LatencyDto
	} from '$lib/api';
	import { latencyTone, formatLatencyMs, latencyClass } from '$lib/latency';
	import { sortByLatency } from '$lib/latencySessionCore';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Metric from '$lib/components/ui/Metric.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import Status from '$lib/components/ui/Status.svelte';

	let healthInfo = $state<HealthResponse | null>(null);
	let runtime = $state<RuntimeStatus | null>(null);
	let latency = $state<LatencyDto[]>([]);
	let error = $state('');
	let message = $state('');
	let busy = $state<'refresh' | 'latency' | 'apply' | 'stop' | 'geoip' | ''>('');
	let loaded = $state(false);
	let confirmStop = $state(false);

	async function refresh() {
		busy = 'refresh';
		error = '';
		try {
			const [healthResult, runtimeResult, latencyResult] = await Promise.all([
				health(),
				getRuntime(),
				listLatency()
			]);
			healthInfo = healthResult;
			runtime = runtimeResult;
			latency = latencyResult.results;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.loadFailed');
		} finally {
			busy = '';
			loaded = true;
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
			const response = await testLatency(null);
			latency = response.results;
			const alive = response.results.filter((result) => result.alive).length;
			message = t('dashboard.latencyFinished', { alive, total: response.results.length });
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.latencyFailed');
		} finally {
			busy = '';
		}
	}

	async function onApply() {
		busy = 'apply';
		error = '';
		message = '';
		try {
			const response = await applyRuntime();
			message = t('dashboard.applied', {
				nodes: response.nodes,
				path: response.config_path,
				running: String(response.running)
			});
			runtime = await getRuntime();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.applyFailed');
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
			confirmStop = false;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.stopFailed');
		} finally {
			busy = '';
		}
	}

	async function onUpdateGeoIp() {
		busy = 'geoip';
		error = '';
		message = '';
		try {
			const result = await updateGeoIpData();
			if (runtime) runtime = { ...runtime, geoip_data: result };
			message = t('dashboard.geoipUpdated');
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.geoipUpdateFailed');
		} finally {
			busy = '';
		}
	}

	function formatBytes(value: number): string {
		if (value < 1024) return `${value} B`;
		return `${(value / 1024 / 1024).toFixed(1)} MB`;
	}

	const latencySummary = $derived.by(() => {
		const total = latency.length;
		const alive = latency.filter((result) => result.alive).length;
		const tones = { good: 0, warn: 0, bad: 0, unknown: 0 };
		for (const result of latency) tones[latencyTone(result.latency_ms, result.alive)]++;
		return { total, alive, ...tones };
	});

	const sortedLatency = $derived(sortByLatency(latency, (result) => result));
</script>

<AppPage>
	<PageHeader title={t('dashboard.title')} description={t('dashboard.subtitle')} meta="chaos / overview">
		{#snippet actions()}
			<Button icon={Gauge} loading={busy === 'latency'} disabled={!!busy} onclick={runAllLatency}>
				{busy === 'latency' ? t('dashboard.testing') : t('dashboard.testAll')}
			</Button>
			<Button
				variant="ghost"
				size="icon"
				icon={RefreshCw}
				loading={busy === 'refresh'}
				disabled={!!busy}
				aria-label={t('common.refresh')}
				title={t('common.refresh')}
				onclick={refresh}
			/>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}
	{#if runtime?.needs_republish}<Notice message={t('dashboard.needsRepublish')} />{/if}

	{#if !loaded}
		<LoadingState label={t('dashboard.loading')} />
	{:else}
		<section class="metrics" aria-label={t('dashboard.title')}>
			<Metric
				label={t('dashboard.field.running')}
				value={runtime?.running ? t('common.on') : t('common.off')}
				note={t('dashboard.daeRuntime')}
				tone={runtime?.running ? 'positive' : 'negative'}
			/>
			<Metric
				label={t('dashboard.latencySummary')}
				value={`${latencySummary.alive}/${latencySummary.total}`}
				note={t('dashboard.aliveLine', latencySummary)}
				tone={latencySummary.total > 0 && latencySummary.alive === latencySummary.total ? 'positive' : 'neutral'}
			/>
			<Metric
				label="API"
				value={healthInfo?.ok ? t('common.available') : t('common.unavailable')}
				note={healthInfo?.api_version ?? t('common.unknown')}
				tone={healthInfo?.ok ? 'positive' : 'negative'}
			/>
			<Metric
				label="dae"
				value={healthInfo?.dae_binary_ok ? t('common.ready') : t('common.missing')}
				note={healthInfo?.dae_binary ?? t('common.none')}
				tone={healthInfo?.dae_binary_ok ? 'positive' : 'negative'}
			/>
		</section>

		<div class="page-grid">
			<div class="span-6">
				<Section title={t('dashboard.runtime')} description={t('dashboard.runtimeDescription')}>
					<div class="runtime-row">
						<Status
							label={runtime?.running ? t('common.running') : t('common.stopped')}
							tone={runtime?.running ? 'positive' : 'neutral'}
						/>
					<div class="inline-actions">
							<Button variant="primary" icon={Play} loading={busy === 'apply'} disabled={!!busy || !!runtime?.needs_republish} onclick={onApply}>
								{busy === 'apply' ? t('dashboard.applying') : t('dashboard.apply')}
							</Button>
							<Button
								icon={Square}
								disabled={!!busy || !runtime?.running}
								onclick={() => (confirmStop = true)}
							>
								{t('dashboard.stop')}
							</Button>
						</div>
					</div>
					<div class="runtime-row geoip-row">
						<div>
							<strong>{t('dashboard.geoip')}</strong>
							<p>{t('dashboard.geoipDescription')}</p>
						</div>
						<Button
							icon={Download}
							loading={busy === 'geoip'}
							disabled={!!busy}
							onclick={onUpdateGeoIp}
						>
							{t('dashboard.updateGeoip')}
						</Button>
					</div>
					<dl class="details">
						<div>
							<dt>{t('dashboard.field.configExists')}</dt>
							<dd>{runtime ? String(runtime.config_exists) : t('common.unknown')}</dd>
						</div>
						<div>
							<dt>{t('dashboard.field.workDir')}</dt>
							<dd><code>{runtime?.work_dir ?? t('common.unknown')}</code></dd>
						</div>
						<div>
							<dt>{t('dashboard.field.dataPlane')}</dt>
							<dd><code>{runtime?.data_plane ?? t('common.unknown')}</code></dd>
						</div>
						<div>
							<dt>{t('dashboard.field.geoipData')}</dt>
							<dd>{runtime?.geoip_data.exists ? formatBytes(runtime.geoip_data.bytes) : t('common.missing')}</dd>
						</div>
					</dl>
				</Section>
			</div>

			<div class="span-6">
				<Section title={t('dashboard.apiHealth')} description={t('dashboard.systemDescription')}>
					<dl class="details system-details">
						<div>
							<dt>{t('dashboard.field.apiVersion')}</dt>
							<dd><code>{healthInfo?.api_version ?? t('common.unknown')}</code></dd>
						</div>
						<div>
							<dt>{t('dashboard.field.daeBinary')}</dt>
							<dd><code>{healthInfo?.dae_binary ?? t('common.none')}</code></dd>
						</div>
					</dl>
				</Section>
			</div>

			<div class="span-12">
				<Section
					title={t('dashboard.latencySummary')}
					description={t('dashboard.latencyDescription')}
					count={latency.length}
					flush
				>
					{#if latency.length}
						<ul class="latency-list">
							{#each sortedLatency.slice(0, 12) as result (result.id)}
								<li>
									<code>{result.id.slice(0, 12)}</code>
									<span class={latencyClass(latencyTone(result.latency_ms, result.alive))}>
										{formatLatencyMs(result.latency_ms, result.alive)}
									</span>
								</li>
							{/each}
						</ul>
					{:else}
						<EmptyState
							icon={Activity}
							title={t('dashboard.noLatency')}
							description={t('dashboard.noLatencyDescription')}
						>
							{#snippet actions()}
								<Button icon={Gauge} onclick={runAllLatency}>{t('dashboard.testAll')}</Button>
							{/snippet}
						</EmptyState>
					{/if}
				</Section>
			</div>
		</div>
	{/if}
</AppPage>

<ConfirmDialog
	bind:open={confirmStop}
	title={t('dashboard.stopConfirmTitle')}
	description={t('dashboard.stopConfirmDescription')}
	confirmLabel={t('dashboard.stop')}
	cancelLabel={t('common.cancel')}
	busy={busy === 'stop'}
	danger
	onconfirm={onStop}
/>

<style>
	.metrics {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
		overflow: hidden;
	}

	.runtime-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding-bottom: var(--space-5);
	}

	.geoip-row {
		padding-top: var(--space-4);
		border-top: 1px solid var(--line);
	}

	.geoip-row strong,
	.geoip-row p {
		display: block;
	}

	.geoip-row strong {
		font-size: 0.82rem;
	}

	.geoip-row p {
		margin: var(--space-1) 0 0;
		color: var(--ink-muted);
		font-size: 0.75rem;
	}

	.details {
		display: flex;
		flex-direction: column;
		margin: 0;
		border-top: 1px solid var(--line);
	}

	.details > div {
		display: grid;
		grid-template-columns: minmax(8rem, 0.45fr) minmax(0, 1fr);
		gap: var(--space-4);
		padding: var(--space-3) 0;
		border-bottom: 1px solid var(--line);
	}

	.details > div:last-child {
		border-bottom: 0;
	}

	dt {
		color: var(--ink-muted);
		font-size: 0.75rem;
	}

	dd {
		min-width: 0;
		margin: 0;
		font-size: 0.78rem;
		text-align: right;
		word-break: break-word;
	}

	.latency-list {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.latency-list li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border-right: 1px solid var(--line);
		border-bottom: 1px solid var(--line);
	}

	.latency-list li:nth-child(3n) {
		border-right: 0;
	}

	.latency-list code {
		min-width: 0;
		overflow: hidden;
		color: var(--ink-muted);
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 960px) {
		.metrics {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.metrics :global(.metric:nth-child(2)) {
			border-right: 0;
		}

		.metrics :global(.metric:nth-child(-n + 2)) {
			border-bottom: 1px solid var(--line);
		}

		.latency-list {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.latency-list li:nth-child(3n) {
			border-right: 1px solid var(--line);
		}

		.latency-list li:nth-child(2n) {
			border-right: 0;
		}
	}

	@media (max-width: 680px) {
		.metrics {
			grid-template-columns: 1fr;
		}

		.metrics :global(.metric) {
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.runtime-row {
			align-items: flex-start;
			flex-direction: column;
		}

		.latency-list {
			grid-template-columns: 1fr;
		}

		.latency-list li,
		.latency-list li:nth-child(3n) {
			border-right: 0;
		}
	}
</style>
