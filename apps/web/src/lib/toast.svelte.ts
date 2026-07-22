import { resolveDuration, type ToastTone } from './toastDuration';

export type { ToastTone };
export { resolveDuration };

export type ToastOptions = {
	id?: string;
	title: string;
	description?: string;
	tone?: ToastTone;
	duration?: number;
	action?: { label: string; onclick: () => void };
};

export type ToastItem = ToastOptions & {
	id: string;
	tone: ToastTone;
	duration: number;
};

const MAX_VISIBLE_TOASTS = 4;

let nextId = 0;
let items = $state<ToastItem[]>([]);

function create(options: ToastOptions): string {
	const id = options.id ?? `toast-${++nextId}`;
	const tone = options.tone ?? 'info';
	const item: ToastItem = {
		...options,
		id,
		tone,
		duration: resolveDuration(tone, options.duration)
	};
	const existing = items.findIndex((toast) => toast.id === id);
	items =
		existing === -1
			? [...items.slice(-(MAX_VISIBLE_TOASTS - 1)), item]
			: items.map((toast) => (toast.id === id ? item : toast));
	return id;
}

function dismiss(id: string) {
	if (!items.some((toast) => toast.id === id)) return;
	items = items.filter((toast) => toast.id !== id);
}

function clear() {
	if (!items.length) return;
	items = [];
}

export const toast = {
	get items() {
		return items;
	},
	show: create,
	dismiss,
	clear,
	info: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'info' }),
	success: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'success' }),
	warning: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'warning' }),
	error: (options: Omit<ToastOptions, 'tone'>) => create({ ...options, tone: 'error' })
};
