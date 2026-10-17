import test from 'node:test';
import assert from 'node:assert/strict';
import { CHART_INDICATOR_IDS, finiteChartIndicatorValue, normalizeChartIndicators } from './chart-indicator-data.js';

test('finite indicator values normalize numeric inputs and reject invalid values', () => {
    assert.equal(finiteChartIndicatorValue('1.25'), 1.25);
    assert.equal(finiteChartIndicatorValue(null), null);
    assert.equal(finiteChartIndicatorValue('invalid'), null);
    assert.equal(finiteChartIndicatorValue(Infinity), null);
});

test('indicator normalization keeps finite supported modes and true crossing flags only', () => {
    const result = normalizeChartIndicators({
        [CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]: {
            repainting: { upper: '12.5', lower: 'bad', crossed_above_upper: true, crossed_below_lower: 1 },
            endpoint: { upper: null, lower: 9 },
            future_mode: { upper: 99 },
        },
        unknown_indicator: { value: 1 },
    });
    assert.deepEqual(result, {
        [CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]: {
            repainting: { upper: 12.5, crossed_above_upper: true },
            endpoint: { lower: 9 },
        },
    });
});

test('indicator normalization returns empty objects for missing or unusable payloads', () => {
    assert.deepEqual(normalizeChartIndicators(null), {});
    assert.deepEqual(normalizeChartIndicators({}), {});
    assert.deepEqual(normalizeChartIndicators({
        [CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]: { endpoint: { upper: 'NaN' } },
    }), {});
});
