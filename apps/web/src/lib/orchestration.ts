import { MarkerType, type Connection } from '@xyflow/svelte';
import type {
	GroupDto,
	NodeDto,
	OrchestrationBuiltinNodeDto,
	OrchestrationDocument,
	OrchestrationEdgeDto,
	OrchestrationEndNodeDto,
	OrchestrationNodeDto,
	OrchestrationNodeGroupData,
	OrchestrationRuleData,
	OrchestrationRuleMatcher,
	OrchestrationRuleNodeDto,
	OrchestrationSource,
	OrchestrationStartNodeDto,
	OrchestrationValidation,
	OrchestrationValidationIssue,
	SubscriptionDto
} from '$lib/api';

const EDGE_MARKER = { type: MarkerType.ArrowClosed, color: '#111111', width: 18, height: 18 };
const DEFAULT_VIEWPORT = { x: 0, y: 0, zoom: 0.85 };
const DEFAULT_GROUP_POLICY = 'min_moving_avg';
const GROUP_POLICIES = new Set(['min_moving_avg', 'min', 'random', 'fixed']);
const MAX_RULE_PRIORITY = 9_999;
const RESERVED_NAMES = new Set(['direct', 'must_direct', 'block']);

export type OrchestrationResources = {
	nodes: NodeDto[];
	subscriptions: SubscriptionDto[];
	groups: GroupDto[];
};

export type RuleDataPatch = Partial<OrchestrationRuleData>;
export type GroupDataPatch = Partial<OrchestrationNodeGroupData>;
export type NodeDataPatch = RuleDataPatch | GroupDataPatch;

export const ORCHESTRATION_VERSION = 4 as const;

export function uid(prefix: string): string {
	const suffix = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
	return `${prefix}-${suffix}`;
}

export function createRuleNode(
	position: { x: number; y: number },
	priority: number,
	kind: OrchestrationRuleMatcher['kind'] = 'domain_suffix'
): OrchestrationRuleNodeDto {
	return {
		id: uid('rule'),
		type: 'rule',
		position,
		data: { matcher: { kind, pattern: '' }, priority },
		deletable: true,
		draggable: true,
		ariaLabel: 'Routing rule'
	};
}

export function createGroupNode(
	position: { x: number; y: number },
	index: number
): Extract<OrchestrationNodeDto, { type: 'node_group' }> {
	return {
		id: uid('group'),
		type: 'node_group',
		position,
		data: {
			name: `proxy_${String(index).padStart(2, '0')}`,
			policy: DEFAULT_GROUP_POLICY,
			sources: []
		},
		deletable: true,
		draggable: true,
		ariaLabel: 'Node group outbound'
	};
}

export function createDirectBuiltin(position: { x: number; y: number }): OrchestrationBuiltinNodeDto {
	return {
		id: 'direct',
		type: 'builtin',
		position,
		data: { builtin: 'direct' },
		deletable: false,
		draggable: true,
		ariaLabel: 'Direct outbound'
	};
}

export function createStartNode(position: { x: number; y: number } = { x: 40, y: 200 }): OrchestrationStartNodeDto {
	return {
		id: 'start',
		type: 'start',
		position,
		data: {},
		deletable: false,
		draggable: true,
		ariaLabel: 'Traffic entry'
	};
}

export function createEndNode(position: { x: number; y: number } = { x: 760, y: 280 }): OrchestrationEndNodeDto {
	return {
		id: 'end',
		type: 'end',
		position,
		data: {},
		deletable: false,
		draggable: true,
		ariaLabel: 'Default exit'
	};
}

