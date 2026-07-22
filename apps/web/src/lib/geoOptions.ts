/** Curated geoip / geosite codes shown as multi-select options. */

export type GeoOption = {
	code: string;
	/** i18n key under flow.geoip.* or flow.geosite.* */
	labelKey: string;
};

/** Common ISO-like geoip codes used in dae routing. */
export const GEOIP_OPTIONS: GeoOption[] = [
	{ code: 'private', labelKey: 'flow.geoip.private' },
	{ code: 'cn', labelKey: 'flow.geoip.cn' },
	{ code: 'hk', labelKey: 'flow.geoip.hk' },
	{ code: 'tw', labelKey: 'flow.geoip.tw' },
	{ code: 'mo', labelKey: 'flow.geoip.mo' },
	{ code: 'us', labelKey: 'flow.geoip.us' },
	{ code: 'jp', labelKey: 'flow.geoip.jp' },
	{ code: 'kr', labelKey: 'flow.geoip.kr' },
	{ code: 'sg', labelKey: 'flow.geoip.sg' },
	{ code: 'gb', labelKey: 'flow.geoip.gb' },
	{ code: 'de', labelKey: 'flow.geoip.de' },
	{ code: 'fr', labelKey: 'flow.geoip.fr' },
	{ code: 'ru', labelKey: 'flow.geoip.ru' },
	{ code: 'in', labelKey: 'flow.geoip.in' },
	{ code: 'au', labelKey: 'flow.geoip.au' },
	{ code: 'ca', labelKey: 'flow.geoip.ca' },
	{ code: 'br', labelKey: 'flow.geoip.br' },
	{ code: 'nl', labelKey: 'flow.geoip.nl' }
];

/** Common geosite categories for routing templates. */
export const GEOSITE_OPTIONS: GeoOption[] = [
	{ code: 'cn', labelKey: 'flow.geosite.cn' },
	{ code: 'geolocation-!cn', labelKey: 'flow.geosite.geolocationNotCn' },
	{ code: 'category-ads', labelKey: 'flow.geosite.categoryAds' },
	{ code: 'category-ads-all', labelKey: 'flow.geosite.categoryAdsAll' },
	{ code: 'google', labelKey: 'flow.geosite.google' },
	{ code: 'youtube', labelKey: 'flow.geosite.youtube' },
	{ code: 'netflix', labelKey: 'flow.geosite.netflix' },
	{ code: 'github', labelKey: 'flow.geosite.github' },
	{ code: 'telegram', labelKey: 'flow.geosite.telegram' },
	{ code: 'twitter', labelKey: 'flow.geosite.twitter' },
	{ code: 'facebook', labelKey: 'flow.geosite.facebook' },
	{ code: 'openai', labelKey: 'flow.geosite.openai' },
	{ code: 'steam', labelKey: 'flow.geosite.steam' },
	{ code: 'microsoft', labelKey: 'flow.geosite.microsoft' },
	{ code: 'apple', labelKey: 'flow.geosite.apple' },
	{ code: 'tiktok', labelKey: 'flow.geosite.tiktok' },
	{ code: 'discord', labelKey: 'flow.geosite.discord' },
	{ code: 'cloudflare', labelKey: 'flow.geosite.cloudflare' }
];

export function parseGeoCodes(pattern: string): string[] {
	const seen = new Set<string>();
	const out: string[] = [];
	for (const part of pattern.split(/[,\s]+/)) {
		const code = part.trim().toLowerCase();
		if (!code || seen.has(code)) continue;
		seen.add(code);
		out.push(code);
	}
	return out;
}

export function serializeGeoCodes(codes: string[]): string {
	return parseGeoCodes(codes.join(',')).join(', ');
}
