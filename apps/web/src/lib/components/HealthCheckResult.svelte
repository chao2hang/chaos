<script lang="ts">
	import { CheckCircle2, XCircle, AlertTriangle } from '@lucide/svelte';
	import type { HealthCheckReport } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { report, compact = false }: { report: HealthCheckReport; compact?: boolean } = $props();

	const tone = $derived(report.ok ? 'ok' : 'bad');
</script>

<div class="health" data-tone={tone} data-compact={compact}>
	<div class="head">
		{#if report.ok}
			<CheckCircle2 size={14} class="ok" />
		{:else}
			<XCircle size={14} class="bad" />
		{/if}
		<span class="title">
			{report.ok ? t('health.ok') : t('health.failed')}
		</span>
		<span class="count">{report.successes}/{report.attempts}</span>
	</div>

	{#if !compact}
		<ul class="probes">
			{#each report.results as probe}
				<li class:ok={probe.ok} class:bad={!probe.ok}>
					{#if probe.ok}
						<CheckCircle2 size={12} class="ok" />
					{:else}
						<XCircle size={12} class="bad" />
					{/if}
					<code class="target">{probe.target}</code>
					{#if probe.ok}
						<span class="meta">
							{probe.status ?? ''} · {probe.latency_ms}{t('health.ms')}
						</span>
					{:else}
						<span class="meta">{probe.error ?? t('health.failed')}</span>
					{/if}
				</li>
			{/each}
		</ul>
		{#if report.error}
			<p class="error"><AlertTriangle size={12} /> {report.error}</p>
		{/if}
	{/if}
</div>

<style>
	.health {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: 0.75rem;
	}

	.head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.title {
		font-weight: 600;
	}

	.count {
		margin-left: auto;
		font-family: var(--font-mono);
		color: var(--ink-muted);
	}

	.ok {
		color: var(--ink);
	}

	.bad {
		color: var(--ink-muted);
	}

	.probes {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.probes li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--ink-muted);
	}

	.target {
		font-size: 0.7rem;
	}

	.meta {
		margin-left: auto;
		font-size: 0.68rem;
	}

	.error {
		display: flex;
		align-items: center;
		gap: 4px;
		margin: 0;
		color: var(--ink);
	}
</style>
