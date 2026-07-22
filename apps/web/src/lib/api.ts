/** Minimal browser API client for chaos-api (Bearer from localStorage.token). */

import { getLocale } from '$lib/i18n.svelte';
import type { Edge } from '@xyflow/svelte';

let sessionRedirectPending = false;
const SESSION_REDIRECT_KEY = 'chaos_force_session_redirect';
const API_TIMEOUT_MS = 30_000;

export function markSessionRedirect(): void {
	if (typeof sessionStorage === 'undefined') return;
	sessionStorage.setItem(SESSION_REDIRECT_KEY, '1');
	// SPA guards use this key; a full location change uses the unload guard.
	sessionStorage.setItem('chaos_allow_dirty_navigation', '1');
}

export function isSessionRedirectPending(): boolean {
	return typeof sessionStorage !== 'undefined' && sessionStorage.getItem(SESSION_REDIRECT_KEY) === '1';
}

export function clearSessionRedirect(): void {
	if (typeof sessionStorage === 'undefined') return;
	sessionStorage.removeItem(SESSION_REDIRECT_KEY);
	sessionStorage.removeItem('chaos_allow_dirty_navigation');
}

export type ApiErrorBody = {
	error: {
		code: string;
		message: string;
		draft_saved?: boolean;
	};
};

export class ApiClientError extends Error {
	status: number;
	code: string;
	draftSaved: boolean;

	constructor(status: number, code: string, message: string, draftSaved = false) {
		super(message);
		this.name = 'ApiClientError';
		this.status = status;
		this.code = code;
		this.draftSaved = draftSaved;
	}
}

function getToken(): string | null {
	if (typeof localStorage === 'undefined') return null;
	return localStorage.getItem('token');
}

export function setToken(token: string | null): void {
	if (typeof localStorage === 'undefined') return;
	if (token) localStorage.setItem('token', token);
	else localStorage.removeItem('token');
}

/**
 * Fetch JSON from the API. Paths should be absolute under the app origin, e.g. `/api/v1/health`.
 * Attaches `Authorization: Bearer <localStorage.token>` when a token is present.
 * Sends `Accept-Language` from the active UI locale.
 */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
	const headers = new Headers(init.headers);

	const method = (init.method ?? 'GET').toUpperCase();
	const hasBody = init.body != null && init.body !== '';
	if (hasBody && !headers.has('Content-Type')) {
		headers.set('Content-Type', 'application/json');
	}

	const token = getToken();
	if (token && !headers.has('Authorization')) {
		headers.set('Authorization', `Bearer ${token}`);
	}

	if (!headers.has('Accept-Language')) {
		headers.set('Accept-Language', getLocale());
	}

	const controller = new AbortController();
	const timeout = setTimeout(() => controller.abort(), API_TIMEOUT_MS);
	const externalSignal = init.signal;
	if (externalSignal) {
		if (externalSignal.aborted) controller.abort();
		else externalSignal.addEventListener('abort', () => controller.abort(), { once: true });
	}
	let res: Response;
	try {
		res = await fetch(path, {
			...init,
			method,
			headers,
			signal: controller.signal
		});
	} catch (cause) {
		if (cause instanceof Error && cause.name === 'AbortError') {
			throw new ApiClientError(408, 'request_timeout', 'request timed out');
		}
		throw cause;
	} finally {
		clearTimeout(timeout);
	}

	const text = await res.text();
	let data: unknown = null;
	if (text) {
		try {
			data = JSON.parse(text);
		} catch {
			if (!res.ok) {
				throw new ApiClientError(res.status, 'invalid_response', text || res.statusText);
			}
			throw new ApiClientError(res.status, 'invalid_response', 'response is not JSON');
		}
	}

	if (!res.ok) {
		if (
			res.status === 401 &&
			token &&
			!path.includes('/auth/login') &&
			!path.includes('/auth/setup') &&
			typeof window !== 'undefined' &&
			!sessionRedirectPending
		) {
			sessionRedirectPending = true;
			setToken(null);
			markSessionRedirect();
			const next = `${window.location.pathname}${window.location.search}`;
			window.location.replace(`/login?expired=1&next=${encodeURIComponent(next)}`);
		}
		const body = data as ApiErrorBody | null;
		const code = body?.error?.code ?? 'request_failed';
		const message = body?.error?.message ?? (res.statusText || 'request failed');
		throw new ApiClientError(res.status, code, message, body?.error?.draft_saved === true);
	}

	return data as T;
}