export function migrateDocument(document: OrchestrationDocument): OrchestrationDocument {
	let nodes = [...(document.nodes ?? [])] as OrchestrationNodeDto[];
	let edges = [...(document.edges ?? [])];

	// V3 → V4: strip chain nodes and rewrite rule targets.
	if ((document as { version?: number }).version === 3) {
		const chainIds = new Set(
			nodes.filter((node) => (node as { type: string }).type === 'chain').map((node) => node.id)
		);
		if (chainIds.size > 0) {
			edges = edges.map((edge) => {
				if (!chainIds.has(edge.target)) return edge;
				const chainNode = nodes.find((node) => node.id === edge.target) as
					| { type: string; data?: { hops?: Array<{ kind: string; id: string }> } }
					| undefined;
				const hop = chainNode?.data?.hops?.[0];
				const target =
					hop?.kind === 'group' &&
					nodes.some((n) => n.id === hop.id && n.type === 'node_group')
						? hop.id
						: 'direct';
				return { ...edge, target };
			});
			edges = edges.filter((edge) => !chainIds.has(edge.source) && !chainIds.has(edge.target));
			nodes = nodes.filter((node) => (node as { type: string }).type !== 'chain');
		}
	}

	if (!nodes.some((node) => node.type === 'start')) nodes.push(createStartNode());
	if (!nodes.some((node) => node.type === 'end')) nodes.push(createEndNode());
	if (!nodes.some((node) => node.type === 'builtin' && node.data.builtin === 'direct')) {
		nodes.push(createDirectBuiltin({ x: 520, y: 280 }));
	}
	for (const rule of nodes.filter((node) => node.type === 'rule')) {
		if (!edges.some((edge) => edge.source === 'start' && edge.target === rule.id)) {
			edges.push({ id: `start-${rule.id}`, source: 'start', target: rule.id });
		}
	}
	if (!edges.some((edge) => edge.source === 'end')) {
		edges.push({ id: 'end-direct', source: 'end', target: 'direct' });
	}
	return sanitizeDocument(nodes, edges, document.viewport);
}

export function addRuleWithStart(
	document: OrchestrationDocument,
	position: { x: number; y: number }
): OrchestrationDocument {
	const migrated = migrateDocument(document);
	let nextPriority = 1;
	for (const node of migrated.nodes) {
		if (node.type === 'rule' && validPriority(node.data.priority)) {
			nextPriority = Math.max(nextPriority, (node.data.priority ?? 0) + 1);
		}
	}
	const rule = createRuleNode(position, nextPriority);
	const edges = [
		...migrated.edges,
		{ id: `start-${rule.id}`, source: 'start', target: rule.id }
	];
	return decorateDocument({
		...migrated,
		nodes: [...migrated.nodes, rule],
		edges
	});
}

export function sanitizeDocument(
	nodes: OrchestrationNodeDto[],
	edges: OrchestrationEdgeDto[],
	viewport: OrchestrationDocument['viewport']
): OrchestrationDocument {
	return {
		version: ORCHESTRATION_VERSION,
		nodes: nodes.map(sanitizeNode),
		edges: edges.map((edge) => ({ id: edge.id, source: edge.source, target: edge.target })),
		viewport: safeViewport(viewport)
	};
}

function sanitizeNode(node: OrchestrationNodeDto): OrchestrationNodeDto {
	const base = { id: node.id, type: node.type, position: safePosition(node.position) };
	if (node.type === 'rule') {
		return {
			...base,
			data: {
				matcher: {
					kind: node.data.matcher.kind,
					pattern:
						node.data.matcher.kind === 'domain_suffix'
							? normalizeDomainSuffix(node.data.matcher.pattern)
							: node.data.matcher.pattern.trim()
				},
				...(validPriority(node.data.priority) ? { priority: Math.floor(node.data.priority!) } : {})
			}
		} as OrchestrationRuleNodeDto;
	}
	if (node.type === 'builtin') {
		return { ...base, data: { builtin: 'direct' } } as OrchestrationBuiltinNodeDto;
	}
	if (node.type === 'start') {
		return { ...base, data: {} } as OrchestrationStartNodeDto;
	}
	if (node.type === 'end') {
		return { ...base, data: {} } as OrchestrationEndNodeDto;
	}
	return {
		...base,
		data: {
			name: node.data.name.trim(),
			policy: node.data.policy || DEFAULT_GROUP_POLICY,
			sources: (node.data.sources ?? []).map((source) => ({
				kind: source.kind,
				id: source.id,
				weight: clampWeight(source.weight)
			})),
			...(node.data.runtime_group_id ? { runtime_group_id: node.data.runtime_group_id } : {})
		}
	} as Extract<OrchestrationNodeDto, { type: 'node_group' }>;
}

function safePosition(position: { x: number; y: number }) {
	return {
		x: Number.isFinite(position.x) ? position.x : 0,
		y: Number.isFinite(position.y) ? position.y : 0
	};
}

