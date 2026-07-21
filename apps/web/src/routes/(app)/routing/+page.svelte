<script lang="ts">
	import { onMount } from 'svelte';
	import { ArrowRight, RefreshCw, Route as RouteIcon } from '@lucide/svelte';
	import { ApiClientError, getRouting, type RoutingRuleDto } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ActionLink from '$lib/components/ui/ActionLink.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';

	let rules = $state<RoutingRuleDto[]>([]);
	let fallback = $state('direct');
	let error = $state('');
	let loaded = $state(false);
	let busy = $state(false);

	async function load() {
		busy = true;
		error = '';
		try {
			const document = await getRouting();
			rules = document.rules;
			fallback = document.fallback;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('routing.loadFailed');
		} finally {
			busy = false;
			loaded = true;
		}
	}

	onMount(() => {
		void load();
	});
</script>

<div class="page-stack">
	<PageHeader title={t('routing.title')} description={t('routing.readOnlyDescription')} meta="generated / routing">
		{#snippet actions()}
			<Button
				variant="ghost"
				size="icon"
				icon={RefreshCw}
				disabled={busy}
				aria-label={t('common.reload')}
				title={t('common.reload')}
				onclick={() => void load()}
			/>
			<ActionLink class="orchestrate-action" href="/orchestrate" variant="primary" icon={ArrowRight}>
				{t('routing.openOrchestrate')}
			</ActionLink>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<Notice tone="info" message={t('routing.readOnlyNotice')} />

		<Section title={t('routing.defaultsTitle')} description={t('routing.generatedFallbackDescription')}>
			<div class="fallback-value"><span>{t('routing.fallback')}</span><strong>{fallback}</strong></div>
		</Section>

		<Section title={t('routing.rulesTitle')} description={t('routing.generatedRulesDescription')} count={rules.length}>
			{#if rules.length}
				<div class="rules-table" role="table" aria-label={t('routing.rulesTitle')}>
					<div class="rule-row rule-head" role="row">
						<span role="columnheader">#</span>
						<span role="columnheader">{t('routing.expression')}</span>
						<span role="columnheader">{t('routing.outbound')}</span>
					</div>
					{#each rules as rule, index (`${rule.expression}:${rule.outbound}:${index}`)}
						<div class:disabled={!rule.enabled} class="rule-row" role="row">
							<span class="rule-index" role="cell">{index + 1}</span>
							<span role="cell"><code>{rule.expression}</code></span>
							<span role="cell"><strong>{rule.outbound}</strong></span>
						</div>
					{/each}
				</div>
			{:else}
				<EmptyState icon={RouteIcon} title={t('routing.emptyDescription')} />
			{/if}
		</Section>
	{/if}
</div>

<style>
	.fallback-value {
		display: inline-flex;
		align-items: center;
		gap: var(--space-3);
		min-width: min(22rem, 100%);
		padding: var(--space-3) var(--space-4);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
	}

	.fallback-value span {
		color: var(--ink-muted);
		font-size: 0.72rem;
	}

	.fallback-value strong,
	.rule-row code,
	.rule-row strong {
		font-family: var(--font-mono);
		font-size: 0.75rem;
	}

	.rules-table {
		overflow: hidden;
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
	}

	.rule-row {
		display: grid;
		grid-template-columns: 3rem minmax(12rem, 1fr) minmax(8rem, 0.4fr);
		gap: var(--space-3);
		align-items: center;
		min-width: 34rem;
		padding: 0.8rem var(--space-4);
		border-top: 1px solid var(--line);
	}

	.rule-row:first-child { border-top: 0; }
	.rule-head { color: var(--ink-muted); background: var(--surface-subtle); font-size: 0.65rem; font-weight: 700; text-transform: uppercase; }
	.rule-index { color: var(--ink-faint); font-family: var(--font-mono); font-size: 0.7rem; }
	.rule-row code { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.rule-row.disabled { opacity: 0.45; }

	@media (max-width: 700px) {
		.rules-table { overflow-x: auto; }
		:global(.orchestrate-action .ui-command__label) { display: none; }
		:global(.orchestrate-action) { width: var(--control-md); min-width: var(--control-md); padding: 0; }
	}
</style>
