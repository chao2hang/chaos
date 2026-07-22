export type ToastTone = 'info' | 'success' | 'warning' | 'error';

const DEFAULT_DURATION = 5000;
const SUCCESS_DURATION = 4000;
const PERSISTENT_DURATION = 0;

export function resolveDuration(tone: ToastTone, duration?: number): number {
	if (duration !== undefined) return duration;
	if (tone === 'success') return SUCCESS_DURATION;
	if (tone === 'warning' || tone === 'error') return PERSISTENT_DURATION;
	return DEFAULT_DURATION;
}
