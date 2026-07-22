// Theme management: light / dark / system
import { browser } from '$app/environment';

export type Theme = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'chaos_theme';

function getStoredTheme(): Theme {
	if (!browser) return 'system';
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored === 'light' || stored === 'dark' || stored === 'system') {
		return stored;
	}
	return 'system';
}

function getSystemDark(): boolean {
	if (!browser) return false;
	return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function resolveTheme(theme: Theme, systemDark: boolean): ResolvedTheme {
	if (theme === 'system') return systemDark ? 'dark' : 'light';
	return theme;
}

function applyTheme(theme: Theme) {
	if (!browser) return;
	const root = document.documentElement;

	if (theme === 'system') {
		// Remove explicit theme attribute, let CSS media query handle it
		root.removeAttribute('data-theme');
	} else {
		root.setAttribute('data-theme', theme);
	}
}

// Reactive theme state (shared across modules via runes)
let preference = $state<Theme>(getStoredTheme());
let systemIsDark = $state(getSystemDark());

export function getTheme(): Theme {
	return preference;
}

export function getResolvedTheme(): ResolvedTheme {
	return resolveTheme(preference, systemIsDark);
}

export function setTheme(theme: Theme) {
	preference = theme;
	if (browser) {
		localStorage.setItem(STORAGE_KEY, theme);
		applyTheme(theme);
	}
}

export function initTheme() {
	if (!browser) return;

	preference = getStoredTheme();
	systemIsDark = getSystemDark();
	applyTheme(preference);

	const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
	mediaQuery.addEventListener('change', (event) => {
		systemIsDark = event.matches;
	});
}

export function cycleTheme(): Theme {
	const order: Theme[] = ['light', 'dark', 'system'];
	const currentIndex = order.indexOf(preference);
	const nextTheme = order[(currentIndex + 1) % order.length];
	setTheme(nextTheme);
	return nextTheme;
}

/** Read a CSS custom property from the document root (browser only). */
export function readCssVar(name: string, fallback = ''): string {
	if (!browser) return fallback;
	const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
	return value || fallback;
}
