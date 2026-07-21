import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
	mergeLatencyMap,
	shouldAcceptGeneration,
	normalizeTestIds
} from './latencySessionCore.ts';

type LatencyDto = {
	id: string;
	latency_ms: number | null;
	alive: boolean;
	tested_at: string;
	message: string | null;
};

function sample(id: string, ms: number | null, alive = true): LatencyDto {
	return { id, latency_ms: ms, alive, tested_at: 't', message: null };
}

describe('mergeLatencyMap', () => {
	it('merges by id without dropping other keys', () => {
		const prev = { a: sample('a', 10), b: sample('b', 20) };
		const next = mergeLatencyMap(prev, [sample('b', 99), sample('c', 5)]);
		assert.equal(next.a.latency_ms, 10);
		assert.equal(next.b.latency_ms, 99);
		assert.equal(next.c.latency_ms, 5);
	});
});

describe('shouldAcceptGeneration', () => {
	it('accepts matching generation only', () => {
		assert.equal(shouldAcceptGeneration(3, 3), true);
		assert.equal(shouldAcceptGeneration(3, 2), false);
	});
});

describe('normalizeTestIds', () => {
	it('keeps nullish as null (all nodes)', () => {
		assert.equal(normalizeTestIds(null), null);
		assert.equal(normalizeTestIds(undefined), null);
	});
	it('returns empty array as empty (caller skips request)', () => {
		assert.deepEqual(normalizeTestIds([]), []);
	});
	it('returns non-empty list as-is', () => {
		assert.deepEqual(normalizeTestIds(['x', 'y']), ['x', 'y']);
	});
});