export type AuthStatus = { initialized: boolean };
export type TokenResponse = { token: string };
export type HealthResponse = {
	ok: boolean;
	api_version: string;
	dae_binary: string | null;
	dae_binary_ok: boolean;
	data_plane: string;
	data_plane_ready: boolean;
};

export function authStatus() {
	return api<AuthStatus>('/api/v1/auth/status');
}

export function setupAdmin(username: string, password: string) {
	return api<TokenResponse>('/api/v1/auth/setup', {
		method: 'POST',
		body: JSON.stringify({ username, password })
	});
}

export function login(username: string, password: string) {
	return api<TokenResponse>('/api/v1/auth/login', {
		method: 'POST',
		body: JSON.stringify({ username, password })
	});
}

export function health() {
	return api<HealthResponse>('/api/v1/health');
}

export type NodeDto = {
	id: string;
	name: string;
	tag: string | null;
	link: string;
	protocol: string | null;
	address: string | null;
	subscription_id: string | null;
	created_at: string;
	country_code: string | null;
};

export type ImportItemResult =
	| { ok: true; node: NodeDto }
	| { ok: false; link: string; error: { code: string; message: string } };

export type SubscriptionDto = {
	id: string;
	tag: string | null;
	url: string;
	updated_at: string;
	status: string;
	node_count: number;
	needs_republish: boolean;
	refresh_interval_hours: number;
	last_refreshed_at: string | null;
	next_refresh_at: string | null;
};

export type LatencyDto = {
	id: string;
	latency_ms: number | null;
	alive: boolean;
	tested_at: string;
	message: string | null;
};

export type GeoIpDataStatus = {
	path: string;
	exists: boolean;
	bytes: number;
};

export type RuntimeStatus = {
	running: boolean;
	dae_binary: string | null;
	dae_binary_ok: boolean;
	work_dir: string;
	config_exists: boolean;
	needs_republish: boolean;
	data_plane: string;
	data_plane_ready: boolean;
	geoip_data: GeoIpDataStatus;
	geosite_data: GeoIpDataStatus;
};

export type ApplyResponse = {
	ok: boolean;
	running: boolean;
	config_path: string;
	nodes: number;
	needs_republish: boolean;
	data_plane: string;
	/** How the config was applied: "hot" (zero-downtime), "cold" (restart), "cold_start" (was not running). */
	reload_method: 'hot' | 'cold' | 'cold_start';
};

export type LogsResponse = {
	path: string;
	exists: boolean;
	lines: string[];
};

export type DiagnosticsResponse = {
	kernel_version: string;
	kernel_ok: boolean;
	ebpf_supported: boolean;
	cgroup2_mounted: boolean;
	bpf_fs_mounted: boolean;
	ip_forward: boolean;
	interfaces: string[];
	dae_binary_version: string | null;
	permissions: {
		root: boolean;
		cap_net_admin: boolean;
		cap_bpf: boolean;
	};
};

export type OrchestrationNodeKind = 'start' | 'end' | 'rule' | 'node_group' | 'builtin';

export type OrchestrationSource = {
	kind: 'node' | 'subscription' | 'group';
	id: string;
	weight: number;
};

export type OrchestrationRuleMatcherKind =
	| 'domain_suffix'
	| 'destination_cidr'
	| 'domain_keyword'
	| 'domain_full'
	| 'geosite'
	| 'geoip'
	| 'source_cidr'
	| 'source_port'
	| 'dest_port'
	| 'ip_version'
	| 'process_name'
	| 'mac_address'
	| 'protocol';

export type OrchestrationRuleMatcher = {
	kind: OrchestrationRuleMatcherKind;
	pattern: string;
};

export type OrchestrationRuleData = {
	matcher: OrchestrationRuleMatcher;
	priority?: number;
	/** Render-only information. Removed before persistence. */
	target_name?: string;
};

