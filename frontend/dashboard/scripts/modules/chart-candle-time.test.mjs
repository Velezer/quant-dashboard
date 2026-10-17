import test from 'node:test';
import assert from 'node:assert/strict';
import { formatChartCandleLabel, formatChartCandleTimestamp, isIntradayChartTimeframe } from './chart-candle-time.js';

const januaryFirstUtcNanoseconds = Date.UTC(2024, 0, 1) * 1_000_000;

test('chart time policy shares intraday timeframe classification', () => {
    assert.equal(isIntradayChartTimeframe('15M'), true);
    assert.equal(isIntradayChartTimeframe('30m'), true);
    assert.equal(isIntradayChartTimeframe('2h'), true);
    assert.equal(isIntradayChartTimeframe('4h'), true);
    assert.equal(isIntradayChartTimeframe('1d'), false);
});

test('chart timestamps render date or date-time in Jakarta consistently', () => {
    assert.equal(formatChartCandleTimestamp(januaryFirstUtcNanoseconds, '1d'), '2024-01-01');
    assert.equal(formatChartCandleTimestamp(januaryFirstUtcNanoseconds, '1h'), '01-01 07:00');
    assert.equal(formatChartCandleTimestamp(januaryFirstUtcNanoseconds, '2h'), '01-01 07:00');
    assert.equal(formatChartCandleTimestamp(januaryFirstUtcNanoseconds, '15m', { includeWib: true }), '01-01 07:00 WIB');
    assert.equal(formatChartCandleTimestamp('invalid', '1d'), null);
});

test('chart labels preserve stored time and use a clear fallback for missing timestamps', () => {
    assert.equal(formatChartCandleLabel({ date: '01-01 07:00', time: januaryFirstUtcNanoseconds }, '1h'), '01-01 07:00 WIB');
    assert.equal(formatChartCandleLabel({ date: '2024-01-01', time: januaryFirstUtcNanoseconds }, '1d'), '2024-01-01');
    assert.equal(formatChartCandleLabel({ date: '01-01 07:00 WIB' }, '1h'), '01-01 07:00 WIB');
    assert.equal(formatChartCandleLabel({}, '1d'), 'Latest Bar');
});
