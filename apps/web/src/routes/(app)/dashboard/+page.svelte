<script lang="ts">
	import { onMount } from 'svelte';
import { Activity, Download, Gauge, Play, Power, RefreshCw, RotateCcw, Square } from '@lucide/svelte';
		import {
			health,
			getRuntime,
			testLatency,
			applyRuntime,
			reloadRuntime,
			stopRuntime,
			updateGeoIpData,
			updateGeositeData,
			listLatency,
			listNodes,
			ApiClientError,
			type HealthResponse,
			type RuntimeStatus,
			type LatencyDto,
			type NodeDto,
			type HealthCheckReport
		} from '$lib/api';
		import { latencyTone, formatLatencyMs, latencyClass } from '$lib/latency';
		import { sortByLatency } from '$lib/latencySessionCore';
		import { apiErrorText, t } from '$lib/i18n.svelte';
		import Button from '$lib/components/ui/Button.svelte';
		import AppPage from '$lib/components/ui/AppPage.svelte';
		import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
		import EmptyState from '$lib/components/ui/EmptyState.svelte';
		import LoadingState from '$lib/components/ui/LoadingState.svelte';
		import LogPanel from '$lib/components/LogPanel.svelte';
		import DiagnosticsPanel from '$lib/components/DiagnosticsPanel.svelte';
		import Metric from '$lib/components/ui/Metric.svelte';
		import Notice from '$lib/components/ui/Notice.svelte';
		import PageHeader from '$lib/components/ui/PageHeader.svelte';
		import Section from '$lib/components/ui/Section.svelte';
		import { toast } from '$lib/toast.svelte';

	let healthInfo = $state<HealthResponse | null>(null);
	let runtime = $state<RuntimeStatus | null>(null);
	let latency = $state<LatencyDto[]>([]);
	let nodes = $state<NodeDto[]>([]);
	let busy = $state<'refresh' | 'latency' | 'apply' | 'reload' | 'stop' | 'geoip' | 'geosite' | ''>('');
	let loaded = $state(false);
	let confirmStop = $state(false);

	function syncNeedsRepublishToast(active: boolean | undefined) {
		if (active) {
			toast.warning({
				id: 'needs-republish',
				title: t('dashboard.needsRepublish'),
				duration: 0
			});
		} else if (active === false) {
			toast.dismiss('needs-republish');
		}
	}

	async function refresh() {
		busy = 'refresh';
		try {
			const [healthResult, runtimeResult, latencyResult, nodesResult] = await Promise.all([
				health(),
				getRuntime(),
				listLatency(),
				listNodes()
			]);
			healthInfo = healthResult;
			runtime = runtimeResult;
			latency = latencyResult.results;
			nodes = nodesResult.nodes;
			syncNeedsRepublishToast(runtimeResult.needs_republish === true);
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.loadFailed')
			});
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
		try {
			const response = await testLatency(null);
			latency = response.results;
			const alive = response.results.filter((result) => result.alive).length;
			toast.success({
				title: t('dashboard.latencyFinished', { alive, total: response.results.length })
			});
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.latencyFailed')
			});
		} finally {
			busy = '';
		}
	}

	async function onApply() {
		busy = 'apply';
		try {
			const response = await applyRuntime();
			const methodLabel = response.reload_method === 'hot' 
				? t('dashboard.reloadHot') 
				: response.reload_method === 'cold_start'
					? t('dashboard.reloadColdStart')
					: t('dashboard.reloadCold');
			const hc = response.health_check;
			if (hc && !hc.ok) {
				toast.warning({
					title: t('dashboard.appliedButUnhealthy'),
					description: healthSummary(hc),
					duration: 8000
				});
			} else {
				toast.success({
					title: t('dashboard.appliedWithMethod', {
						nodes: response.nodes,
						path: response.config_path,
						running: String(response.running),
						method: methodLabel
					}),
					description: hc ? healthSummary(hc) : undefined
				});
			}
			runtime = await getRuntime();
			syncNeedsRepublishToast(runtime.needs_republish === true);
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.applyFailed'),
				description:
					cause instanceof ApiClientError && cause.code === 'dataplane_verify_failed'
						? t('dashboard.verifyFailedRolledBack')
						: undefined,
				duration: 8000
			});
		} finally {
			busy = '';
		}
	}

