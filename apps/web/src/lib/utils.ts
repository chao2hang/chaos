/**
 * Convert an ISO 3166-1 alpha-2 country code to a flag emoji.
 *
 * Each letter is offset into the Regional Indicator Symbol block:
 * 'A' → 🇦 (U+1F1E6), 'B' → 🇧 (U+1F1E7), etc.
 *
 * @example countryFlag('US') // '🇺🇸'
 * @example countryFlag('JP') // '🇯🇵'
 */
export function countryFlag(code: string | null | undefined): string {
	if (!code || code.length !== 2) return '';
	const upper = code.toUpperCase();
	const a = upper.codePointAt(0)! - 0x41 + 0x1f1e6;
	const b = upper.codePointAt(1)! - 0x41 + 0x1f1e6;
	return String.fromCodePoint(a) + String.fromCodePoint(b);
}
