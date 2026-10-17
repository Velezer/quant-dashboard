import test from 'node:test';
import assert from 'node:assert/strict';
import { requestAllInstruments } from './api.js';

test('all-instrument requests fetch bounded pages and report progress', async () => {
    const originalFetch = globalThis.fetch;
    const requests = [];
    const progress = [];
    const signal = new AbortController().signal;
    globalThis.fetch = async (url, options) => {
        requests.push({ url, options });
        const offset = Number(new URL(url, 'http://localhost').searchParams.get('offset'));
        const rows = offset === 0
            ? Array.from({ length: 500 }, (_, index) => ({ symbol: 'T' + index }))
            : [{ symbol: 'LAST' }];
        return {
            ok: true,
            status: 200,
            json: async () => ({
                registered_count: 501,
                matching_count: 501,
                next_offset: offset + rows.length,
                has_more: offset === 0,
                instruments: rows,
            }),
        };
    };
    try {
        const result = await requestAllInstruments({
            signal,
            onPage: value => progress.push(value),
        });
        assert.equal(result.instruments.length, 501);
        assert.equal(result.instruments.at(-1).symbol, 'LAST');
        assert.deepEqual(progress, [
            { loaded: 500, total: 501 },
            { loaded: 501, total: 501 },
        ]);
        assert.deepEqual(
            requests.map(({ url }) => new URL(url, 'http://localhost').searchParams.get('limit')),
            ['500', '500'],
        );
        assert.deepEqual(
            requests.map(({ url }) => new URL(url, 'http://localhost').searchParams.get('offset')),
            ['0', '500'],
        );
        assert.ok(requests.every(({ options }) => options.signal === signal));
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('all-instrument requests reject a non-advancing page cursor', async () => {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => ({
        ok: true,
        status: 200,
        json: async () => ({
            registered_count: 1,
            matching_count: 1,
            next_offset: 0,
            has_more: true,
            instruments: [{ symbol: 'BBCA' }],
        }),
    });
    try {
        await assert.rejects(requestAllInstruments(), /invalid page offset/);
    } finally {
        globalThis.fetch = originalFetch;
    }
});
