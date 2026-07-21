import {
	listLatency,
	testLatency,
	ApiClientError,
	type LatencyDto
} from '$lib/api';
import { apiErrorText, t } from '$lib/i18n.svelte';
import {
	mergeLatencyMap,
	normalizeTestIds,
	shouldAcceptGeneration
} from '$lib/latencySessionCore';

export function createLatencySession() {
	let latencyById = $state<Record<string, LatencyDto>>({});
	let testing = $state<string | null>(null);
	let error = $state('');
	let message = $state('');
	let generation = 0;

	function dispose() {
		generation += 1;
		testing = null;
	}

	function clearNotices() {
		error = '';
		message = '';
	}

	async function load() {
		const gen = generation;
		error = '';
		try {
			const response = await listLatency();
			if (!shouldAcceptGeneration(generation, gen)) return;
			const map: Record<string, LatencyDto> = {};
			for (const result of response.results) map[result.id] = result;
			latencyById = map;
		} catch (cause) {
			if (!shouldAcceptGeneration(generation, gen)) return;
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed');
		}
	}

	async function test(ids: string[] | null, marker: string) {
		const normalized = normalizeTestIds(ids);
		if (Array.isArray(normalized) && normalized.length === 0) return;

		const gen = generation;
		testing = marker;
		error = '';
		message = '';
		try {
			const response = await testLatency(normalized);
			if (!shouldAcceptGeneration(generation, gen)) return;
			latencyById = mergeLatencyMap(latencyById, response.results);
			const alive = response.results.filter((result) => result.alive).length;
			message = t('dashboard.latencyFinished', {
				alive,
				total: response.results.length
			});
		} catch (cause) {
			if (!shouldAcceptGeneration(generation, gen)) return;
			error =
				cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed');
		} finally {
			if (shouldAcceptGeneration(generation, gen)) testing = null;
		}
	}

	return {
		get latencyById() {
			return latencyById;
		},
		get testing() {
			return testing;
		},
		get error() {
			return error;
		},
		get message() {
			return message;
		},
		load,
		test,
		dispose,
		clearNotices
	};
}

export type LatencySession = ReturnType<typeof createLatencySession>;
