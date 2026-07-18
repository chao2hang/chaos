import adapter from '@sveltejs/adapter-auto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter(),
		alias: {
			$locales: path.join(root, 'locales')
		}
	}
};

export default config;
