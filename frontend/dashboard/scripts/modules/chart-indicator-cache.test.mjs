import test from 'node:test';
import assert from 'node:assert/strict';
import { chartIndicatorSeriesKey } from './chart-indicator-data.js';
import { clearRustIndicatorCache, rustIndicatorCacheForSeries } from './chart-indicator-cache.js';

test('retry invalidates a matching series cache after candles are reconstructed', () => {
    const original = [{ time: '100', open: 1, high: 2, low: 1, close: 2, volume: 4, dss: 73 }];
    const reloaded = original.map(candle => ({ ...candle, dss: 50, trigger: 50 }));
    const seriesKey = chartIndicatorSeriesKey(original);
    const signal = { _rustIndicatorCache: { '15m': { seriesKey, version: 'v1' } } };

    assert.equal(chartIndicatorSeriesKey(reloaded), seriesKey);
    assert.equal(rustIndicatorCacheForSeries(signal._rustIndicatorCache, '15m', seriesKey)?.version, 'v1');

    clearRustIndicatorCache(signal);
    assert.equal(rustIndicatorCacheForSeries(signal._rustIndicatorCache, '15m', seriesKey), null);
});

test('cache lookup preserves the verified version for a matching hydrated series', () => {
    const cache = { '4h': { seriesKey: 'bars-4h', version: 'engine-42' } };

    assert.deepEqual(rustIndicatorCacheForSeries(cache, '4h', 'bars-4h'), {
        seriesKey: 'bars-4h',
        version: 'engine-42',
    });
    assert.equal(rustIndicatorCacheForSeries(cache, '15m', 'bars-4h'), null);
});
