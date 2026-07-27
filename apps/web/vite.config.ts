import path from 'node:path';
import { fileURLToPath } from 'node:url';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const localesDir = path.join(root, 'locales');
const apiOrigin = process.env.CHAOS_API_ORIGIN ?? 'http://127.0.0.1:2030';
const webHost = process.env.CHAOS_WEB_HOST ?? '0.0.0.0';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			adapter: adapter({ fallback: 'index.html' }),
			alias: {
				$locales: localesDir
			}
		})
	],
	resolve: {
		alias: {
			$locales: localesDir
		}
	},
	server: {
		host: webHost,
		fs: {
			allow: [root]
		},
		proxy: {
			'/api': {
				target: apiOrigin,
				changeOrigin: true
			}
		}
	}
});