export type OrchestrationNodeGroupData = {
	name: string;
	policy: string;
	sources: OrchestrationSource[];
	runtime_group_id?: string | null;
	/** Render-only information. Removed before persistence. */
	route_count?: number;
};

export type OrchestrationBuiltinData = {
		builtin: 'direct';
		/** Render-only information. Removed before persistence. */
		route_count?: number;
	};

	export type OrchestrationStartData = {
		/** Render-only information. Removed before persistence. */
		route_count?: number;
	};

	export type OrchestrationEndData = {
		/** Render-only information. Removed before persistence. */
		route_count?: number;
		/** Render-only information. Removed before persistence. */
		target_name?: string;
	};

/** @deprecated Prefer Start/End specific data types. */
export type OrchestrationAnchorData = OrchestrationStartData | OrchestrationEndData;

	export type OrchestrationNodeData =
                | OrchestrationRuleData
                | OrchestrationNodeGroupData
                | OrchestrationBuiltinData
                | OrchestrationStartData
                | OrchestrationEndData;

	type OrchestrationNodePresentation = {
		selected?: boolean;
		draggable?: boolean;
		deletable?: boolean;
		ariaLabel?: string;
	};

	export type OrchestrationRuleNodeDto = OrchestrationNodePresentation & {
		id: string;
		type: 'rule';
		position: { x: number; y: number };
		data: OrchestrationRuleData;
	};

	export type OrchestrationNodeGroupDto = OrchestrationNodePresentation & {
		id: string;
		type: 'node_group';
		position: { x: number; y: number };
		data: OrchestrationNodeGroupData;
	};

	export type OrchestrationBuiltinNodeDto = OrchestrationNodePresentation & {
		id: string;
		type: 'builtin';
		position: { x: number; y: number };
		data: OrchestrationBuiltinData;
	};

	export type OrchestrationStartNodeDto = OrchestrationNodePresentation & {
		id: string;
		type: 'start';
		position: { x: number; y: number };
		data: OrchestrationStartData;
	};

export type OrchestrationEndNodeDto = OrchestrationNodePresentation & {
                        id: string;
                        type: 'end';
                        position: { x: number; y: number };
                        data: OrchestrationEndData;
                };

                export type OrchestrationNodeDto =
                | OrchestrationRuleNodeDto
                | OrchestrationNodeGroupDto
                | OrchestrationBuiltinNodeDto
                | OrchestrationStartNodeDto
                | OrchestrationEndNodeDto;

	export type OrchestrationEdgeDto = {
		id: string;
		source: string;
		target: string;
		type?: string;
		selected?: boolean;
		deletable?: boolean;
		markerEnd?: Edge['markerEnd'];
		style?: string;
		label?: string;
	};

	export type OrchestrationDocument = {
		version: 2 | 3 | 4;
		nodes: OrchestrationNodeDto[];
		edges: OrchestrationEdgeDto[];
		viewport: { x: number; y: number; zoom: number };
		needs_republish?: boolean;
	};

export type OrchestrationValidationIssue = {
	code: string;
	scope: 'graph' | 'runtime';
	node_id?: string;
	edge_id?: string;
};

export type OrchestrationValidation = {
	valid: boolean;
	dae_compatible: boolean;
	issues: OrchestrationValidationIssue[];
};

export type PublishOrchestrationResponse = {
	document: OrchestrationDocument;
	applied: ApplyResponse;
};

export function listNodes() {
	return api<{ nodes: NodeDto[] }>('/api/v1/nodes');
}

export function importNodes(links: { link: string; tag?: string }[]) {
	return api<{ results: ImportItemResult[] }>('/api/v1/nodes', {
		method: 'POST',
		body: JSON.stringify({ links })
	});
}

export function updateNode(
	id: string,
	body: { link: string; name?: string; tag?: string }
) {
	return api<NodeDto>(`/api/v1/nodes/${encodeURIComponent(id)}`, {
		method: 'PATCH',
		body: JSON.stringify(body)
	});
}