function safeViewport(viewport: OrchestrationDocument['viewport']) {
	return {
		x: Number.isFinite(viewport.x) ? viewport.x : DEFAULT_VIEWPORT.x,
		y: Number.isFinite(viewport.y) ? viewport.y : DEFAULT_VIEWPORT.y,
		zoom: Number.isFinite(viewport.zoom) ? viewport.zoom : DEFAULT_VIEWPORT.zoom
	};
}

function validPriority(priority: number | undefined): boolean {
	return (
		priority !== undefined &&
		Number.isInteger(priority) &&
		priority >= 1 &&
		priority <= MAX_RULE_PRIORITY
	);
}

export function snapshotDocument(document: OrchestrationDocument): string {
	const graph = sanitizeDocument(document.nodes, document.edges, document.viewport);
	return JSON.stringify(graph);
}

export function decorateDocument(document: OrchestrationDocument): OrchestrationDocument {
		const migrated = migrateDocument(document);
		const incoming = countEdges(migrated.edges, 'target');
		const outgoing = countEdges(migrated.edges, 'source');
		const runtimeGroupRefs = new Map<string, string>();
		for (const node of migrated.nodes) {
			if (node.type === 'node_group' && node.data.runtime_group_id) {
				runtimeGroupRefs.set(node.data.runtime_group_id, node.id);
			}
		}
		const targets = new Map(
			migrated.nodes
				.filter((node) => node.type === 'node_group' || node.type === 'builtin')
				.map((node) => [
					node.id,
					node.type === 'builtin' ? 'DIRECT' : node.data.name
				])
		);
		let fallbackPriority = 1;
		for (const node of migrated.nodes) {
			if (node.type === 'rule') fallbackPriority = Math.max(fallbackPriority, (node.data.priority ?? 0) + 1);
		}

		return {
			...migrated,
			nodes: migrated.nodes.map((node): OrchestrationNodeDto => {
				if (node.type === 'rule') {
					const target = migrated.edges.find((edge) => edge.source === node.id)?.target;
					const priority = validPriority(node.data.priority) ? node.data.priority : fallbackPriority++;
					return {
						...node,
						data: { ...node.data, priority, target_name: target ? targets.get(target) : undefined },
						deletable: true,
						draggable: true,
						ariaLabel: 'Routing rule'
					};
				}
				if (node.type === 'node_group') {
					return {
						...node,
						data: {
							...node.data,
							sources: (node.data.sources ?? []).map((source) =>
								source.kind === 'group' && runtimeGroupRefs.has(source.id)
									? { ...source, id: runtimeGroupRefs.get(source.id)! }
									: source
							),
							route_count: incoming.get(node.id) ?? 0
						},
						deletable: true,
						draggable: true,
						ariaLabel: node.data.name || 'Node group outbound'
					};
				}
				if (node.type === 'start') {
					return {
						...node,
						data: { route_count: outgoing.get(node.id) ?? 0 },
						deletable: false,
						draggable: true,
						ariaLabel: 'Traffic entry'
					};
				}
				if (node.type === 'end') {
					return {
						...node,
						data: { route_count: outgoing.get(node.id) ?? 0 },
						deletable: false,
						draggable: true,
						ariaLabel: 'Default exit'
					};
				}
				return {
					...node,
					data: { ...node.data, route_count: incoming.get(node.id) ?? 0 },
					deletable: false,
					draggable: true,
					ariaLabel: 'Direct outbound'
				};
			}),
			edges: migrated.edges.map(decorateEdge)
		};
	}

function countEdges(edges: OrchestrationEdgeDto[], key: 'source' | 'target') {
	const counts = new Map<string, number>();
	for (const edge of edges) counts.set(edge[key], (counts.get(edge[key]) ?? 0) + 1);
	return counts;
}

export function decorateEdge(edge: OrchestrationEdgeDto): OrchestrationEdgeDto {
	return {
		...edge,
		type: 'smoothstep',
		deletable: true,
		markerEnd: EDGE_MARKER,
		style: `stroke: #111111; stroke-width: ${edge.selected ? 2.25 : 1.35};`
	};
}

