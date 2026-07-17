/** Minimal browser API client for chaos-api (Bearer from localStorage.token). */

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