export function deleteNode(id: string) {
	return api<{ deleted: boolean }>(`/api/v1/nodes/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
}

export function listSubscriptions() {
	return api<{ subscriptions: SubscriptionDto[] }>('/api/v1/subscriptions');
}

export function importSubscription(url: string, tag?: string) {
	return api<{ subscription: SubscriptionDto; nodes: NodeDto[] }>('/api/v1/subscriptions', {
		method: 'POST',
		body: JSON.stringify({ url, tag })
	});
}

export function refreshSubscription(id: string) {
	return api<{ subscription: SubscriptionDto; nodes: NodeDto[] }>(
		`/api/v1/subscriptions/${encodeURIComponent(id)}/refresh`,
		{ method: 'POST' }
	);
}

export function deleteSubscription(id: string) {
	return api<{ deleted: boolean }>(
		`/api/v1/subscriptions/${encodeURIComponent(id)}`,
		{ method: 'DELETE' }
	);
}

export function setSubscriptionRefresh(id: string, intervalHours: number) {
	return api<{ subscription: SubscriptionDto }>(
		`/api/v1/subscriptions/${encodeURIComponent(id)}/refresh-schedule`,
		{ method: 'PUT', body: JSON.stringify({ refresh_interval_hours: intervalHours }) }
	);
}

export type UserDto = {
	id: string;
	username: string;
	role: string;
	created_at: string;
};

export function listUsers() {
	return api<{ users: UserDto[] }>('/api/v1/users');
}

export function createUser(username: string, password: string, role: string) {
	return api<{ user: UserDto }>('/api/v1/users', {
		method: 'POST',
		body: JSON.stringify({ username, password, role })
	});
}

export function deleteUser(id: string) {
	return api<{ deleted: boolean }>(`/api/v1/users/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
}

export function listLatency() {
	return api<{ results: LatencyDto[] }>('/api/v1/latency');
}

export function testLatency(ids?: string[] | null) {
	return api<{ results: LatencyDto[] }>('/api/v1/latency/test', {
		method: 'POST',
		body: JSON.stringify({ ids: ids === undefined ? null : ids })
	});
}

export function getRuntime() {
	return api<RuntimeStatus>('/api/v1/runtime');
}

export function updateGeoIpData() {
	return api<GeoIpDataStatus>('/api/v1/runtime/geoip/update', { method: 'POST' });
}

export function updateGeositeData() {
	return api<GeoIpDataStatus>('/api/v1/runtime/geosite/update', { method: 'POST' });
}

export function applyRuntime() {
	return api<ApplyResponse>('/api/v1/runtime/apply', { method: 'POST' });
}

export function getOrchestration() {
	return api<OrchestrationDocument>('/api/v1/orchestration');
}

export function putOrchestration(document: OrchestrationDocument) {
	return api<OrchestrationDocument>('/api/v1/orchestration', {
		method: 'PUT',
		body: JSON.stringify(document)
	});
}

export function validateOrchestration(document: OrchestrationDocument) {
	return api<OrchestrationValidation>('/api/v1/orchestration/validate', {
		method: 'POST',
		body: JSON.stringify(document)
	});
}

export function publishOrchestration(document: OrchestrationDocument) {
	return api<PublishOrchestrationResponse>('/api/v1/orchestration/publish', {
		method: 'POST',
		body: JSON.stringify(document)
	});
}

export type RouteProbe = {
	domain?: string | null;
	dest_ip?: string | null;
	geoip?: string[];
	geosite?: string[];
	protocol?: string | null;
	dest_port?: number | null;
};

export type RouteTraceStep = {
	kind: 'rule' | 'fallback' | string;
	matched: boolean;
	rule_index?: number;
	priority?: number;
	matcher_kind?: string;
	pattern?: string;
	condition?: string;
	outbound: string;
	reason: string;
};

export type RouteSimulation = {
	outbound: string;
	matched: boolean;
	matched_rule_index?: number;
	matched_condition?: string;
	matched_priority?: number;
	matched_matcher_kind?: string;
	matched_pattern?: string;
	steps: RouteTraceStep[];
};

/** Dry-run the compiled routing table against probe facts (current draft graph). */
export function simulateOrchestration(document: OrchestrationDocument, probe: RouteProbe) {
	return api<RouteSimulation>('/api/v1/orchestration/simulate', {
		method: 'POST',
		body: JSON.stringify({ document, probe })
	});
}

export function stopRuntime() {
	return api<RuntimeStatus>('/api/v1/runtime/stop', { method: 'POST' });
}

export function reloadRuntime() {
	return api<ApplyResponse>('/api/v1/runtime/reload', { method: 'POST' });
}

export function getLogs(lines = 100) {
	return api<LogsResponse>(`/api/v1/runtime/logs?lines=${lines}`);
}

export function getDiagnostics() {
	return api<DiagnosticsResponse>('/api/v1/runtime/diagnostics');
}

export type GroupMemberDto = {
	node_id: string;
	weight: number;
	sort_order: number;
	name: string | null;
	tag: string | null;
	protocol: string | null;
	address: string | null;
};

export type GroupDto = {
	id: string;
	name: string;
	policy: string;
	filter_tag: string | null;
	sort_order: number;
	created_at: string;
	members: GroupMemberDto[];
};

export type RoutingRuleDto = {
	expression: string;
	outbound: string;
	enabled: boolean;
};

export type RoutingDocument = {
	rules: RoutingRuleDto[];
	fallback: string;
};

export type DnsUpstreamDto = {
	name: string;
	address: string;
};

export type DnsRuleDto = {
	expression: string;
	upstream: string;
	enabled: boolean;
};

export type DnsDocument = {
	upstreams: DnsUpstreamDto[];
	rules: DnsRuleDto[];
	fallback: string;
};

export function listGroups() {
	return api<{ groups: GroupDto[] }>('/api/v1/groups');
}

export function createGroup(body: {
	name: string;
	policy?: string;
	filter_tag?: string;
	sort_order?: number;
}) {
	return api<GroupDto>('/api/v1/groups', {
		method: 'POST',
		body: JSON.stringify(body)
	});
}

export function updateGroup(
	id: string,
	body: { name: string; policy: string; filter_tag?: string | null; sort_order?: number }
) {
	return api<GroupDto>(`/api/v1/groups/${encodeURIComponent(id)}`, {
		method: 'PATCH',
		body: JSON.stringify(body)
	});
}

export function deleteGroup(id: string) {
	return api<{ deleted: boolean }>(`/api/v1/groups/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
}

export function replaceGroupMembers(
	id: string,
	members: { node_id: string; weight?: number }[]
) {
	return api<GroupDto>(`/api/v1/groups/${encodeURIComponent(id)}/members`, {
		method: 'PUT',
		body: JSON.stringify({ members })
	});
}

export function addGroupMember(id: string, node_id: string, weight = 1) {
	return api<GroupDto>(`/api/v1/groups/${encodeURIComponent(id)}/members`, {
		method: 'POST',
		body: JSON.stringify({ node_id, weight })
	});
}

export function removeGroupMember(id: string, node_id: string) {
	return api<{ deleted: boolean }>(
		`/api/v1/groups/${encodeURIComponent(id)}/members/${encodeURIComponent(node_id)}`,
		{ method: 'DELETE' }
	);
}

export function setGroupMemberWeight(id: string, node_id: string, weight: number) {
	return api<{ node_id: string; weight: number }>(
		`/api/v1/groups/${encodeURIComponent(id)}/members/${encodeURIComponent(node_id)}`,
		{ method: 'PATCH', body: JSON.stringify({ weight }) }
	);
}

export function getRouting() {
	return api<RoutingDocument>('/api/v1/routing');
}

export function getDns() {
	return api<DnsDocument>('/api/v1/dns');
}

export function putDns(doc: DnsDocument) {
	return api<DnsDocument>('/api/v1/dns', {
		method: 'PUT',
		body: JSON.stringify(doc)
	});
}

export type NetworkDocument = {
	wan_interfaces: string[];
	lan_interfaces: string[];
	auto_config_kernel_parameter: boolean;
};

export type NetworkInterfaceInfo = {
	name: string;
	ips: string[];
	up: boolean;
	is_default_route: boolean;
};

export function getNetwork() {
	return api<NetworkDocument>('/api/v1/network');
}

export function putNetwork(doc: NetworkDocument) {
	return api<NetworkDocument>('/api/v1/network', {
		method: 'PUT',
		body: JSON.stringify(doc)
	});
}

export function getNetworkInterfaces() {
	return api<{ interfaces: NetworkInterfaceInfo[] }>('/api/v1/network/interfaces');
}
