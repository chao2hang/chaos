import adapter from '@sveltejs/adapter-static';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter({ fallback: 'index.html' }),
		alias: {
			$locales: path.join(root, 'locales')
		}
	}
};

export default config;