function edgeAllowed(
		source: OrchestrationNodeDto,
		target: OrchestrationNodeDto
	): boolean {
		if (source.type === 'start') return target.type === 'rule';
		if (source.type === 'rule') {
			return target.type === 'node_group' || target.type === 'builtin';
		}
		if (source.type === 'end') {
			return target.type === 'node_group' || target.type === 'builtin';
		}
		return false;
	}

	export function canConnect(
		connection: Pick<Connection, 'source' | 'target'>,
		nodes: OrchestrationNodeDto[],
		edges: OrchestrationEdgeDto[]
	): boolean {
		const { source, target } = connection;
		if (!source || !target || source === target) return false;
		if (edges.some((edge) => edge.source === source && edge.target === target)) return false;
		const sourceNode = nodes.find((node) => node.id === source);
		const targetNode = nodes.find((node) => node.id === target);
		if (!sourceNode || !targetNode || !edgeAllowed(sourceNode, targetNode)) return false;
		if (sourceNode.type === 'start') {
			// each rule may have at most one inbound from start
			if (edges.some((edge) => edge.source === 'start' && edge.target === target)) return false;
		}
		if (sourceNode.type === 'rule' || sourceNode.type === 'end') {
			// replace semantics handled in createConnectionEdge; allow if no other out or same retarget path
			return true;
		}
		return true;
	}

	export function createConnectionEdge(
		connection: Pick<Connection, 'source' | 'target'>,
		nodes: OrchestrationNodeDto[],
		edges: OrchestrationEdgeDto[]
	): OrchestrationEdgeDto | null {
		if (!connection.source || !connection.target) return null;
		let working = edges;
		const sourceNode = nodes.find((node) => node.id === connection.source);
		if (sourceNode?.type === 'rule' || sourceNode?.type === 'end') {
			working = edges.filter((edge) => edge.source !== connection.source);
		}
		if (!canConnect(connection, nodes, working)) return null;
		return decorateEdge({
			id: uid('edge'),
			source: connection.source,
			target: connection.target
		});
	}

	export function setRuleTarget(
		ruleId: string,
		targetId: string | null,
		nodes: OrchestrationNodeDto[],
		edges: OrchestrationEdgeDto[]
	): OrchestrationEdgeDto[] {
		const withoutCurrent = edges.filter((edge) => edge.source !== ruleId);
		if (!targetId) return withoutCurrent;
		const edge = createConnectionEdge({ source: ruleId, target: targetId }, nodes, withoutCurrent);
		return edge ? [...withoutCurrent, edge] : withoutCurrent;
	}

	export function setEndTarget(
		targetId: string | null,
		nodes: OrchestrationNodeDto[],
		edges: OrchestrationEdgeDto[]
	): OrchestrationEdgeDto[] {
		const withoutCurrent = edges.filter((edge) => edge.source !== 'end');
		if (!targetId) return withoutCurrent;
		const edge = createConnectionEdge({ source: 'end', target: targetId }, nodes, withoutCurrent);
		return edge ? [...withoutCurrent, edge] : withoutCurrent;
	}

	export function autoLayout(document: OrchestrationDocument): OrchestrationDocument {
		const migrated = migrateDocument(document);
		const rules = migrated.nodes
			.filter((node): node is OrchestrationRuleNodeDto => node.type === 'rule')
			.sort(
				(left, right) =>
					(left.data.priority ?? Number.MAX_SAFE_INTEGER) -
					(right.data.priority ?? Number.MAX_SAFE_INTEGER)
			);
		const outbounds = [
			...migrated.nodes.filter((node) => node.type === 'node_group'),
			...migrated.nodes.filter((node) => node.type === 'builtin')
		];
		const positions = new Map<string, { x: number; y: number }>();
		positions.set('start', { x: 40, y: 200 });
		positions.set('end', { x: 800, y: 280 });
		rules.forEach((node, index) => positions.set(node.id, { x: 280, y: 70 + index * 145 }));
		outbounds.forEach((node, index) => positions.set(node.id, { x: 560, y: 70 + index * 180 }));
		return decorateDocument({
			...migrated,
			nodes: migrated.nodes.map((node) => ({
				...node,
				position: positions.get(node.id) ?? node.position
			}))
		});
	}

	export function validateLocal(
		document: OrchestrationDocument,
		resources: OrchestrationResources
	): OrchestrationValidation {
		const issues: OrchestrationValidationIssue[] = [];
		const graph = (code: string, node_id?: string, edge_id?: string) =>
			issues.push({ code, scope: 'graph', ...(node_id ? { node_id } : {}), ...(edge_id ? { edge_id } : {}) });
		const runtime = (code: string, node_id?: string) =>
			issues.push({ code, scope: 'runtime', ...(node_id ? { node_id } : {}) });
		const migrated = migrateDocument(document);
		if (migrated.version !== ORCHESTRATION_VERSION) graph('unsupported_version');

		const byId = new Map<string, OrchestrationNodeDto>();
		for (const node of migrated.nodes) {
			if (!node.id.trim()) graph('node_id_required', node.id);
			else if (byId.has(node.id)) graph('duplicate_node_id', node.id);
			byId.set(node.id, node);
			if (!Number.isFinite(node.position.x) || !Number.isFinite(node.position.y)) {
				graph('invalid_node_position', node.id);
			}
		}

		const starts = migrated.nodes.filter((node) => node.type === 'start');
		const ends = migrated.nodes.filter((node) => node.type === 'end');
		const direct = migrated.nodes.filter(
			(node) => node.type === 'builtin' && node.data.builtin === 'direct'
		);
		if (starts.length !== 1) graph('start_required');
		if (ends.length !== 1) graph('end_required');
		if (direct.length !== 1) graph('direct_required');

		const outgoing = new Map<string, OrchestrationEdgeDto[]>();
		const incoming = new Map<string, OrchestrationEdgeDto[]>();
		const edgeIds = new Set<string>();
		const pairs = new Set<string>();
		for (const edge of migrated.edges) {
			if (!edge.id.trim()) graph('edge_id_required', undefined, edge.id);
			else if (edgeIds.has(edge.id)) graph('duplicate_edge_id', undefined, edge.id);
			edgeIds.add(edge.id);
			const source = byId.get(edge.source);
			const target = byId.get(edge.target);
			if (!source || !target) {
				graph('dangling_edge', undefined, edge.id);
				continue;
			}
			if (edge.source === edge.target) graph('self_connection', edge.source, edge.id);
			const pair = `${edge.source}\0${edge.target}`;
			if (pairs.has(pair)) graph('duplicate_connection', undefined, edge.id);
			pairs.add(pair);
			if (!edgeAllowed(source, target)) {
				graph('invalid_connection', undefined, edge.id);
			}
			outgoing.set(edge.source, [...(outgoing.get(edge.source) ?? []), edge]);
			incoming.set(edge.target, [...(incoming.get(edge.target) ?? []), edge]);
		}

		const nameKeys = new Set<string>();
		const rulePriorities = new Set<number>();
		const ruleMatchers = new Set<string>();
		const flowGroupRefs = new Set<string>();
		const flowGroups = new Map<
			string,
			Extract<OrchestrationNodeDto, { type: 'node_group' }>
		>();
		for (const node of migrated.nodes) {
			if (node.type !== 'node_group') continue;
			flowGroupRefs.add(node.id);
			flowGroups.set(node.id, node);
			if (node.data.runtime_group_id) flowGroupRefs.add(node.data.runtime_group_id);
			if (node.data.runtime_group_id) flowGroups.set(node.data.runtime_group_id, node);
		}
		for (const node of migrated.nodes) {
			const outputs = outgoing.get(node.id) ?? [];
			const inputs = incoming.get(node.id) ?? [];
			if (node.type === 'rule') {
				if (!matcherValid(node.data.matcher)) graph('invalid_rule_pattern', node.id);
				if (outputs.length !== 1) graph('rule_target_required', node.id);
				const startIns = inputs.filter((edge) => edge.source === 'start');
				if (startIns.length !== 1) graph('rule_start_required', node.id);
				if (!validPriority(node.data.priority)) graph('invalid_rule_priority', node.id);
				if (node.data.priority !== undefined && rulePriorities.has(node.data.priority)) {
					graph('duplicate_rule_priority', node.id);
				}
				if (node.data.priority !== undefined) rulePriorities.add(node.data.priority);
				const matcherKey = `${node.data.matcher.kind}:${
					node.data.matcher.kind === 'domain_suffix'
						? normalizeDomainSuffix(node.data.matcher.pattern)
						: node.data.matcher.pattern.trim().toLowerCase()
				}`;
				if (ruleMatchers.has(matcherKey)) graph('duplicate_rule_matcher', node.id);
				ruleMatchers.add(matcherKey);
			}
			if (node.type === 'node_group') {
				if (outputs.length) graph('group_terminal_required', node.id);
				const key = daeIdentifier(node.data.name).toLowerCase();
				if (!key || nameKeys.has(key)) graph('invalid_group_name', node.id);
				if (RESERVED_NAMES.has(key)) graph('reserved_group_name', node.id);
				nameKeys.add(key);
				if (!GROUP_POLICIES.has(node.data.policy.trim())) graph('invalid_group_policy', node.id);
				if (!node.data.sources.length) runtime('group_source_required', node.id);
				validateSources(node.data.sources, node.id, resources, flowGroupRefs, flowGroups, graph, runtime);
			}
			if (node.type === 'builtin') {
				if (node.data.builtin !== 'direct') graph('unsupported_builtin', node.id);
				if (outputs.length) graph('builtin_terminal_required', node.id);
			}
			if ((node as { type: string }).type === 'chain') {
				graph('chain_unsupported', node.id);
			}
			if (node.type === 'end') {
				if (outputs.length !== 1) graph('end_target_required', node.id);
				else {
					const target = byId.get(outputs[0].target);
					if (!target || (target.type !== 'node_group' && target.type !== 'builtin')) {
						graph('end_target_invalid', node.id);
					}
				}
			}
		}
		validateGroupSourceCycles(migrated, graph);

		const deduplicated = deduplicateIssues(issues);
		const valid = !deduplicated.some((issue) => issue.scope === 'graph');
		return {
			valid,
			dae_compatible: valid && !deduplicated.some((issue) => issue.scope === 'runtime'),
			issues: deduplicated
		};
	}

