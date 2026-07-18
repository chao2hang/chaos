<script lang="ts">
	import { onMount } from 'svelte';
	import {
		SvelteFlow,
		Background,
		Controls,
		MiniMap,
		BackgroundVariant,
		MarkerType,
		type Node,
		type Edge,
		type Connection,
		type IsValidConnection
	} from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';

	import StartNode from '$lib/flow/StartNode.svelte';
	import ProxyNode from '$lib/flow/ProxyNode.svelte';
	import GroupNode from '$lib/flow/GroupNode.svelte';
	import RuleNode from '$lib/flow/RuleNode.svelte';
	import FallbackNode from '$lib/flow/FallbackNode.svelte';
	import BuiltinNode from '$lib/flow/BuiltinNode.svelte';

	import {
		listNodes,
		listGroups,
		createGroup,
		addGroupMember,
		removeGroupMember,
		setGroupMemberWeight,
		getRouting,
		putRouting,
		ApiClientError,
		type NodeDto,
		type GroupDto,
		type RoutingRuleDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';

	const nodeTypes = {
		start: StartNode,
		proxy: ProxyNode,
		group: GroupNode,
		rule: RuleNode,
		fallback: FallbackNode,
		builtin: BuiltinNode
	};

	let nodes = $state.raw<Node[]>([]);
	let edges = $state.raw<Edge[]>([]);
	let apiNodes = $state<NodeDto[]>([]);
	let apiGroups = $state<GroupDto[]>([]);
	let apiRules = $state<RoutingRuleDto[]>([]);
	let fallback = $state('proxy');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);
	let newGroupName = $state('');
	let newPolicy = $state('fixed');
	let dirty = $state(false);

	function rebuildGraph(
		proxyNodes: NodeDto[],
		groups: GroupDto[],
		rules: RoutingRuleDto[],
		fb: string
	) {
		const n: Node[] = [];
		const e: Edge[] = [];

		n.push({
			id: 'start',
			type: 'start',
			position: { x: 40, y: 220 },
			data: { label: 'Traffic' },
			draggable: true
		});

		// Proxy pool (left column)
		proxyNodes.forEach((pn, i) => {
			n.push({
				id: `proxy:${pn.id}`,
				type: 'proxy',
				position: { x: 40, y: 320 + i * 88 },
				data: {
					name: pn.name,
					protocol: pn.protocol,
					address: pn.address,
					nodeId: pn.id
				}
			});
		});

		// Groups (middle)
		groups.forEach((g, i) => {
			n.push({
				id: `group:${g.id}`,
				type: 'group',
				position: { x: 320, y: 80 + i * 200 },
				data: {
					name: g.name,
					policy: g.policy,
					groupId: g.id,
					members: (g.members ?? []).map((m) => ({
						node_id: m.node_id,
						name: m.name,
						weight: m.weight
					})),
					onWeight: (nodeId: string, weight: number) => {
						void handleWeight(g.id, nodeId, weight);
					},
					onRemove: (nodeId: string) => {
						void handleRemoveMember(g.id, nodeId);
					}
				}
			});

			// proxy → group membership edges
			for (const m of g.members ?? []) {
				e.push({
					id: `mem:${g.id}:${m.node_id}`,
					source: `proxy:${m.node_id}`,
					target: `group:${g.id}`,
					sourceHandle: 'out',
					targetHandle: 'in',
					animated: true,
					style: 'stroke: #22c55e; stroke-width: 1.5',
					markerEnd: { type: MarkerType.ArrowClosed, color: '#22c55e' },
					label: `w${m.weight}`,
					data: { kind: 'member', groupId: g.id, nodeId: m.node_id }
				});
			}
		});

		// Builtin outbounds
		const builtins = ['direct', 'must_direct', 'block'];
		builtins.forEach((name, i) => {
			n.push({
				id: `builtin:${name}`,
				type: 'builtin',
				position: { x: 320, y: 80 + (groups.length + i) * 100 + 40 },
				data: { name }
			});
		});

		// Rules chain
		rules.forEach((r, i) => {
			const rid = `rule:${i}`;
			n.push({
				id: rid,
				type: 'rule',
				position: { x: 620, y: 60 + i * 160 },
				data: {
					expression: r.expression,
					outbound: r.outbound,
					enabled: r.enabled,
					index: i,
					onChange: (patch: { expression?: string; enabled?: boolean }) => {
						const next = [...apiRules];
						next[i] = { ...next[i], ...patch };
						apiRules = next;
						dirty = true;
						// update node label fields without full rebuild
						nodes = nodes.map((nd) =>
							nd.id === rid
								? {
										...nd,
										data: {
											...nd.data,
											expression: next[i].expression,
											enabled: next[i].enabled
										}
									}
								: nd
						);
					}
				}
			});

			// start → first rule, rule → next rule
			if (i === 0) {
				e.push({
					id: 'flow:start-rule0',
					source: 'start',
					target: rid,
					sourceHandle: 'out',
					targetHandle: 'in',
					style: 'stroke: #94a3b8',
					markerEnd: { type: MarkerType.ArrowClosed, color: '#94a3b8' },
					data: { kind: 'chain' }
				});
			} else {
				e.push({
					id: `flow:rule${i - 1}-${i}`,
					source: `rule:${i - 1}`,
					target: rid,
					sourceHandle: 'out',
					targetHandle: 'in',
					style: 'stroke: #94a3b8',
					markerEnd: { type: MarkerType.ArrowClosed, color: '#94a3b8' },
					data: { kind: 'chain' }
				});
			}

			// rule → group/builtin by outbound name
			const target = resolveOutboundNodeId(r.outbound, groups);
			if (target) {
				e.push({
					id: `out:${i}:${r.outbound}`,
					source: rid,
					target,
					sourceHandle: 'out',
					targetHandle: 'in',
					style: 'stroke: #fbbf24; stroke-dasharray: 4 3',
					markerEnd: { type: MarkerType.ArrowClosed, color: '#fbbf24' },
					data: { kind: 'route', ruleIndex: i }
				});
			}
		});

		// Fallback node
		n.push({
			id: 'fallback',
			type: 'fallback',
			position: { x: 620, y: 60 + rules.length * 160 + 20 },
			data: { outbound: fb }
		});
		if (rules.length) {
			e.push({
				id: 'flow:last-fallback',
				source: `rule:${rules.length - 1}`,
				target: 'fallback',
				sourceHandle: 'out',
				targetHandle: 'in',
				style: 'stroke: #f87171',
				markerEnd: { type: MarkerType.ArrowClosed, color: '#f87171' },
				data: { kind: 'chain' }
			});
		} else {
			e.push({
				id: 'flow:start-fallback',
				source: 'start',
				target: 'fallback',
				sourceHandle: 'out',
				targetHandle: 'in',
				style: 'stroke: #f87171',
				markerEnd: { type: MarkerType.ArrowClosed, color: '#f87171' },
				data: { kind: 'chain' }
			});
		}
		const fbTarget = resolveOutboundNodeId(fb, groups);
		if (fbTarget) {
			e.push({
				id: `fb-out:${fb}`,
				source: 'fallback',
				target: fbTarget,
				sourceHandle: 'out',
				targetHandle: 'in',
				style: 'stroke: #f87171; stroke-dasharray: 2 2',
				markerEnd: { type: MarkerType.ArrowClosed, color: '#f87171' },
				data: { kind: 'fallback-out' }
			});
		}

		nodes = n;
		edges = e;
	}

	function resolveOutboundNodeId(outbound: string, groups: GroupDto[]): string | null {
		const o = outbound.trim();
		const g = groups.find((x) => x.name === o);
		if (g) return `group:${g.id}`;
		if (['direct', 'must_direct', 'block'].includes(o)) return `builtin:${o}`;
		// proxy default group by name
		const proxy = groups.find((x) => x.name === 'proxy');
		if (o === 'proxy' && proxy) return `group:${proxy.id}`;
		return null;
	}

	async function reload() {
		error = '';
		try {
			const [n, g, r] = await Promise.all([listNodes(), listGroups(), getRouting()]);
			apiNodes = n.nodes;
			apiGroups = g.groups.map((x) => ({ ...x, members: x.members ?? [] }));
			apiRules = r.rules;
			fallback = r.fallback;
			rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
			dirty = false;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('groups.loadFailed');
		}
	}

	onMount(() => {
		void reload();
	});

	async function handleWeight(groupId: string, nodeId: string, weight: number) {
		const w = Math.max(1, Math.min(99, Math.floor(weight) || 1));
		try {
			await setGroupMemberWeight(groupId, nodeId, w);
			apiGroups = apiGroups.map((g) =>
				g.id !== groupId
					? g
					: {
							...g,
							members: (g.members ?? []).map((m) =>
								m.node_id === nodeId ? { ...m, weight: w } : m
							)
						}
			);
			rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.saveFailed');
		}
	}

	async function handleRemoveMember(groupId: string, nodeId: string) {
		busy = true;
		try {
			await removeGroupMember(groupId, nodeId);
			await reload();
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.deleteFailed');
		} finally {
			busy = false;
		}
	}

	const isValidConnection: IsValidConnection = (c) => {
		if (!c.source || !c.target || c.source === c.target) return false;
		// proxy → group only for membership
		if (c.source.startsWith('proxy:') && c.target.startsWith('group:')) return true;
		// rule → group/builtin for outbound
		if (c.source.startsWith('rule:') && (c.target.startsWith('group:') || c.target.startsWith('builtin:')))
			return true;
		// start → rule
		if (c.source === 'start' && c.target.startsWith('rule:')) return true;
		// rule → rule (reorder chain manually still allowed as visual only; save uses array order)
		if (c.source.startsWith('rule:') && c.target.startsWith('rule:')) return true;
		return false;
	};

	async function onconnect(c: Connection) {
		if (!c.source || !c.target) return;

		// Membership: proxy → group
		if (c.source.startsWith('proxy:') && c.target.startsWith('group:')) {
			const nodeId = c.source.slice('proxy:'.length);
			const groupId = c.target.slice('group:'.length);
			busy = true;
			error = '';
			try {
				const g = await addGroupMember(groupId, nodeId, 1);
				apiGroups = apiGroups.map((x) =>
					x.id === g.id ? { ...g, members: g.members ?? [] } : x
				);
				rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
				message = t('flow.saved');
			} catch (err) {
				error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.saveFailed');
			} finally {
				busy = false;
			}
			return;
		}

		// Route: rule → group/builtin — set rule outbound
		if (c.source.startsWith('rule:') && (c.target.startsWith('group:') || c.target.startsWith('builtin:'))) {
			const idx = Number(c.source.slice('rule:'.length));
			if (Number.isNaN(idx) || !apiRules[idx]) return;
			let outbound = '';
			if (c.target.startsWith('group:')) {
				const gid = c.target.slice('group:'.length);
				outbound = apiGroups.find((g) => g.id === gid)?.name ?? '';
			} else {
				outbound = c.target.slice('builtin:'.length);
			}
			if (!outbound) return;
			const next = [...apiRules];
			next[idx] = { ...next[idx], outbound };
			apiRules = next;
			dirty = true;
			rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
		}
	}

	async function onCreateGroup() {
		const name = newGroupName.trim();
		if (!name) {
			error = t('groups.nameRequired');
			return;
		}
		busy = true;
		error = '';
		try {
			const g = await createGroup({ name, policy: newPolicy || 'fixed' });
			apiGroups = [...apiGroups, { ...g, members: g.members ?? [] }];
			newGroupName = '';
			rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.saveFailed');
		} finally {
			busy = false;
		}
	}

	function addRule() {
		const out = apiGroups[0]?.name || 'proxy';
		apiRules = [
			...apiRules,
			{ expression: 'domain(example.com)', outbound: out, enabled: true }
		];
		dirty = true;
		rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
	}

	async function saveRouting() {
		busy = true;
		error = '';
		message = '';
		try {
			const doc = await putRouting({
				rules: apiRules.map((r) => ({
					expression: r.expression.trim(),
					outbound: r.outbound.trim(),
					enabled: r.enabled
				})),
				fallback: fallback.trim() || 'proxy'
			});
			apiRules = doc.rules;
			fallback = doc.fallback;
			rebuildGraph(apiNodes, apiGroups, apiRules, fallback);
			dirty = false;
			message = t('flow.saved');
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('routing.saveFailed');
		} finally {
			busy = false;
		}
	}
