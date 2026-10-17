import test from 'node:test';
import assert from 'node:assert/strict';
import { requestJson } from './api.js';

test('JSON requests preserve options, merge headers, and decode a successful response', async () => {
    const originalFetch = globalThis.fetch;
    let request;
    globalThis.fetch = async (url, options) => {
        request = { url, options };
        return { ok: true, status: 200, json: async () => ({ status: 'ok' }) };
    };
    try {
        const result = await requestJson('/api/test', {
            method: 'POST',
            signal: 'abort-signal',
            headers: { 'X-Request-Id': 'request-1' },
        });
        assert.deepEqual(result, { status: 'ok' });
        assert.equal(request.url, '/api/test');
        assert.equal(request.options.method, 'POST');
        assert.equal(request.options.signal, 'abort-signal');
        assert.deepEqual(request.options.headers, {
            'Content-Type': 'application/json',
            'X-Request-Id': 'request-1',
        });
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('JSON requests preserve application errors and reject malformed responses', async () => {
    const originalFetch = globalThis.fetch;
    try {
        globalThis.fetch = async () => ({
            ok: false, status: 422, json: async () => ({ error: 'invalid source' }),
        });
        await assert.rejects(requestJson('/api/test'), /invalid source/);

        globalThis.fetch = async () => ({
            ok: false, status: 502, json: async () => { throw new Error('bad body'); },
        });
        await assert.rejects(requestJson('/api/test'), /Service returned invalid JSON \(502\)\./);
    } finally {
        globalThis.fetch = originalFetch;
    }
});