function validateSources(
	sources: OrchestrationSource[],
	nodeId: string,
	resources: OrchestrationResources,
	flowGroupRefs: Set<string>,
	flowGroups: Map<string, Extract<OrchestrationNodeDto, { type: 'node_group' }>>,
	graph: (code: string, nodeId?: string) => void,
	runtime: (code: string, nodeId?: string) => void
) {
	const seen = new Set<string>();
	for (const source of sources) {
		const key = `${source.kind}:${source.id}`;
		if (!source.id.trim()) graph('source_id_required', nodeId);
		if (!Number.isInteger(source.weight) || source.weight < 1 || source.weight > 99) {
			graph('invalid_source_weight', nodeId);
		}
		if (seen.has(key)) graph('duplicate_group_source', nodeId);
		seen.add(key);
		if (!resourceExists(source, resources, flowGroupRefs)) runtime('source_missing', nodeId);
		else if (!resourceCanExpand(source, resources, flowGroups)) runtime('group_source_empty', nodeId);
	}
}

function resourceCanExpand(
	source: OrchestrationSource,
	resources: OrchestrationResources,
	flowGroups: Map<string, Extract<OrchestrationNodeDto, { type: 'node_group' }>>
): boolean {
	if (source.kind === 'node') return true;
	if (source.kind === 'subscription') {
		return (resources.subscriptions.find((item) => item.id === source.id)?.node_count ?? 0) > 0;
	}
	const flowGroup = flowGroups.get(source.id);
	if (flowGroup) return flowGroup.data.sources.length > 0;
	const stored = resources.groups.find((group) => group.id === source.id);
	if (!stored) return false;
	return stored.members.length > 0 || Boolean(
		stored.filter_tag && resources.nodes.some((node) => node.tag === stored.filter_tag)
	);
}

