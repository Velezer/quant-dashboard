import test from 'node:test';
import assert from 'node:assert/strict';
import { pineWorkerCandle, pineWorkerInput } from './pine-indicator-input.js';
import { CHART_INDICATOR_IDS } from './chart-indicator-data.js';
import { NWE_LOWER_SOURCE_TITLE, NWE_UPPER_SOURCE_TITLE } from './pine-sources.js';

test('worker candle projection keeps OHLCV and normalizes invalid time', () => {
    assert.deepEqual(pineWorkerCandle({
        open: 1, high: 3, low: 0.5, close: 2, volume: 5, time: 'invalid', ignored: 'value',
    }), { open: 1, high: 3, low: 0.5, close: 2, volume: 5, time: null });
});

test('worker input preserves timeframe candles and selects the configured NWE mode', () => {
    const indicator = CHART_INDICATOR_IDS.nadarayaWatsonEnvelope;
    const candles = [{
        open: 1, high: 3, low: 0.5, close: 2, volume: 5, time: 10,
        indicators: { [indicator]: { endpoint: { upper: '2.5', lower: 0.75 }, repainting: { upper: 4 } } },
    }, {
        open: 2, high: 4, low: 1, close: 3, volume: 6, time: 11,
        indicators: { [indicator]: { endpoint: { upper: 'bad', lower: null } } },
    }];
    const result = pineWorkerInput({
        source: 'plot(close)',
        signal: { candles_by_tf: { '1h': [{ ...candles[0] }], '4h': null } },
        candles,
        timeframe: '1h',
        repainting: false,
        scriptType: 'strategy',
    });
    assert.equal(result.mode, 'strategy');
    assert.equal(result.timeframes['1h'].length, 1);
    assert.deepEqual(result.timeframes['4h'], []);
    assert.equal(result.candles[0].time, 10);
    assert.deepEqual(result.input_sources[NWE_UPPER_SOURCE_TITLE], [2.5, null]);
    assert.deepEqual(result.input_sources[NWE_LOWER_SOURCE_TITLE], [0.75, null]);
});

test('worker input falls back to selected candles and honors repainting override', () => {
    const candle = {
        open: 1, high: 3, low: 0.5, close: 2, volume: 5, time: 10,
        indicators: { [CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]: { repainting: { upper: 5, lower: 1 } } },
    };
    const result = pineWorkerInput({
        source: '', signal: {}, candles: [candle], timeframe: '15m', repainting: true, modeOverride: 'endpoint',
    });
    assert.deepEqual(result.timeframes['15m'], result.candles);
    assert.deepEqual(result.input_sources[NWE_UPPER_SOURCE_TITLE], [null]);
});
