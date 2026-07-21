// Theme management: light / dark / system
import { browser } from '$app/environment';

export type Theme = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'chaos_theme';

function getStoredTheme(): Theme {
	if (!browser) return 'system';
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored === 'light' || stored === 'dark' || stored === 'system') {
		return stored;
	}
	return 'system';
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

// Reactive theme state
let currentTheme: Theme = getStoredTheme();

export function getTheme(): Theme {
	return currentTheme;
}

export function setTheme(theme: Theme) {
	currentTheme = theme;
	if (browser) {
		localStorage.setItem(STORAGE_KEY, theme);
		applyTheme(theme);
	}
}

export function initTheme() {
	if (browser) {
		currentTheme = getStoredTheme();
		applyTheme(currentTheme);

		// Listen for system theme changes when in 'system' mode
		const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
		mediaQuery.addEventListener('change', () => {
			if (currentTheme === 'system') {
				// CSS handles this automatically via media query
				// but we can trigger a re-render if needed
			}
		});
	}
}

export function cycleTheme(): Theme {
	const order: Theme[] = ['light', 'dark', 'system'];
	const currentIndex = order.indexOf(currentTheme);
	const nextTheme = order[(currentIndex + 1) % order.length];
	setTheme(nextTheme);
	return nextTheme;
}
