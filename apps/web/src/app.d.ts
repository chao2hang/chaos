// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

declare module '$locales/en.json' {
	const value: Record<string, string>;
	export default value;
}

declare module '$locales/zh-CN.json' {
	const value: Record<string, string>;
	export default value;
}

export {};
