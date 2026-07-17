/** Latency display helpers: good <200ms, warn 200–500ms, bad >500ms or failed. */

export type LatencyTone = 'good' | 'warn' | 'bad' | 'unknown';

export function latencyTone(
	ms: number | null | undefined,
	alive: boolean
): LatencyTone {
	if (!alive || ms == null) return 'bad';
	if (ms < 200) return 'good';
	if (ms <= 500) return 'warn';
	return 'bad';
}

/** CSS class name for a tone (prefix with `lat-`). */
export function latencyClass(tone: LatencyTone): string {
	return `lat-${tone}`;
}

export function formatLatencyMs(
	ms: number | null | undefined,
	alive: boolean
): string {
	if (!alive) return 'fail';
	if (ms == null) return '—';
	return `${ms} ms`;
}
