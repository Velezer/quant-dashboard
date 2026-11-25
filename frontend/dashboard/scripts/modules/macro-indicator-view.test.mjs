import test from 'node:test';
import assert from 'node:assert/strict';
import {
    formatBasisPoints,
    formatCurrency,
    formatIndex,
    formatIndexPoints,
    formatMillionBarrels,
    formatObservationChange,
    formatUsdPerBarrel,
    formatUsdPrice,
    formatUsdFlow,
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
    assert.equal(formatIndex(11), '11 / 100');
    assert.equal(formatIndex(null), 'Unavailable');
    assert.equal(formatIndexPoints(122.6503), '122.65 points');
    assert.equal(formatMillionBarrels(417.797), '417.8 million bbl');
    assert.equal(formatUsdPerBarrel(92.94), '$92.94 / bbl');
    assert.equal(formatUsdPrice(80_000), '$80,000');
    assert.equal(formatUsdFlow(-125_500_000), '−$125.5M');
    assert.equal(formatUsdFlow(1_250_000_000), '+$1.25B');
    assert.equal(formatValue(2.88, 'percent'), '2.88%');
    assert.equal(formatValue(17983.818, 'idr_per_usd'), 'IDR 17,983.82 / USD');
    assert.equal(formatValue(11, 'index_0_100'), '11 / 100');
    assert.equal(formatValue(28.44, 'index_points'), '28.44 points');
    assert.equal(formatValue(92.94, 'usd_per_barrel'), '$92.94 / bbl');
    assert.equal(formatValue(80_000, 'usd_price'), '$80,000');
    assert.equal(formatValue(417.797, 'million_barrels'), '417.8 million bbl');
    assert.equal(formatValue(-125_500_000, 'usd_flow'), '−$125.5M');
    assert.equal(formatBasisPoints(2.88), '+3 bp');
    assert.equal(formatBasisPoints(-4.6), '-5 bp');
    assert.equal(formatBasisPoints(null), 'Unavailable');
    assert.equal(formatObservationChange(0.01, 2.88, 'percent', 'from prior day'), '+1 bp from prior day');
    assert.equal(formatObservationChange(25, 17983, 'idr_per_usd', 'from prior month'), '+25.00 IDR/USD (+0.14%) from prior month');
    assert.equal(formatObservationChange(-4, 11, 'index_0_100', 'from prior day'), '−4 points from prior day');
    assert.equal(formatObservationChange(0.5, 28.44, 'index_points', 'from prior day'), '+0.50 points from prior day');
    assert.equal(formatObservationChange(-1.25, 92.94, 'usd_per_barrel', 'from prior observation'), '−$1.25 / bbl from prior observation');
    assert.equal(formatObservationChange(3.38, 417.797, 'million_barrels', 'from prior week'), '+3.4 million bbl from prior week');
    assert.equal(formatObservationChange(1_250_000_000, 2_000_000_000, 'usd_flow', 'from prior session'), '+$1.25B from prior session');
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

test('macro charts use fixed bounds for sentiment and show a zero line for flows', () => {
    const fearGreedChart = renderMacroChart([
        { date: '2026-11-21', value: 28 },
        { date: '2026-11-22', value: 11 },
    ], 'Bitcoin Fear & Greed Index', 'index_0_100');
    assert.match(fearGreedChart, />100<\/text>/);
    assert.match(fearGreedChart, />0<\/text>/);

    const flowChart = renderMacroChart([
        { date: '2026-11-18', value: 75_000_000 },
        { date: '2026-11-19', value: -125_500_000 },
    ], 'U.S. spot Bitcoin ETF net flows', 'usd_flow');
    assert.match(flowChart, /macro-chart-zero/);
    assert.match(flowChart, /\+\$\d+\.\d+M/);
    assert.match(flowChart, /−\$\d+\.\d+M/);

    const pmiChart = renderMacroChart([
        { date: '2026-09-01', value: 50.1 },
        { date: '2026-10-01', value: 49.3 },
    ], 'China official manufacturing PMI', 'index_points', 50);
    assert.match(pmiChart, /macro-chart-reference/);
    assert.match(pmiChart, />50 · reference<\/text>/);
    assert.match(pmiChart, /Reference line at 50/);
});

test('Max Pain charts format BTC strike prices and keep expiry levels visible', () => {
    const chart = renderMacroChart([
        { date: '2026-11-27', value: 80_000 },
        { date: '2026-12-25', value: 90_000 },
    ], 'Bitcoin options Max Pain', 'usd_price');
    assert.match(chart, /2026-11-27/);
    assert.match(chart, /2026-12-25/);
    assert.match(chart, /Latest value \$90,000/);
});
