<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { interceptDirtyNavigation } from '$lib/dirtyNavigation.svelte';
	import {
		Copy,
		ExternalLink,
		Plus,
		Route as RouteIcon,
		Trash2
	} from '@lucide/svelte';
	import {
		ApiClientError,
		getOrchestration,
		isSessionRedirectPending,
		putOrchestration,
		type OrchestrationDocument,
		type OrchestrationEdgeDto,
		type OrchestrationNodeDto,
		type OrchestrationRuleMatcher,
		type OrchestrationRuleNodeDto
	} from '$lib/api';
	import {
		createRuleNode,
		decorateDocument,
		parseDomainList,
		sanitizeDocument,
		serializeDomainList,
		setEndTarget,
		setRuleTarget,
		snapshotDocument
	} from '$lib/orchestration';
	import { GEOIP_OPTIONS, GEOSITE_OPTIONS } from '$lib/geoOptions';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import { toast } from '$lib/toast.svelte';
	import ActionLink from '$lib/components/ui/ActionLink.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
import Section from '$lib/components/ui/Section.svelte';
		import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';

		type RuleNode = OrchestrationRuleNodeDto;
	type OutboundNode = Extract<OrchestrationNodeDto, { type: 'node_group' | 'builtin' }>;

	const matcherKinds: OrchestrationRuleMatcher['kind'][] = [
		'domain_suffix',
		'destination_cidr',
		'geosite',
		'geoip'
	];

	let flowNodes = $state.raw<OrchestrationNodeDto[]>([]);
	let flowEdges = $state.raw<OrchestrationEdgeDto[]>([]);
	let viewport = $state({ x: 0, y: 0, zoom: 0.85 });
	let loaded = $state(false);
	let busy = $state(false);
	let savedSnapshot = $state('');
	let confirmDeleteId = $state<string | null>(null);

	function currentDocument(): OrchestrationDocument {
		return sanitizeDocument(flowNodes, flowEdges, viewport);
	}

	function currentSnapshot(): string {
		return snapshotDocument(currentDocument());
	}

	function ruleNodes(): RuleNode[] {
		return flowNodes
			.filter((node): node is RuleNode => node.type === 'rule')
			.slice()
			.sort((a, b) => (a.data.priority ?? 9999) - (b.data.priority ?? 9999));
	}

	function outboundNodes(): OutboundNode[] {
		return flowNodes.filter(
			(node): node is OutboundNode => node.type === 'node_group' || node.type === 'builtin'
		);
	}

	function outboundName(id: string | null | undefined): string {
		if (!id) return t('flow.rule.noTarget');
		const node = flowNodes.find((item) => item.id === id);
		if (!node) return id;
		if (node.type === 'builtin') return 'DIRECT';
		if (node.type === 'node_group') return node.data.name || t('flow.node.unnamedGroup');
		return id;
	}

	function ruleTargetId(ruleId: string): string {
		return flowEdges.find((edge) => edge.source === ruleId)?.target ?? '';
	}

	function fallbackTargetId(): string {
		return flowEdges.find((edge) => edge.source === 'end')?.target ?? '';
	}

	function matcherLabel(kind: string): string {
		const map: Record<string, string> = {
			domain_suffix: t('flow.matcher.domainSuffix'),
			destination_cidr: t('flow.matcher.destinationCidr'),
			geosite: t('flow.matcher.geosite'),
			geoip: t('flow.matcher.geoip')
		};
		return map[kind] ?? kind;
	}

	async function load() {
		busy = true;
		try {
			const orchestration = await getOrchestration();
			const decorated = decorateDocument({
				version: orchestration.version,
				nodes: orchestration.nodes,
				edges: orchestration.edges,
				viewport: orchestration.viewport
			});
			flowNodes = decorated.nodes;
			flowEdges = decorated.edges;
			viewport = decorated.viewport;
			savedSnapshot = currentSnapshot();
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('routing.loadFailed')
			});
		} finally {
			busy = false;
			loaded = true;
		}
	}

	onMount(() => {
		void load();
	});

	const dirty = $derived(loaded && currentSnapshot() !== savedSnapshot);

	beforeNavigate(({ cancel, to }) => {
		if (!dirty || typeof window === 'undefined') return;
		if (isSessionRedirectPending()) return;
		interceptDirtyNavigation({
			dirty: true,
			cancel,
			href: to?.url.href ?? null
		});
	});

	$effect(() => {
		if (typeof document === 'undefined') return;
		document.documentElement.dataset.chaosUnsaved = dirty ? 'true' : 'false';
		return () => {
			delete document.documentElement.dataset.chaosUnsaved;
		};
	});

	function nextPriority(): number {
		return Math.max(0, ...ruleNodes().map((node) => node.data.priority ?? 0)) + 1;
	}

	function updateRule(id: string, patch: Partial<RuleNode['data']>) {
		flowNodes = flowNodes.map((node) =>
			node.id === id && node.type === 'rule'
				? { ...node, data: { ...node.data, ...patch } }
				: node
		);
	}

	function patchMatcher(id: string, patch: Partial<OrchestrationRuleMatcher>) {
		const node = flowNodes.find((item) => item.id === id && item.type === 'rule');
		if (!node || node.type !== 'rule') return;
		updateRule(id, { matcher: { ...node.data.matcher, ...patch } });
	}

	function addRule() {
		const created = createRuleNode({ x: 280, y: 70 + ruleNodes().length * 145 }, nextPriority());
		flowNodes = [...flowNodes, created];
		flowEdges = [
			...flowEdges,
			{ id: `start-${created.id}`, source: 'start', target: created.id }
		];
	}

	function duplicateRule(rule: RuleNode) {
		const created = createRuleNode(
			{ x: rule.position.x, y: rule.position.y + 40 },
			nextPriority(),
			rule.data.matcher.kind
		);
		created.data = {
			...created.data,
			matcher: { ...rule.data.matcher },
			priority: nextPriority()
		};
		flowNodes = [...flowNodes, created];
		const target = ruleTargetId(rule.id);
		flowEdges = [
			...flowEdges,
			{ id: `start-${created.id}`, source: 'start', target: created.id }
		];
		if (target) flowEdges = setRuleTarget(created.id, target, flowNodes, flowEdges);
	}