async function onReload() {
			busy = 'reload';
			try {
				const response = await reloadRuntime();
				const methodLabel =
					response.reload_method === 'hot'
						? t('dashboard.reloadHot')
						: response.reload_method === 'cold_start'
							? t('dashboard.reloadColdStart')
							: t('dashboard.reloadCold');
				const hc = response.health_check;
				if (hc && !hc.ok) {
					toast.warning({
						title: t('dashboard.reloadUnhealthy'),
						description: healthSummary(hc),
						duration: 8000
					});
				} else {
					toast.success({
						title: t('dashboard.reloadedWithMethod', { method: methodLabel }),
						description: hc ? healthSummary(hc) : undefined
					});
				}
				runtime = await getRuntime();
				syncNeedsRepublishToast(runtime.needs_republish === true);
			} catch (cause) {
				toast.error({
					title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.reloadFailed'),
					description:
						cause instanceof ApiClientError && cause.code === 'dataplane_verify_failed'
							? t('dashboard.verifyFailed')
							: undefined,
					duration: 8000
				});
			} finally {
				busy = '';
			}
		}

		async function onStop() {
			busy = 'stop';
			try {
				runtime = await stopRuntime();
				toast.success({ title: t('dashboard.stopRequested') });
				confirmStop = false;
				syncNeedsRepublishToast(runtime.needs_republish === true);
			} catch (cause) {
				toast.error({
					title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.stopFailed')
				});
			} finally {
				busy = '';
			}
		}


	async function onUpdateGeoIp() {
		busy = 'geoip';
		try {
			const result = await updateGeoIpData();
			if (runtime) runtime = { ...runtime, geoip_data: result };
			toast.success({ title: t('dashboard.geoipUpdated') });
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.geoipUpdateFailed')
			});
		} finally {
			busy = '';
		}
	}

	async function onUpdateGeosite() {
		busy = 'geosite';
		try {
			const result = await updateGeositeData();
			if (runtime) runtime = { ...runtime, geosite_data: result };
			toast.success({ title: t('dashboard.geositeUpdated') });
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('dashboard.geositeUpdateFailed')
			});
		} finally {
			busy = '';
		}
	}


	function formatBytes(value: number): string {
		if (value < 1024) return `${value} B`;
		return `${(value / 1024 / 1024).toFixed(1)} MB`;
	}

	function healthSummary(hc: HealthCheckReport): string {
		if (hc.ok) {
			return t('health.summaryOk', { successes: hc.successes, attempts: hc.attempts });
		}
		const failed = hc.results.find((r) => !r.ok);
		const detail = failed?.error
			? failed.error
			: failed
				? `${failed.target} → ${failed.status ?? t('health.noResponse')}`
				: (hc.error ?? t('health.failed'));
		return t('health.summaryFailed', { successes: hc.successes, attempts: hc.attempts, detail });
	}

	const latencySummary = $derived.by(() => {
		const total = latency.length;
		const alive = latency.filter((result) => result.alive).length;
		const tones = { good: 0, warn: 0, bad: 0, unknown: 0 };
		for (const result of latency) tones[latencyTone(result.latency_ms, result.alive)]++;
		return { total, alive, ...tones };
	});

	const sortedLatency = $derived(sortByLatency(latency, (result) => result));
	const nodeNames = $derived(new Map(nodes.map((node) => [node.id, node.name])));

	function latencyDisplayName(result: LatencyDto) {
		return nodeNames.get(result.id) || result.id.slice(0, 12);
	}
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

		{#if !loaded}
		<LoadingState label={t('dashboard.loading')} />
	{:else}
		{#if runtime?.needs_republish}
			<div class="republish-banner">
				<Notice message={t('dashboard.needsRepublish')} tone="error" />
				<a class="republish-link" href="/orchestrate">{t('dashboard.openOrchestrate')}</a>
			</div>
		{/if}
		<section class="status-card" aria-label={t('dashboard.runtime')}>
			<div class="status-card__main">
				<span class="eyebrow">{t('dashboard.daeRuntime')}</span>
				<div class="status-card__title">
					<span class:status-dot--on={runtime?.running} class="status-dot" aria-hidden="true"></span>
					<strong>{runtime?.running ? t('common.running') : t('common.stopped')}</strong>
				</div>
				<p>
					{runtime?.data_plane ?? t('common.unknown')}
					<span aria-hidden="true"> · </span>
					{healthInfo?.api_version ?? t('common.unknown')}
				</p>
			</div>
			<div class="status-card__actions">
				<Button variant="primary" icon={Play} loading={busy === 'apply'} disabled={!!busy || !!runtime?.needs_republish} onclick={onApply}>
					{busy === 'apply' ? t('dashboard.applying') : t('dashboard.apply')}
				</Button>
				<Button
					icon={RotateCcw}
					loading={busy === 'reload'}
					disabled={!!busy || !runtime?.running || !!runtime?.needs_republish}
					onclick={onReload}
				>
					{busy === 'reload' ? t('dashboard.reloading') : t('dashboard.reload')}
				</Button>
				<Button icon={Square} disabled={!!busy || !runtime?.running} onclick={() => (confirmStop = true)}>
					{t('dashboard.stop')}
				</Button>
			</div>
			<div class="status-card__facts">
				<div>
					<span>{t('dashboard.field.configExists')}</span>
					<strong>{runtime ? String(runtime.config_exists) : t('common.unknown')}</strong>
				</div>
				<div>
					<span>{t('dashboard.field.dataPlane')}</span>
					<strong>{runtime?.data_plane ?? t('common.unknown')}</strong>
				</div>
				<div>
					<span>dae</span>
					<strong>{healthInfo?.dae_binary_ok ? t('common.ready') : t('common.missing')}</strong>
				</div>
			</div>
		</section>

		<section class="metrics metrics--compact" aria-label={t('dashboard.title')}>
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
				label={t('dashboard.field.daeBinaryOk')}
				value={healthInfo?.dae_binary_ok ? t('common.ready') : t('common.missing')}
				note={healthInfo?.dae_binary ?? t('common.none')}
				tone={healthInfo?.dae_binary_ok ? 'positive' : 'negative'}
			/>
		</section>

		<div class="page-grid">
			<div class="span-8">
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
									<code>{latencyDisplayName(result)}</code>
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

			<div class="span-4">
				<Section title={t('dashboard.apiHealth')} description={t('dashboard.systemDescription')}>
					<dl class="details">
						<div>
							<dt>{t('dashboard.field.apiVersion')}</dt>
							<dd><code>{healthInfo?.api_version ?? t('common.unknown')}</code></dd>
						</div>
						<div>
							<dt>{t('dashboard.field.daeBinary')}</dt>
							<dd><code>{healthInfo?.dae_binary ?? t('common.none')}</code></dd>
						</div>
						<div>
							<dt>{t('dashboard.field.workDir')}</dt>
							<dd><code>{runtime?.work_dir ?? t('common.unknown')}</code></dd>
						</div>
					</dl>
					<div class="dataset-actions">
						<div>
							<strong>{t('dashboard.geoip')}</strong>
							<span>{runtime?.geoip_data.exists ? formatBytes(runtime.geoip_data.bytes) : t('common.missing')}</span>
						</div>
						<div class="inline-actions">
							<Button size="sm" icon={Download} loading={busy === 'geoip'} disabled={!!busy} onclick={onUpdateGeoIp}>
								{t('dashboard.updateGeoip')}
							</Button>
							<Button size="sm" icon={Download} loading={busy === 'geosite'} disabled={!!busy} onclick={onUpdateGeosite}>
								{t('dashboard.updateGeosite')}
							</Button>
						</div>
					</div>
				</Section>
			</div>

			<div class="span-12">
				<details class="advanced-panel">
					<summary>{t('logs.title')}</summary>
					<LogPanel />
				</details>
			</div>

			<div class="span-12">
				<details class="advanced-panel">
					<summary>{t('diagnostics.title')}</summary>
					<DiagnosticsPanel />
				</details>
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
	.status-card {
		display: grid;
		grid-template-columns: minmax(12rem, 1fr) auto;
		gap: var(--space-5);
		padding: var(--space-5);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-lg);
		background: var(--surface);
	}

	.status-card__main {
		min-width: 0;
	}

	.eyebrow {
		color: var(--ink-muted);
		font-size: 0.68rem;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}

	.status-card__title {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-2);
	}

	.status-card__title strong {
		font-size: 1.35rem;
		letter-spacing: -0.02em;
	}

	.status-card__main p {
		margin: var(--space-2) 0 0;
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.72rem;
	}

	.status-dot {
		width: 0.62rem;
		height: 0.62rem;
		border-radius: 50%;
		background: var(--ink-faint);
		box-shadow: 0 0 0 4px var(--surface-subtle);
	}

	.status-dot--on {
		background: #4f8a58;
	}

	.status-card__actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-2);
		flex-wrap: wrap;
	}

	.status-card__facts {
		display: grid;
		grid-column: 1 / -1;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-4);
		padding-top: var(--space-4);
		border-top: 1px solid var(--line);
	}

	.status-card__facts div {
		display: flex;
		min-width: 0;
		flex-direction: column;
		gap: var(--space-1);
	}

	.status-card__facts span {
		color: var(--ink-muted);
		font-size: 0.7rem;
	}

	.status-card__facts strong {
		overflow: hidden;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.metrics {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
		overflow: hidden;
	}

	.metrics--compact {
		grid-template-columns: repeat(3, minmax(0, 1fr));
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

	.dataset-actions {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		margin-top: var(--space-4);
		padding-top: var(--space-4);
		border-top: 1px solid var(--line);
	}

	.dataset-actions > div:first-child {
		display: flex;
		min-width: 0;
		flex-direction: column;
		gap: var(--space-1);
	}

	.dataset-actions strong {
		font-size: 0.78rem;
	}

	.dataset-actions span {
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.7rem;
	}

	.latency-list {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.latency-list li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: 3.3rem;
		padding: var(--space-3) var(--space-5);
		border-bottom: 1px solid var(--line);
	}

	.latency-list li:nth-child(2n) {
		border-left: 1px solid var(--line);
	}

	.latency-list code {
		min-width: 0;
		overflow: hidden;
		color: var(--ink);
		font-family: inherit;
		font-size: 0.78rem;
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.advanced-panel {
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
		overflow: hidden;
	}

	.advanced-panel summary {
		padding: var(--space-4) var(--space-5);
		color: var(--ink-muted);
		cursor: pointer;
		font-size: 0.8rem;
		font-weight: 650;
		list-style-position: inside;
	}

	.advanced-panel[open] summary {
		border-bottom: 1px solid var(--line);
	}

	.advanced-panel :global(.log-panel),
	.advanced-panel :global(.diagnostics) {
		border: 0;
		border-radius: 0;
	}

	@media (max-width: 960px) {
		.metrics {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.latency-list {
			grid-template-columns: 1fr;
		}

		.latency-list li:nth-child(2n) {
			border-left: 0;
		}
	}

	.republish-banner {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}

	.republish-banner :global(.notice) {
		flex: 1 1 16rem;
	}

	.republish-link {
		display: inline-flex;
		align-items: center;
		min-height: 2.25rem;
		padding: 0 0.85rem;
		border: 1px solid var(--ink);
		border-radius: var(--radius-md);
		background: var(--surface-inverse);
		color: var(--ink-inverse);
		font-size: 0.8rem;
		font-weight: 650;
		text-decoration: none;
	}

	.republish-link:hover {
		background: var(--surface-inverse-hover);
	}

	@media (max-width: 680px) {
		.status-card {
			grid-template-columns: 1fr;
		}

		.status-card__actions {
			justify-content: flex-start;
		}

		.status-card__actions :global(.ui-command) {
			flex: 1 1 auto;
		}

		.status-card__facts {
			grid-template-columns: 1fr;
			gap: var(--space-3);
		}

		.metrics {
			grid-template-columns: 1fr;
		}

		.metrics :global(.metric) {
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.dataset-actions {
			align-items: flex-start;
			flex-direction: column;
		}

		.dataset-actions .inline-actions {
			width: 100%;
		}
	}
</style>
