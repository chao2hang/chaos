<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import type { Connection } from '@xyflow/svelte';
	import {
		AlertTriangle,
		Boxes,
		Check,
		Layers3,
		LayoutGrid,
		ListFilter,
		LockKeyhole,
		Play,
		Plus,
		RadioTower,
		Redo2,
		RefreshCw,
		Save,
		Server,
		Undo2
	} from '@lucide/svelte';
	import {
		ApiClientError,
		getOrchestration,
		listGroups,
		listNodes,
		listSubscriptions,
		publishOrchestration,
		putOrchestration,
		isSessionRedirectPending,
		type GroupDto,
		type NodeDto,
		type OrchestrationDocument,
		type OrchestrationEdgeDto,
		type OrchestrationNodeDto,
		type OrchestrationNodeGroupData,
		type OrchestrationRuleData,
		type SubscriptionDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import {
		autoLayout,
		createGroupNode,
		createRuleNode,
		decorateDocument,
		ORCHESTRATION_VERSION,
		sanitizeDocument,
		setEndTarget,
		setRuleTarget,
		snapshotDocument,
		validateLocal
	} from '$lib/orchestration';
	import Button from '$lib/components/ui/Button.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import { toast } from '$lib/toast.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import SegmentedControl from '$lib/components/ui/SegmentedControl.svelte';
	import OrchestrationCanvas from '$lib/components/features/OrchestrationCanvas.svelte';
	import OrchestrationInspector from '$lib/components/features/OrchestrationInspector.svelte';

	type AddableNodeKind = 'rule' | 'node_group';

	let flowNodes = $state.raw<OrchestrationNodeDto[]>([]);
	let flowEdges = $state.raw<OrchestrationEdgeDto[]>([]);
	let viewport = $state({ x: 0, y: 0, zoom: 0.85 });
	let fitOnInit = $state(true);
	let inventoryNodes = $state<NodeDto[]>([]);
	let subscriptions = $state<SubscriptionDto[]>([]);
	let groups = $state<GroupDto[]>([]);
	let selectedNodeId = $state<string | null>(null);
	let selectedEdgeId = $state<string | null>(null);
	let loaded = $state(false);
	let busy = $state<'load' | 'save' | 'publish' | ''>('');
	let error = $state('');
	let message = $state('');
	let confirmReload = $state(false);
	let savedSnapshot = $state('');
	let undoStack = $state<string[]>([]);
	let redoStack = $state<string[]>([]);
	let pendingBefore = $state<string | null>(null);
	let lastHistoryKey = $state('');
	let lastHistoryAt = $state(0);
	let mobilePanel = $state<'canvas' | 'inspector'>('canvas');
	let canvasSettled = $state(false);
	let needsRepublish = $state(false);
	let validationToastSignature = '';

	function currentDocument(): OrchestrationDocument {
		return sanitizeDocument(flowNodes, flowEdges, viewport);
	}

	function currentSnapshot(): string {
		return snapshotDocument(currentDocument());
	}

	function setDocument(document: OrchestrationDocument) {
		const decorated = decorateDocument(document);
		flowNodes = decorated.nodes;
		flowEdges = decorated.edges;
		viewport = { ...decorated.viewport };
		selectedNodeId = null;
		selectedEdgeId = null;
	}

	function refreshDecoration() {
		const selectedNode = selectedNodeId;
		const selectedEdge = selectedEdgeId;
		const decorated = decorateDocument({
			version: ORCHESTRATION_VERSION,
			nodes: flowNodes,
			edges: flowEdges,
			viewport
		});
		flowNodes = decorated.nodes.map((node) => ({
			...node,
			selected: node.id === selectedNode
		}));
		flowEdges = decorated.edges.map((edge) => ({
			...edge,
			selected: edge.id === selectedEdge
		}));
	}

	function pushHistory(before: string, key = '') {
		if (before === currentSnapshot()) return;
		const now = Date.now();
		const merge = Boolean(key && key === lastHistoryKey && now - lastHistoryAt < 700);
		if (!merge) undoStack = [...undoStack.slice(-49), before];
		redoStack = [];
		lastHistoryKey = key;
		lastHistoryAt = now;
	}

	function mutate(mutator: () => void, historyKey = '') {
		const before = currentSnapshot();
		mutator();
		refreshDecoration();
		pushHistory(before, historyKey);
	}

	function beginCanvasChange() {
		pendingBefore ??= currentSnapshot();
	}

	function finishCanvasChange() {
		refreshDecoration();
		if (pendingBefore) pushHistory(pendingBefore);
		pendingBefore = null;
	}

	function settleInitialCanvas() {
		if (canvasSettled || typeof window === 'undefined') return;
		requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				if (canvasSettled) return;
				fitOnInit = false;
				savedSnapshot = currentSnapshot();
				undoStack = [];
				redoStack = [];
				canvasSettled = true;
			});
		});
	}

	function restoreSnapshot(snapshot: string) {
		const document = JSON.parse(snapshot) as OrchestrationDocument;
		setDocument(document);
		pendingBefore = null;
		lastHistoryKey = '';
	}

	function undo() {
		const previous = undoStack.at(-1);
		if (!previous) return;
		redoStack = [...redoStack.slice(-49), currentSnapshot()];
		undoStack = undoStack.slice(0, -1);
		restoreSnapshot(previous);
	}

	function redo() {
		const next = redoStack.at(-1);
		if (!next) return;
		undoStack = [...undoStack.slice(-49), currentSnapshot()];
		redoStack = redoStack.slice(0, -1);
		restoreSnapshot(next);
	}

	async function load() {
		busy = 'load';
		error = '';
		message = '';
		try {
			const [document, nodeResult, subscriptionResult, groupResult] = await Promise.all([
				getOrchestration(),
				listNodes(),
				listSubscriptions(),
				listGroups()
			]);
			inventoryNodes = nodeResult.nodes;
			subscriptions = subscriptionResult.subscriptions;
			groups = groupResult.groups.map((group) => ({ ...group, members: group.members ?? [] }));
			needsRepublish = document.needs_republish === true;
			setDocument(document);
			const requestedGroup = page.url.searchParams.get('group');
			if (requestedGroup) {
				const target = flowNodes.find(
					(node) =>
						node.type === 'node_group' &&
						(node.id === requestedGroup ||
							node.data.runtime_group_id === requestedGroup ||
							node.data.sources.some(
								(source) => source.kind === 'group' && source.id === requestedGroup
							))
				);
				if (target) setTimeout(() => selectNode(target.id), 0);
			}
			if (typeof window !== 'undefined') {
				fitOnInit = true;
			}
			savedSnapshot = currentSnapshot();
			undoStack = [];
			redoStack = [];
			confirmReload = false;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('flow.loadFailed');
		} finally {
			busy = '';
			loaded = true;
		}
	}

	onMount(() => {
		void load();
	});

	beforeNavigate(({ cancel }) => {
		if (!dirty || typeof window === 'undefined') return;
		if (isSessionRedirectPending()) return;
		if (sessionStorage.getItem('chaos_allow_dirty_navigation') === '1') {
			sessionStorage.removeItem('chaos_allow_dirty_navigation');
			return;
		}
		if (!window.confirm(t('common.discardDescription'))) cancel();
	});

	$effect(() => {
		if (typeof document === 'undefined') return;
		document.documentElement.dataset.chaosUnsaved = dirty ? 'true' : 'false';
		return () => {
			delete document.documentElement.dataset.chaosUnsaved;
		};
	});

	function requestReload() {
		if (dirty) confirmReload = true;
		else void load();
	}

	function nextRulePriority(): number {
		return (
			Math.max(
				0,
				...flowNodes
					.filter((node) => node.type === 'rule')
					.map((node) => node.data.priority ?? 0)
			) + 1
		);
	}

	function nextGroupIndex(): number {
		const names = new Set(
			flowNodes
				.filter((node) => node.type === 'node_group')
				.map((node) => node.data.name.trim().toLowerCase())
		);
		let index = flowNodes.filter((node) => node.type === 'node_group').length + 1;
		while (names.has(`proxy_${String(index).padStart(2, '0')}`)) index += 1;
		return index;
	}

	function addNodeAt(kind: AddableNodeKind, position: { x: number; y: number }) {
		let created: OrchestrationNodeDto | null = null;
		mutate(() => {
			created =
				kind === 'rule'
					? createRuleNode(position, nextRulePriority())
					: createGroupNode(position, nextGroupIndex());
			flowNodes = [...flowNodes, created];
			if (kind === 'rule' && created) {
				flowEdges = [
					...flowEdges,
					{ id: `start-${created.id}`, source: 'start', target: created.id }
				];
			}
			if (kind === 'node_group') {
				const directY = 70 + flowNodes.filter((node) => node.type === 'node_group').length * 180;
				flowNodes = flowNodes.map((node) =>
					node.type === 'builtin'
						? { ...node, position: { x: 520, y: directY } }
						: node
				);
			}
		}, `add:${kind}`);
		if (created) {
			const id = (created as OrchestrationNodeDto).id;
			selectNode(id);
			setTimeout(() => selectNode(id), 60);
		}
	}

	function addFromLibrary(kind: AddableNodeKind) {
		if (kind === 'rule') {
			addNodeAt(kind, { x: 280, y: 70 + ruleCount * 145 });
			return;
		}
		addNodeAt(kind, { x: 520, y: 70 + groupCount * 180 });
	}

	function updateRule(id: string, patch: Partial<OrchestrationRuleData>) {
		const key = Object.keys(patch)[0] ?? 'data';
		mutate(() => {
			flowNodes = flowNodes.map((node) =>
				node.id === id && node.type === 'rule'
					? { ...node, data: { ...node.data, ...patch } }
					: node
			);
		}, `rule:${id}:${key}`);
	}

	function updateGroup(id: string, patch: Partial<OrchestrationNodeGroupData>) {
		const key = Object.keys(patch)[0] ?? 'data';
		mutate(() => {
			flowNodes = flowNodes.map((node) =>
				node.id === id && node.type === 'node_group'
					? { ...node, data: { ...node.data, ...patch } }
					: node
			);
		}, `group:${id}:${key}`);
	}

	function updateRuleTarget(ruleId: string, targetId: string | null) {
		mutate(() => {
			flowEdges = setRuleTarget(ruleId, targetId, flowNodes, flowEdges);
		}, `target:${ruleId}`);
	}

	function updateEndTarget(targetId: string | null) {
		mutate(() => {
			flowEdges = setEndTarget(targetId, flowNodes, flowEdges);
		}, 'target:end');
	}

	function deleteElement(kind: 'node' | 'edge', id: string) {
		mutate(() => {
			if (kind === 'node') {
				const node = flowNodes.find((item) => item.id === id);
				if (!node || node.type === 'builtin' || node.type === 'start' || node.type === 'end') return;
				flowNodes = flowNodes.filter((item) => item.id !== id);
				flowEdges = flowEdges.filter((edge) => edge.source !== id && edge.target !== id);
			} else {
				flowEdges = flowEdges.filter((edge) => edge.id !== id);
			}
		}, `delete:${kind}`);
		selectedNodeId = null;
		selectedEdgeId = null;
	}

	function selectNode(id: string) {
		selectedNodeId = id;
		selectedEdgeId = null;
		flowNodes = flowNodes.map((node) => ({ ...node, selected: node.id === id }));
		flowEdges = flowEdges.map((edge) => ({ ...edge, selected: false }));
		if (typeof window !== 'undefined' && window.matchMedia('(max-width: 700px)').matches) {
			mobilePanel = 'inspector';
		}
	}

	function focusIssue() {
		const issue = validation.issues.find((item) => item.node_id || item.edge_id);
		if (!issue) return;
		if (issue.node_id) selectNode(issue.node_id);
		else if (issue.edge_id) {
			selectedNodeId = null;
			selectedEdgeId = issue.edge_id;
		}
	}

	function showValidationToast() {
		if (!validation.issues.length) return;
		toast.warning({
			id: 'flow-validation',
			title: t('flow.issueCount', { count: validation.issues.length }),
			description: t('flow.issueTrayHint'),
			duration: 8000,
			action: { label: t('flow.viewIssue'), onclick: focusIssue }
		});
	}

	function addConnection(connection: Connection) {
		if (!connection.source || !connection.target) return;
		if (connection.source === 'end') updateEndTarget(connection.target);
		else if (connection.source !== 'start') updateRuleTarget(connection.source, connection.target);
		else return;
		selectNode(connection.source);
	}

	function arrangeFlow() {
		const before = currentSnapshot();
		setDocument(autoLayout(currentDocument()));
		pushHistory(before, 'layout');
	}

	async function saveDraft() {
		error = '';
		message = '';
		busy = 'save';
		try {
			const saved = await putOrchestration(currentDocument());
			setDocument(saved);
			savedSnapshot = currentSnapshot();
			undoStack = [];
			redoStack = [];
			message = t('flow.draftSaved');
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('flow.saveFailed');
		} finally {
			busy = '';
		}
	}

	async function publishAndApply() {
		error = '';
		message = '';
		if (!validation.dae_compatible) {
			error = validation.issues
				.slice(0, 6)
				.map((issue) => t(`flow.validation.${issue.code}`))
				.join('\n');
			return;
		}

		busy = 'publish';
		try {
			const result = await publishOrchestration(currentDocument());
			setDocument(result.document);
			savedSnapshot = currentSnapshot();
			undoStack = [];
			redoStack = [];
			groups = (await listGroups()).groups.map((group) => ({
				...group,
				members: group.members ?? []
			}));
			message = t('flow.applied', { nodes: result.applied.nodes });
			needsRepublish = false;
		} catch (cause) {
			if (cause instanceof ApiClientError && cause.draftSaved) {
				savedSnapshot = currentSnapshot();
				undoStack = [];
				redoStack = [];
				message = t('flow.draftSavedApplyFailed');
			}
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('flow.saveFailed');
		} finally {
			busy = '';
		}
	}

	function onBeforeUnload(event: BeforeUnloadEvent) {
		if (!dirty || isSessionRedirectPending()) return;
		event.preventDefault();
		event.returnValue = '';
	}

	function onPaletteDragStart(event: DragEvent, kind: AddableNodeKind) {
		event.dataTransfer?.setData('application/chaos-flow-node', kind);
		if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
	}

	const selectedNode = $derived(flowNodes.find((node) => node.id === selectedNodeId) ?? null);
	const selectedEdge = $derived(flowEdges.find((edge) => edge.id === selectedEdgeId) ?? null);
	const validation = $derived(
		validateLocal(currentDocument(), { nodes: inventoryNodes, subscriptions, groups })
	);
	const graphIssues = $derived(validation.issues.filter((issue) => issue.scope === 'graph'));
	const runtimeIssues = $derived(validation.issues.filter((issue) => issue.scope === 'runtime'));
	const dirty = $derived(loaded && canvasSettled && currentSnapshot() !== savedSnapshot);
	const groupCount = $derived(flowNodes.filter((node) => node.type === 'node_group').length);
	const ruleCount = $derived(flowNodes.filter((node) => node.type === 'rule').length);
	const directNode = $derived(
		flowNodes.find((node) => node.type === 'builtin' && node.data.builtin === 'direct') ?? null
	);