function resourceExists(
	source: OrchestrationSource,
	resources: OrchestrationResources,
	flowGroupRefs: Set<string>
): boolean {
	if (source.kind === 'node') return resources.nodes.some((node) => node.id === source.id);
	if (source.kind === 'subscription') return resources.subscriptions.some((item) => item.id === source.id);
	return flowGroupRefs.has(source.id) || resources.groups.some((group) => group.id === source.id);
}

function validateGroupSourceCycles(
	document: OrchestrationDocument,
	graph: (code: string, nodeId?: string) => void
) {
	const groups = document.nodes.filter(
		(node): node is Extract<OrchestrationNodeDto, { type: 'node_group' }> =>
			node.type === 'node_group'
	);
	const references = new Map<string, string>();
	for (const group of groups) {
		references.set(group.id, group.id);
		if (group.data.runtime_group_id) references.set(group.data.runtime_group_id, group.id);
	}
	const byId = new Map(groups.map((group) => [group.id, group]));
	const visiting = new Set<string>();
	const visited = new Set<string>();

	function visit(groupId: string): boolean {
		if (visiting.has(groupId)) return true;
		if (visited.has(groupId)) return false;
		visiting.add(groupId);
		const group = byId.get(groupId);
		const cyclic = Boolean(
			group?.data.sources.some(
				(source) =>
					source.kind === 'group' &&
					references.has(source.id) &&
					visit(references.get(source.id)!)
			)
		);
		visiting.delete(groupId);
		visited.add(groupId);
		return cyclic;
	}

	for (const group of groups) {
		if (visit(group.id)) graph('group_source_cycle', group.id);
	}
}

