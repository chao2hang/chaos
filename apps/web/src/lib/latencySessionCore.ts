export type LatencyLike = {
	id: string;
	latency_ms: number | null;
	alive: boolean;
	tested_at: string;
	message: string | null;
};

export function mergeLatencyMap<T extends LatencyLike>(
	prev: Record<string, T>,
	results: T[]
): Record<string, T> {
	const next = { ...prev };
	for (const result of results) next[result.id] = result;
	return next;
}

export function shouldAcceptGeneration(active: number, response: number): boolean {
	return active === response;
}

/** null = test all; [] = test none (skip HTTP); non-empty = those ids */
export function normalizeTestIds(ids: string[] | null | undefined): string[] | null {
	if (ids === null || ids === undefined) return null;
	return ids;
}

/**
 * Place successfully measured nodes first, ordered by their fastest latency.
 * Untested and unavailable nodes remain after them in their existing order.
 */
export function sortByLatency<T>(
	items: readonly T[],
	getLatency: (item: T) => Pick<LatencyLike, 'alive' | 'latency_ms'> | undefined
): T[] {
	return items
		.map((item, index) => ({ item, index, latency: getLatency(item) }))
		.sort((left, right) => {
			const leftMeasured = left.latency?.alive && Number.isFinite(left.latency.latency_ms);
			const rightMeasured = right.latency?.alive && Number.isFinite(right.latency.latency_ms);

			if (leftMeasured && rightMeasured) {
				return left.latency!.latency_ms! - right.latency!.latency_ms! || left.index - right.index;
			}
			if (leftMeasured) return -1;
			if (rightMeasured) return 1;
			return left.index - right.index;
		})
		.map(({ item }) => item);
}
