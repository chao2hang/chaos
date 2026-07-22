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
import { toast } from '$lib/toast.svelte';

export function createLatencySession() {
	let latencyById = $state<Record<string, LatencyDto>>({});
	let testing = $state<string | null>(null);
	let generation = 0;

	function dispose() {
		generation += 1;
		testing = null;
	}

	async function load() {
		const gen = generation;
		try {
			const response = await listLatency();
			if (!shouldAcceptGeneration(generation, gen)) return;
			const map: Record<string, LatencyDto> = {};
			for (const result of response.results) map[result.id] = result;
			latencyById = map;
		} catch (cause) {
			if (!shouldAcceptGeneration(generation, gen)) return;
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed')
			});
		}
	}

	async function test(ids: string[] | null, marker: string) {
		const normalized = normalizeTestIds(ids);
		if (Array.isArray(normalized) && normalized.length === 0) return;

		const gen = generation;
		testing = marker;
		try {
			const response = await testLatency(normalized);
			if (!shouldAcceptGeneration(generation, gen)) return;
			latencyById = mergeLatencyMap(latencyById, response.results);
			const alive = response.results.filter((result) => result.alive).length;
			toast.success({
				title: t('dashboard.latencyFinished', {
					alive,
					total: response.results.length
				})
			});
		} catch (cause) {
			if (!shouldAcceptGeneration(generation, gen)) return;
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed')
			});
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
		load,
		test,
		dispose
	};
}

export type LatencySession = ReturnType<typeof createLatencySession>;
