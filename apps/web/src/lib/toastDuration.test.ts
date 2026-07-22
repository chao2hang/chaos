import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { resolveDuration } from './toastDuration.ts';

describe('resolveDuration', () => {
	it('uses success default 4000 when omitted', () => {
		assert.equal(resolveDuration('success'), 4000);
	});
	it('uses info default 5000 when omitted', () => {
		assert.equal(resolveDuration('info'), 5000);
	});
	it('uses warning and error default 0 when omitted', () => {
		assert.equal(resolveDuration('warning'), 0);
		assert.equal(resolveDuration('error'), 0);
	});
	it('honors explicit duration including zero', () => {
		assert.equal(resolveDuration('success', 0), 0);
		assert.equal(resolveDuration('error', 8000), 8000);
	});
});
