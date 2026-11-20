import test from 'node:test';
import assert from 'node:assert/strict';
import { formatBasisPoints, formatYield, observationsForRange, renderYieldChart } from './macro-indicator-view.js';

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

test('yield and change presenters retain the real-yield sign and units', () => {
    assert.equal(formatYield(-0.25), '-0.25%');
    assert.equal(formatYield(null), 'Unavailable');
    assert.equal(formatBasisPoints(2.88), '+3 bp');
    assert.equal(formatBasisPoints(-4.6), '-5 bp');
    assert.equal(formatBasisPoints(null), 'Unavailable');
});

test('macro chart handles a flat one-observation range and invalid inputs', () => {
    const singlePoint = renderYieldChart([{ date: '2026-11-18', value: 2.88 }]);
    assert.match(singlePoint, /<circle class="macro-chart-point"/);
    assert.match(singlePoint, /2\.88%/);
    assert.match(renderYieldChart([]), /No observations are available/);
    assert.deepEqual(observationsForRange([{ date: 'bad', value: 'NaN' }], '10y'), []);
});
