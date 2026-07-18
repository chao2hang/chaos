/** Minimal browser API client for chaos-api (Bearer from localStorage.token). */

import { getLocale } from '$lib/i18n';

export type ApiErrorBody = {
	error: {
		code: string;
		message: string;
	};
};

export class ApiClientError extends Error {
	status: number;
	code: string;

	constructor(status: number, code: string, message: string) {
		super(message);
		this.name = 'ApiClientError';
		this.status = status;
		this.code = code;
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

	const res = await fetch(path, {
		...init,
		method,
		headers
	});

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
		const body = data as ApiErrorBody | null;
		const code = body?.error?.code ?? 'request_failed';
		const message = body?.error?.message ?? (res.statusText || 'request failed');
		throw new ApiClientError(res.status, code, message);
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
};

export type LatencyDto = {
	id: string;
	latency_ms: number | null;
	alive: boolean;
	tested_at: string;
	message: string | null;
};

export type RuntimeStatus = {
	running: boolean;
	dae_binary: string | null;
	dae_binary_ok: boolean;
	work_dir: string;
	config_exists: boolean;
};

export type ApplyResponse = {
	ok: boolean;
	running: boolean;
	config_path: string;
	nodes: number;
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

export function applyRuntime() {
	return api<ApplyResponse>('/api/v1/runtime/apply', { method: 'POST' });
}

export function stopRuntime() {
	return api<RuntimeStatus>('/api/v1/runtime/stop', { method: 'POST' });
}

export type GroupDto = {
	id: string;
	name: string;
	policy: string;
	filter_tag: string | null;
	sort_order: number;
	created_at: string;
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

export function getRouting() {
	return api<RoutingDocument>('/api/v1/routing');
}

export function putRouting(doc: RoutingDocument) {
	return api<RoutingDocument>('/api/v1/routing', {
		method: 'PUT',
		body: JSON.stringify(doc)
	});
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