function matcherValid(matcher: OrchestrationRuleMatcher): boolean {
	return matcher.kind === 'domain_suffix'
		? validDomainSuffix(matcher.pattern)
		: validCidr(matcher.pattern);
}

export function normalizeDomainSuffix(value: string): string {
	return value.trim().toLowerCase().replace(/^\.+|\.+$/g, '');
}

function validDomainSuffix(value: string): boolean {
	const domain = normalizeDomainSuffix(value);
	if (!domain || domain.length > 253) return false;
	return domain.split('.').every(
		(label) =>
			label.length > 0 &&
			label.length <= 63 &&
			/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/i.test(label)
	);
}

function validCidr(value: string): boolean {
	const parts = value.trim().split('/');
	if (parts.length !== 2) return false;
	const [address, prefix] = parts;
	if (!address || !prefix || !/^\d{1,3}$/.test(prefix)) return false;
	const bits = Number(prefix);
	if (validIpv4(address)) return bits <= 32;
	return bits <= 128 && validIpv6(address);
}

function validIpv4(address: string): boolean {
	const parts = address.split('.');
	return (
		parts.length === 4 &&
		parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) >= 0 && Number(part) <= 255)
	);
}

function validIpv6(address: string): boolean {
	if (!address.includes(':') || !/^[0-9a-f:.]+$/i.test(address)) return false;
	const halves = address.split('::');
	if (halves.length > 2) return false;

	function segmentCount(half: string): number | null {
		if (!half) return 0;
		const segments = half.split(':');
		let count = 0;
		for (const [index, segment] of segments.entries()) {
			if (segment.includes('.')) {
				if (index !== segments.length - 1 || !validIpv4(segment)) return null;
				count += 2;
			} else {
				if (!/^[0-9a-f]{1,4}$/i.test(segment)) return null;
				count += 1;
			}
		}
		return count;
	}

	const left = segmentCount(halves[0]);
	const right = segmentCount(halves[1] ?? '');
	if (left === null || right === null) return false;
	return halves.length === 2 ? left + right < 8 : left === 8;
}

function daeIdentifier(value: string): string {
	let output = '';
	let lastUnderscore = false;
	for (const character of value.trim()) {
		if (/[a-z0-9]/i.test(character)) {
			output += character;
			lastUnderscore = false;
		} else if (!lastUnderscore) {
			output += '_';
			lastUnderscore = true;
		}
	}
	output = output.replace(/^_+|_+$/g, '');
	return /^[0-9]/.test(output) ? `id_${output}` : output;
}

function deduplicateIssues(issues: OrchestrationValidationIssue[]): OrchestrationValidationIssue[] {
	const seen = new Set<string>();
	return issues.filter((issue) => {
		const key = `${issue.scope}:${issue.code}:${issue.node_id ?? ''}:${issue.edge_id ?? ''}`;
		if (seen.has(key)) return false;
		seen.add(key);
		return true;
	});
}

function clampWeight(value: number): number {
	return Math.min(99, Math.max(1, Math.floor(Number(value) || 1)));
}

export function nodeDisplayName(node: OrchestrationNodeDto): string {
		if (node.type === 'rule') return node.data.matcher.pattern || 'Untitled rule';
		if (node.type === 'node_group') return node.data.name || 'Unnamed node group';
			if (node.type === 'start') return 'START';
		if (node.type === 'end') return 'END';
		return 'DIRECT';
	}

	export function nodeKindLabel(kind: OrchestrationNodeDto['type']): string {
		if (kind === 'node_group') return 'NODE GROUP';
		if (kind === 'builtin') return 'BUILT-IN';
			if (kind === 'start') return 'START';
		if (kind === 'end') return 'END';
		return 'RULE';
	}
