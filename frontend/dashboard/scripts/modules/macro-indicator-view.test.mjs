import test from 'node:test';
import assert from 'node:assert/strict';
import {
    formatBasisPoints,
    formatCurrency,
    formatObservationChange,
    formatValue,
    formatYield,
    observationsForRange,
    renderMacroChart,
} from './macro-indicator-view.js';

const daily = [
    { date: '2024-01-02', value: 1.25 },
    { date: '2025-01-02', value: 1.50 },
    { date: '2026-01-02', value: -0.25 },
    { date: '2026-11-18', value: 2.88 },
];

test('macro range selection uses the newest observation as its anchor', () => {
    assert.deepEqual(observationsForRange(daily, '1y').map(row => row.date), ['2026-01-02', '2026-11-18']);
    assert.deepEqual(observationsForRange(daily, '5y'), daily);
    assert.deepEqual(observationsForRange(daily, 'unknown').map(row => row.date), ['2026-01-02', '2026-11-18']);
});

test('macro value and change presenters preserve each indicator unit', () => {
    assert.equal(formatYield(-0.25), '-0.25%');
    assert.equal(formatYield(null), 'Unavailable');
    assert.equal(formatCurrency(17983.818), 'IDR 17,983.82 / USD');
    assert.equal(formatCurrency(null), 'Unavailable');
    assert.equal(formatValue(2.88, 'percent'), '2.88%');
    assert.equal(formatValue(17983.818, 'idr_per_usd'), 'IDR 17,983.82 / USD');
    assert.equal(formatBasisPoints(2.88), '+3 bp');
    assert.equal(formatBasisPoints(-4.6), '-5 bp');
    assert.equal(formatBasisPoints(null), 'Unavailable');
    assert.equal(formatObservationChange(0.01, 2.88, 'percent', 'from prior day'), '+1 bp from prior day');
    assert.equal(formatObservationChange(25, 17983, 'idr_per_usd', 'from prior month'), '+25.00 IDR/USD (+0.14%) from prior month');
    assert.equal(formatObservationChange(null, 2.88), 'Unavailable');
});

test('generic macro chart formats both yield and exchange-rate axes', () => {
    const singlePoint = renderMacroChart([{ date: '2026-11-18', value: 2.88 }], '10-Year TIPS');
    assert.match(singlePoint, /<circle class="macro-chart-point"/);
    assert.match(singlePoint, /2\.88%/);

    const fxChart = renderMacroChart([
        { date: '2026-09-01', value: 17743 },
        { date: '2026-10-01', value: 17983.8 },
    ], 'USD\/IDR', 'idr_per_usd');
    assert.match(fxChart, /17,983/);
    assert.match(fxChart, /IDR 17,983\.80 \/ USD/);
    assert.match(renderMacroChart([]), /No observations are available/);
    assert.deepEqual(observationsForRange([{ date: 'bad', value: 'NaN' }], '10y'), []);
});
