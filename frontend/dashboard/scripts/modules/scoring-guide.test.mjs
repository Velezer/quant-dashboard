import test from 'node:test';
import assert from 'node:assert/strict';
import { exampleTotals, fetchGuideCatalog, guideModels } from './scoring-guide.js';

test('the worked example adds four factors without changing their values', () => {
    assert.deepEqual(exampleTotals([8, 6, 5, 6]), { price: 14, volume: 11, total: 25 });
    assert.deepEqual(exampleTotals([0, 0, 0, 0]), { price: 0, volume: 0, total: 0 });
    assert.deepEqual(exampleTotals([10, 10, 10, 10]), { price: 20, volume: 20, total: 40 });
    assert.deepEqual(exampleTotals([1.5, 0, 3.5, 0]), { price: 1.5, volume: 3.5, total: 5 });
    for (const values of [[11, 0, 0, 0], [-1, 0, 0, 0], [NaN, 1, 2, 3], [null, 1, 2, 3], [1, 2, 3]]) {
        assert.equal(exampleTotals(values), null);
    }
});

test('catalog filters preserve separate models, apps, timeframes, products, and scales', () => {
    const stockbit = { id: 'nwe', score_model_id: 'nwe_v6', score_scale_max: 40,
        exchange_ids: ['stockbit'], product_ids: ['stockbit_regular'], timeframe: '4h' };
    const pintu = { ...stockbit, id: 'crypto_nwe', exchange_ids: ['pintu'], product_ids: ['pintu_pro_spot'] };
    const legacy = { ...stockbit, id: 'blend', score_model_id: 'blend_v1', score_scale_max: 10, timeframe: '1h' };
    const catalog = { signal_strategies: [stockbit, pintu, legacy, null, {}, { ...stockbit, score_scale_max: null }] };
    assert.deepEqual(guideModels(catalog, 'stockbit', '4h'), [stockbit]);
    assert.deepEqual(guideModels(catalog, 'stockbit'), [stockbit, legacy]);
    assert.deepEqual(guideModels(catalog, 'pintu', '4h'), [pintu]);
    assert.deepEqual(guideModels(catalog, 'tokocrypto'), []);
    assert.deepEqual(guideModels(null), []);
    assert.equal(guideModels(catalog)[2].score_scale_max, 10);
});

test('catalog reads the existing API and accepts an explicit empty catalog', async () => {
    const empty = { signal_strategies: [] };
    const result = await fetchGuideCatalog(async (url, options) => {
        assert.equal(url, '/api/master-data');
        assert.equal(options.headers.Accept, 'application/json');
        assert.equal(options.cache, 'no-store');
        assert(options.signal instanceof AbortSignal);
        return { ok: true, json: async () => empty };
    });
    assert.equal(result, empty);
});

test('HTTP, network, malformed payload, and JSON failures do not become an empty success', async () => {
    await assert.rejects(fetchGuideCatalog(async () => ({ ok: false, status: 503 })), /503/);
    await assert.rejects(fetchGuideCatalog(async () => { throw new Error('offline'); }), /offline/);
    for (const payload of [null, {}, { signal_strategies: 'invalid' }]) {
        await assert.rejects(fetchGuideCatalog(async () => ({ ok: true, json: async () => payload })), /invalid model catalog/);
    }
    await assert.rejects(fetchGuideCatalog(async () => ({ ok: true, json: async () => { throw new Error('invalid JSON'); } })), /invalid JSON/);
});
