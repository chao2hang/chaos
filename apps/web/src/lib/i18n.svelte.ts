/** Shared locale catalogs from monorepo `locales/`. */

import en from '$locales/en.json';
import zhCN from '$locales/zh-CN.json';

export type LocaleId = 'en' | 'zh-CN';

export const SUPPORTED_LOCALES: { id: LocaleId; label: string }[] = [
	{ id: 'en', label: 'English' },
	{ id: 'zh-CN', label: '简体中文' }
];

const STORAGE_KEY = 'chaos_locale';

type Catalog = Record<string, string>;

const catalogs: Record<LocaleId, Catalog> = {
	en: en as Catalog,
	'zh-CN': zhCN as Catalog
};

/** Shared reactive locale (Svelte 5 module state). */
export const i18n = $state({
	locale: 'en' as LocaleId
});

function isLocaleId(v: string | null | undefined): v is LocaleId {
	return v === 'en' || v === 'zh-CN';
}

/** Normalize browser / Accept-Language tags. */
export function parseLocaleTag(tag: string | null | undefined): LocaleId | null {
	if (!tag) return null;
	const primary = tag
		.split(',')[0]
		?.split(';')[0]
		?.trim()
		.toLowerCase()
		.replace(/_/g, '-');
	if (!primary) return null;
	if (
		primary === 'zh' ||
		primary.startsWith('zh-hans') ||
		primary.startsWith('zh-cn') ||
		primary.startsWith('zh-sg')
	) {
		return 'zh-CN';
	}
	if (primary === 'en' || primary.startsWith('en-')) return 'en';
	return null;
}

function detectBrowserLocale(): LocaleId {
	if (typeof navigator === 'undefined') return 'en';
	const list = navigator.languages?.length ? navigator.languages : [navigator.language];
	for (const l of list) {
		const parsed = parseLocaleTag(l);
		if (parsed) return parsed;
	}
	return 'en';
}

export function initLocale(): LocaleId {
	if (typeof localStorage !== 'undefined') {
		const stored = localStorage.getItem(STORAGE_KEY);
		if (isLocaleId(stored)) {
			i18n.locale = stored;
			applyDocumentLang(stored);
			return stored;
		}
	}
	const detected = detectBrowserLocale();
	i18n.locale = detected;
	applyDocumentLang(detected);
	return detected;
}

export function setLocale(next: LocaleId): void {
	i18n.locale = next;
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, next);
	}
	applyDocumentLang(next);
}

function applyDocumentLang(id: LocaleId) {
	if (typeof document !== 'undefined') {
		document.documentElement.lang = id;
	}
}

export function getLocale(): LocaleId {
	return i18n.locale;
}

function lookup(id: LocaleId, key: string): string | undefined {
	return catalogs[id]?.[key] ?? (id !== 'en' ? catalogs.en[key] : undefined);
}

/** Translate `key` with optional `{name}` params. Reads reactive `i18n.locale`. */
export function t(key: string, params?: Record<string, string | number>): string {
	const id = i18n.locale;
	let s = lookup(id, key) ?? key;
	if (params) {
		for (const [name, value] of Object.entries(params)) {
			s = s.replaceAll(`{${name}}`, String(value));
		}
	}
	return s;
}

/** Prefer localized catalog for API error codes; else server message. */
export function apiErrorText(err: { code?: string; message?: string } | null | undefined): string {
	if (!err) return t('error.unknown');
	if (err.code) {
		const key = `error.${err.code}`;
		const localized = lookup(i18n.locale, key);
		if (localized) return localized;
	}
	return err.message || t('error.unknown');
}