</script>

<span class="eyebrow">orchestrate · svelte flow</span>
<div class="title-row">
	<div>
		<h1 class="page-title">{t('flow.title')}</h1>
		<p class="page-sub">{t('flow.subtitle')}</p>
	</div>
	<div class="toolbar">
		<input class="gname" placeholder={t('groups.name')} bind:value={newGroupName} disabled={busy} />
		<select bind:value={newPolicy} disabled={busy}>
			<option value="fixed">fixed</option>
			<option value="random">random</option>
			<option value="min_moving_avg">min_moving_avg</option>
			<option value="min">min</option>
		</select>
		<button type="button" disabled={busy} onclick={onCreateGroup}>{t('flow.addGroup')}</button>
		<button type="button" disabled={busy} onclick={addRule}>{t('flow.addRule')}</button>
		<button type="button" class="primary" disabled={busy} onclick={saveRouting}>
			{busy ? t('common.saving') : t('flow.saveRules')}
			{#if dirty}<span class="dot">●</span>{/if}
		</button>
	</div>
</div>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<p class="legend mono">
	<span class="l g">proxy → group</span> membership ·
	<span class="l y">rule → group</span> outbound · drag cards freely · connect handles
</p>

<div class="canvas panel">
	<SvelteFlow
		bind:nodes
		bind:edges
		{nodeTypes}
		fitView
		colorMode="dark"
		minZoom={0.25}
		maxZoom={1.75}
		defaultEdgeOptions={{ type: 'smoothstep' }}
		isValidConnection={isValidConnection}
		onconnect={onconnect}
	>
		<Controls />
		<MiniMap pannable zoomable />
		<Background variant={BackgroundVariant.Dots} gap={18} size={1} />
	</SvelteFlow>
</div>

<style>
	.title-row {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 1rem;
		align-items: flex-start;
		margin-bottom: 0.5rem;
	}
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: center;
	}
	.gname {
		width: 8rem;
	}
	.dot {
		margin-left: 0.25rem;
		color: #fbbf24;
	}
	.legend {
		font-size: 0.75rem;
		color: var(--ink-dim);
		margin: 0 0 0.75rem;
	}
	.l {
		font-weight: 600;
	}
	.l.g {
		color: var(--signal);
	}
	.l.y {
		color: #fbbf24;
	}
	.canvas {
		height: min(72vh, 720px);
		padding: 0;
		overflow: hidden;
	}
	.canvas :global(.svelte-flow) {
		background: transparent;
	}
	.canvas :global(.svelte-flow__minimap) {
		background: #0f172a !important;
	}
	.mono {
		font-family: var(--font-mono);
	}
</style>
