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