</script>

<svelte:window onbeforeunload={onBeforeUnload} />

<AppPage variant="editor">
	<PageHeader title={t('flow.title')} meta="FLOW / V4">
		{#snippet actions()}
			<div class="runtime-state" class:blocked={!validation.dae_compatible}>
				{#if validation.dae_compatible}
					<Check size={14} strokeWidth={2} aria-hidden="true" />
				{:else}
					<AlertTriangle size={14} strokeWidth={1.8} aria-hidden="true" />
				{/if}
				<span>{validation.dae_compatible ? t('flow.runtime.ready') : t('flow.runtime.blocked')}</span>
			</div>
			<Button
				variant="ghost"
				size="icon"
				icon={RefreshCw}
				disabled={!!busy}
				aria-label={t('common.reload')}
				title={t('common.reload')}
				onclick={requestReload}
			/>
			<Button
				icon={Save}
				loading={busy === 'save'}
				disabled={!dirty || !!busy}
				onclick={() => void saveDraft()}
			>
				{t('flow.saveDraft')}
			</Button>
			<Button
				variant="primary"
				icon={Play}
				loading={busy === 'publish'}
				disabled={!validation.dae_compatible || !!busy}
				title={!validation.dae_compatible ? t('flow.runtime.applyDisabled') : t('flow.publishApply')}
				data-testid="publish-orchestration"
				onclick={() => void publishAndApply()}
			>
				{t('flow.publishApply')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}
	{#if needsRepublish}<Notice message={t('flow.needsRepublish')} />{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
		{:else}
			<div class="mobile-panel-switch">
				<SegmentedControl
					bind:value={mobilePanel}
					label={t('flow.mobilePanel')}
					options={[
						{ value: 'canvas', label: t('flow.canvas.title') },
						{ value: 'inspector', label: t('flow.inspector.title') }
					]}
				/>
			</div>
			<div class:show-inspector={mobilePanel === 'inspector'} class="editor-shell">
			<aside class="node-library" aria-label={t('flow.library.title')}>
				<header>
					<span>{t('flow.library.title')}</span>
					<strong>{t('flow.library.v4Document')}</strong>
				</header>

				<section class="library-section">
					<div class="library-heading">
						<span>{t('flow.library.available')}</span>
						<em>{ruleCount + groupCount}</em>
					</div>
					<div class="palette-list">
						<button
							type="button"
							class="palette-node"
							draggable="true"
							data-testid="add-rule"
							ondragstart={(event) => onPaletteDragStart(event, 'rule')}
							onclick={() => addFromLibrary('rule')}
						>
							<ListFilter size={17} strokeWidth={1.8} aria-hidden="true" />
							<span><strong>{t('flow.node.rule')}</strong><small>RULE</small></span>
							<Plus size={14} strokeWidth={1.8} aria-hidden="true" />
						</button>
						<button
							type="button"
							class="palette-node"
							draggable="true"
							data-testid="add-group"
							ondragstart={(event) => onPaletteDragStart(event, 'node_group')}
							onclick={() => addFromLibrary('node_group')}
						>
							<Boxes size={17} strokeWidth={1.8} aria-hidden="true" />
							<span><strong>{t('flow.node.group')}</strong><small>NODE GROUP</small></span>
							<Plus size={14} strokeWidth={1.8} aria-hidden="true" />
						</button>
					</div>
				</section>

				<section class="library-section fixed-section">
					<div class="library-heading"><span>{t('flow.library.fixed')}</span></div>
					<button
						type="button"
						class="fixed-outbound"
						disabled={!directNode}
						onclick={() => directNode && selectNode(directNode.id)}
					>
						<LockKeyhole size={16} strokeWidth={1.8} aria-hidden="true" />
						<span><strong>DIRECT</strong><small>{t('flow.direct.fallback')}</small></span>
					</button>
				</section>

				<section class="library-section resource-summary">
					<div class="library-heading"><span>{t('flow.library.resources')}</span></div>
					<dl>
						<div><dt><Server size={13} strokeWidth={1.8} />{t('flow.source.nodes')}</dt><dd>{inventoryNodes.length}</dd></div>
						<div><dt><RadioTower size={13} strokeWidth={1.8} />{t('flow.source.subscriptions')}</dt><dd>{subscriptions.length}</dd></div>
						<div><dt><Layers3 size={13} strokeWidth={1.8} />{t('flow.source.groups')}</dt><dd>{groups.length}</dd></div>
					</dl>
				</section>

				<footer class:blocked={runtimeIssues.length > 0}>
					<span>DATA PLANE</span>
					<strong>DIRECT FALLBACK</strong>
				</footer>
			</aside>

			<section class="canvas-column">
				<div class="canvas-toolbar">
					<div class="canvas-meta">
						<span>{t('flow.canvas.title')}</span>
						<strong>{t('flow.canvas.summary', { rules: ruleCount, groups: groupCount })}</strong>
					</div>
					<div class="toolbar-actions">
						<Button variant="ghost" size="icon" icon={Undo2} disabled={!undoStack.length || !!busy} aria-label={t('flow.undo')} title={t('flow.undo')} onclick={undo} />
						<Button variant="ghost" size="icon" icon={Redo2} disabled={!redoStack.length || !!busy} aria-label={t('flow.redo')} title={t('flow.redo')} onclick={redo} />
						<span class="toolbar-divider"></span>
						<Button variant="ghost" size="icon" icon={LayoutGrid} disabled={!!busy} aria-label={t('flow.autoLayout')} title={t('flow.autoLayout')} onclick={arrangeFlow} />
						<Button variant="ghost" size="icon" icon={ListFilter} disabled={!!busy} aria-label={t('flow.addRule')} title={t('flow.addRule')} onclick={() => addFromLibrary('rule')} />
						<Button variant="ghost" size="icon" icon={Plus} disabled={!!busy} aria-label={t('flow.addGroup')} title={t('flow.addGroup')} onclick={() => addFromLibrary('node_group')} />
					</div>
				</div>

				<div class="canvas-body">
					<OrchestrationCanvas
						bind:nodes={flowNodes}
						bind:edges={flowEdges}
						bind:viewport
						{fitOnInit}
						bind:selectedNodeId
						bind:selectedEdgeId
						onaddnode={addNodeAt}
						onconnectrequest={addConnection}
						onselectnode={selectNode}
						onready={settleInitialCanvas}
						onbeforechange={beginCanvasChange}
						onchange={finishCanvasChange}
					/>
				</div>

				<footer class="canvas-status">
					<div><span class:active={dirty}></span>{dirty ? t('common.unsavedChanges') : t('flow.draftSavedState')}</div>
					<div>{validation.issues.length ? t('flow.issueCount', { count: validation.issues.length }) : t('flow.validationPassed')}</div>
					<div>ZOOM {Math.round(viewport.zoom * 100)}%</div>
				</footer>
			</section>

			<OrchestrationInspector
				node={selectedNode}
				edge={selectedEdge}
				flowNodes={flowNodes}
				flowEdges={flowEdges}
				{inventoryNodes}
				{subscriptions}
				{groups}
				issues={validation.issues}
				busy={!!busy}
				onupdaterule={updateRule}
				onupdategroup={updateGroup}
				onsettarget={updateRuleTarget}
				onsetendtarget={updateEndTarget}
				ondelete={deleteElement}
				onselectnode={selectNode}
			/>
		</div>
	{/if}
</AppPage>

<ConfirmDialog
	bind:open={confirmReload}
	title={t('common.discardTitle')}
	description={t('common.discardDescription')}
	confirmLabel={t('common.discard')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={load}
/>

<style>


	.runtime-state {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		min-height: 2.25rem;
		padding: 0 0.65rem;
		border-left: 1px solid var(--line-strong);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.62rem;
		font-weight: 700;
	}

	.runtime-state.blocked {
		color: var(--ink);
		text-decoration: underline dotted;
	}

	.editor-shell {
		display: grid;
		grid-template-columns: 13.5rem minmax(30rem, 1fr) 20rem;
		height: max(44rem, calc(100vh - 11.5rem));
		min-height: 0;
		border: 1px solid var(--ink);
		border-radius: var(--radius-lg);
		background: var(--surface);
		overflow: hidden;
	}

	.mobile-panel-switch { display: none; }

	.node-library {
		display: flex;
		min-width: 0;
		min-height: 0;
		flex-direction: column;
		border-right: 1px solid var(--line);
		background: var(--surface);
		overflow-y: auto;
	}

	.node-library > header {
		min-height: 4rem;
		padding: 0.7rem var(--space-3);
		border-bottom: 1px solid var(--ink);
	}

	.node-library > header span,
	.node-library > header strong {
		display: block;
	}

	.node-library > header span,
	.library-heading span,
	.node-library > footer span {
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.57rem;
		font-weight: 700;
		text-transform: uppercase;
	}

	.node-library > header strong {
		margin-top: 0.15rem;
		font-size: 0.78rem;
	}

	.library-section {
		padding: var(--space-3);
		border-bottom: 1px solid var(--line);
	}

	.library-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}

	.library-heading em {
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.6rem;
		font-style: normal;
	}

	.palette-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.palette-node,
	.fixed-outbound {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: 3.5rem;
		padding: 0.55rem 0.6rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		color: var(--ink);
		text-align: left;
	}

	.palette-node:hover,
	.fixed-outbound:hover:not(:disabled) {
		border-color: var(--ink);
		background: var(--surface-subtle);
	}

	.palette-node strong,
	.palette-node small,
	.fixed-outbound strong,
	.fixed-outbound small {
		display: block;
	}

	.palette-node strong,
	.fixed-outbound strong {
		font-size: 0.72rem;
	}

	.palette-node small,
	.fixed-outbound small {
		margin-top: 0.06rem;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.52rem;
	}

	.fixed-outbound {
		grid-template-columns: auto minmax(0, 1fr);
		background: var(--surface-subtle);
	}

	.resource-summary dl {
		margin: 0;
	}

	.resource-summary dl div {
		display: flex;
		align-items: center;
		justify-content: space-between;
		min-height: 2rem;
		border-bottom: 1px solid var(--line);
	}

	.resource-summary dl div:last-child {
		border-bottom: 0;
	}

	.resource-summary dt {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		color: var(--ink-muted);
		font-size: 0.66rem;
	}

	.resource-summary dd {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 0.64rem;
		font-weight: 700;
	}

	.node-library > footer {
		margin-top: auto;
		padding: var(--space-3);
		border-top: 1px solid var(--line);
	}

	.node-library > footer strong {
		display: block;
		margin-top: 0.12rem;
		font-size: 0.68rem;
	}

	.node-library > footer.blocked {
		background: var(--danger-surface);
	}

	.canvas-column {
		display: grid;
		min-width: 0;
		min-height: 0;
		grid-template-rows: 4rem minmax(0, 1fr) 2.2rem;
	}

	.canvas-toolbar,
	.canvas-status {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: 0 var(--space-3);
		background: var(--surface);
	}

	.canvas-toolbar {
		border-bottom: 1px solid var(--ink);
	}

	.canvas-meta span,
	.canvas-meta strong {
		display: block;
	}

	.canvas-meta span {
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.57rem;
		font-weight: 700;
	}

	.canvas-meta strong {
		margin-top: 0.1rem;
		font-family: var(--font-mono);
		font-size: 0.64rem;
	}

	.toolbar-actions {
		display: flex;
		align-items: center;
		gap: 0.1rem;
	}

	.toolbar-divider {
		width: 1px;
		height: 1.5rem;
		margin: 0 var(--space-1);
		background: var(--line);
	}

	.canvas-body {
		min-width: 0;
		min-height: 0;
	}

	.canvas-status {
		border-top: 1px solid var(--line);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.57rem;
	}

	.canvas-status > div:first-child {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.canvas-status > div:first-child span {
		width: 0.42rem;
		height: 0.42rem;
		border: 1px solid var(--line-strong);
		border-radius: 50%;
	}

	.canvas-status > div:first-child span.active {
		border-color: var(--ink);
		background: var(--ink);
	}

	@media (max-width: 1160px) {
		.editor-shell {
			grid-template-columns: 12rem minmax(27rem, 1fr) 18rem;
		}
	}

	@media (max-width: 980px) {
		.editor-shell {
			grid-template-columns: 11.5rem minmax(0, 1fr);
			height: auto;
			min-height: 42rem;
		}

		.editor-shell :global(.inspector) {
			grid-column: 1 / -1;
			max-height: 34rem;
			border-top: 1px solid var(--ink);
			border-left: 0;
		}

		.canvas-column {
			height: 42rem;
		}
	}

	@media (max-width: 700px) {
		.mobile-panel-switch { display: block; }
		.mobile-panel-switch :global(.segments) { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); width: 100%; }
		.runtime-state {
			order: 10;
			width: 100%;
			border-top: 1px solid var(--line);
			border-left: 0;
		}

		.editor-shell {
			display: block;
			border-inline: 0;
			border-radius: 0;
		}

		.editor-shell:not(.show-inspector) :global(.inspector) { display: none; }
		.editor-shell.show-inspector .canvas-column { display: none; }
		.editor-shell.show-inspector :global(.inspector) { display: flex; max-height: min(44rem, 72vh); border-top: 0; }

		.node-library {
			border-right: 0;
			border-bottom: 1px solid var(--ink);
		}

		.node-library > header,
		.resource-summary,
		.node-library > footer {
			display: none;
		}

		.node-library .library-section {
			border-bottom: 0;
		}

		.node-library .fixed-section {
			display: none;
		}

		.palette-list {
			display: grid;
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.canvas-column {
			height: 38rem;
		}

		.canvas-status > div:nth-child(2) {
			display: none;
		}
	}
</style>
