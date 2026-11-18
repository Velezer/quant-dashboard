import test from 'node:test';
import assert from 'node:assert/strict';
import { chartCandlesForTimeframe, chartTimeframeWasLoaded } from './chart-timeframe-candles.js';

test('an explicitly empty timeframe never borrows the mutable candles array', () => {
    const fourHourBars = [{ time: 4, close: 4 }];
    const signal = {
        timeframe: '15m',
        candles: fourHourBars,
        candles_by_tf: { '15m': [], '4h': fourHourBars },
    };

    assert.deepEqual(chartCandlesForTimeframe(signal, '15m'), []);
    assert.equal(chartCandlesForTimeframe(signal, '4h'), fourHourBars);
});

test('only the signal timeframe may use candles when its map key is absent', () => {
    const signalBars = [{ time: 15, close: 15 }];
    const signal = { timeframe: '15m', candles: signalBars, candles_by_tf: { '4h': [] } };

    assert.equal(chartCandlesForTimeframe(signal, '15m'), signalBars);
    assert.deepEqual(chartCandlesForTimeframe(signal, '4h'), []);
});

test('switching to an unvisited frame cannot reuse candles tagged with the prior frame', () => {
    const fourHourBars = [{ time: 4, close: 4 }];
    const signal = {
        timeframe: '15m',
        candles: fourHourBars,
        _chartCandlesTimeframe: '4h',
        candles_by_tf: { '4h': fourHourBars },
    };

    assert.deepEqual(chartCandlesForTimeframe(signal, '15m'), []);
    assert.equal(chartCandlesForTimeframe(signal, '4h'), fourHourBars);
});

test('an empty frame is a cache hit only after that exact frame loaded', () => {
    const signal = {
        candles_by_tf: { '15m': [], '4h': [] },
        chart_data_loaded: true,
        chart_data_loaded_by_tf: { '4h': true },
    };

    assert.equal(chartTimeframeWasLoaded(signal, '15m'), false);
    assert.equal(chartTimeframeWasLoaded(signal, '4h'), true);
});
