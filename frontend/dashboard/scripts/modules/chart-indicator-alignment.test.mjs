import test from 'node:test';
import assert from 'node:assert/strict';
import {chartIndicatorPointsByTime, chartIndicatorSeriesKey} from './chart-indicator-data.js';

test('30m indicator points match candle timestamps despite missing or newer bars', () => {
    const points = chartIndicatorPointsByTime([{time: 1800, dss: 10}, {time: 5400, dss: 80}]);
    assert.equal(points.get('1800').dss, 10);
    assert.equal(points.get('3600'), undefined);
    assert.equal(points.get('5400').dss, 80);
});
test('same candle count cannot hide a changed candle or moving time window', () => {
    const bars = [{time: 1800, open: 1, high: 3, low: 1, close: 2, volume: 5}];
    assert.notEqual(chartIndicatorSeriesKey(bars), chartIndicatorSeriesKey([{...bars[0], close: 3}]));
    assert.notEqual(chartIndicatorSeriesKey(bars), chartIndicatorSeriesKey([{...bars[0], time: 3600}]));
});