function requestRemoveRule(id: string) {
			confirmDeleteId = id;
		}

		function confirmRemoveRule() {
			const id = confirmDeleteId;
			confirmDeleteId = null;
			if (!id) return;
			flowNodes = flowNodes.filter((node) => node.id !== id);
			flowEdges = flowEdges.filter((edge) => edge.source !== id && edge.target !== id);
		}

	function setTarget(ruleId: string, targetId: string) {
		flowEdges = setRuleTarget(ruleId, targetId || null, flowNodes, flowEdges);
	}

	function setFallback(targetId: string) {
		flowEdges = setEndTarget(targetId || null, flowNodes, flowEdges);
	}

	async function saveDraft() {
		busy = true;
		try {
			const saved = await putOrchestration(currentDocument());
			const decorated = decorateDocument(saved);
			flowNodes = decorated.nodes;
			flowEdges = decorated.edges;
			viewport = decorated.viewport;
			savedSnapshot = currentSnapshot();
			toast.success({ title: t('routing.draftSaved') });
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('routing.saveFailed')
			});
		} finally {
			busy = false;
		}
	}

	const rules = $derived(ruleNodes());
	const outbounds = $derived(outboundNodes());
</script>

<AppPage>
	<PageHeader title={t('routing.title')} description={t('routing.subtitle')} meta="routing / draft">
		{#snippet actions()}
			{#if dirty}<span class="dirty-pill">{t('common.unsavedChanges')}</span>{/if}
			<ActionLink variant="ghost" icon={ExternalLink} href="/orchestrate">
				{t('routing.openOrchestrate')}
			</ActionLink>
			<Button variant="ghost" icon={Plus} disabled={busy} onclick={addRule}>
				{t('routing.addRule')}
			</Button>
			<Button variant="primary" loading={busy} disabled={!dirty} onclick={() => void saveDraft()}>
				{t('routing.saveDraft')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<p class="page-hint">{t('routing.editNotice')}</p>

		<Section title={t('routing.defaultsTitle')} description={t('routing.generatedFallbackDescription')}>
			<Field label={t('routing.fallback')} forId="routing-fallback">
				<select
					id="routing-fallback"
					value={fallbackTargetId()}
					disabled={busy}
					onchange={(event) => setFallback((event.currentTarget as HTMLSelectElement).value)}
				>
					<option value="">{t('flow.rule.noTarget')}</option>
					{#each outbounds as target (target.id)}
						<option value={target.id}>{outboundName(target.id)}</option>
					{/each}
				</select>
			</Field>
		</Section>

		<Section
			title={t('routing.rulesTitle')}
			description={t('routing.rulesDescription')}
			count={rules.length}
		>
			{#if rules.length}
				<div class="rules-editor">
					{#each rules as rule, index (rule.id)}
						<article class="rule-card">
							<header>
								<span>#{rule.data.priority ?? index + 1}</span>
								<div class="rule-card-actions">
									<Button
										variant="ghost"
										size="icon"
										icon={Copy}
										disabled={busy}
										aria-label={t('routing.duplicateRule')}
										title={t('routing.duplicateRule')}
										onclick={() => duplicateRule(rule)}
									/>
									<Button
										variant="ghost"
										size="icon"
										icon={Trash2}
										disabled={busy}
										aria-label={t('routing.deleteRule', { index: index + 1 })}
										title={t('routing.deleteRule', { index: index + 1 })}
										onclick={() => requestRemoveRule(rule.id)}
									/>
								</div>
							</header>
							<div class="rule-grid">
								<Field label={t('flow.rule.matchType')} forId={`rule-kind-${rule.id}`}>
									<select
										id={`rule-kind-${rule.id}`}
										value={rule.data.matcher.kind}
										disabled={busy}
										onchange={(event) =>
											patchMatcher(rule.id, {
												kind: (event.currentTarget as HTMLSelectElement)
													.value as OrchestrationRuleMatcher['kind'],
												pattern: ''
											})}
									>
										{#each matcherKinds as kind}
											<option value={kind}>{matcherLabel(kind)}</option>
										{/each}
									</select>
								</Field>
								<Field label={t('flow.rule.priority')} forId={`rule-priority-${rule.id}`}>
									<input
										id={`rule-priority-${rule.id}`}
										type="number"
										min="1"
										max="9999"
										value={rule.data.priority ?? 1}
										disabled={busy}
										onchange={(event) =>
											updateRule(rule.id, {
												priority: Math.min(
													9999,
													Math.max(
														1,
														Math.floor(
															Number((event.currentTarget as HTMLInputElement).value) || 1
														)
													)
												)
											})}
									/>
								</Field>
								{#if rule.data.matcher.kind === 'geosite' || rule.data.matcher.kind === 'geoip'}
									<Field
										label={t('flow.rule.pattern')}
										forId={`rule-pattern-${rule.id}`}
										hint={rule.data.matcher.kind === 'geosite'
											? t('flow.matcher.geositeHint')
											: t('flow.matcher.geoipHint')}
									>
										<select
											id={`rule-pattern-${rule.id}`}
											value={rule.data.matcher.pattern.split(/[\s,]+/)[0] ?? ''}
											disabled={busy}
											onchange={(event) =>
												patchMatcher(rule.id, {
													pattern: (event.currentTarget as HTMLSelectElement).value
												})}
										>
											<option value="">{t('flow.matcher.selectCodes')}</option>
											{#each rule.data.matcher.kind === 'geosite' ? GEOSITE_OPTIONS : GEOIP_OPTIONS as option}
												<option value={option.code}>
													{t(option.labelKey) === option.labelKey
														? option.code
														: `${t(option.labelKey)} (${option.code})`}
												</option>
											{/each}
										</select>
									</Field>
								{:else if rule.data.matcher.kind === 'domain_suffix'}
									<Field
										label={t('flow.rule.domains')}
										forId={`rule-pattern-${rule.id}`}
										hint={t('flow.rule.domainsHint')}
									>
										<textarea
											id={`rule-pattern-${rule.id}`}
											rows="3"
											disabled={busy}
											placeholder={"google.com\nyoutube.com\nfast.com"}
											value={(rule.data.matcher.pattern || '')
												.split(/[\s,;]+/)
												.filter(Boolean)
												.join('\n')}
											oninput={(event) =>
												patchMatcher(rule.id, {
													pattern: serializeDomainList(
														parseDomainList((event.currentTarget as HTMLTextAreaElement).value)
													)
												})}
										></textarea>
									</Field>
								{:else}
									<Field label={t('flow.rule.pattern')} forId={`rule-pattern-${rule.id}`}>
										<input
											id={`rule-pattern-${rule.id}`}
											type="text"
											value={rule.data.matcher.pattern}
											disabled={busy}
											placeholder="192.0.2.0/24"
											oninput={(event) =>
												patchMatcher(rule.id, {
													pattern: (event.currentTarget as HTMLInputElement).value
												})}
										/>
									</Field>
								{/if}
								<Field label={t('routing.outbound')} forId={`rule-outbound-${rule.id}`}>
									<select
										id={`rule-outbound-${rule.id}`}
										value={ruleTargetId(rule.id)}
										disabled={busy}
										onchange={(event) =>
											setTarget(rule.id, (event.currentTarget as HTMLSelectElement).value)}
									>
										<option value="">{t('flow.rule.noTarget')}</option>
										{#each outbounds as target (target.id)}
											<option value={target.id}>{outboundName(target.id)}</option>
										{/each}
									</select>
								</Field>
							</div>
						</article>
					{/each}
				</div>
			{:else}
				<EmptyState icon={RouteIcon} title={t('routing.empty')} description={t('routing.emptyDescription')}>
					{#snippet actions()}
						<Button variant="primary" icon={Plus} onclick={addRule}>{t('routing.addRule')}</Button>
					{/snippet}
				</EmptyState>
			{/if}
		</Section>
	{/if}
</AppPage>

<ConfirmDialog
	open={confirmDeleteId !== null}
	title={t('routing.deleteRuleTitle')}
	description={t('routing.deleteRuleDescription')}
	confirmLabel={t('common.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={confirmRemoveRule}
	oncancel={() => {
		confirmDeleteId = null;
	}}
/>

<style>
	.dirty-pill {
		display: inline-flex;
		align-items: center;
		min-height: 2rem;
		padding: 0 0.65rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.62rem;
		font-weight: 700;
	}
	.page-hint {
		margin: 0 0 var(--space-3);
		color: var(--ink-muted);
		font-size: 0.78rem;
		line-height: 1.45;
	}
	.rules-editor {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.rule-card {
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface);
		overflow: hidden;
	}
	.rule-card header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.55rem 0.75rem;
		border-bottom: 1px solid var(--line);
		background: var(--surface-subtle);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.7rem;
		font-weight: 700;
	}
	.rule-card-actions {
		display: inline-flex;
		gap: 0.1rem;
	}
	.rule-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
		padding: var(--space-3);
	}
	.rule-grid :global(textarea) {
		width: 100%;
		min-height: 4.5rem;
		padding: 0.55rem 0.7rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink);
		font-family: var(--font-mono);
		font-size: 0.78rem;
		resize: vertical;
	}
	@media (max-width: 800px) {
		.rule-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
