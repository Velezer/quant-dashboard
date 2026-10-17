import test from 'node:test';
import assert from 'node:assert/strict';
import { accountGrowthChartModel, formatUsdBalance } from './rule-simulator.js';
import { createRuleSimulatorGrowthChart } from './rule-simulator-growth-chart.js';

test('USD balance formatting makes the configured starting unit clear', () => {
    assert.equal(formatUsdBalance(100), '$100.00');
    assert.equal(formatUsdBalance(12_060.79), '$12,060.79');
    assert.equal(formatUsdBalance(Number.NaN), 'Unavailable');
});

function projection(overrides = {}) {
    const points = [
        { trading_day: 0, expected_balance_units: 10_000 },
        { trading_day: 1, expected_balance_units: 10_350 },
        { trading_day: 2, expected_balance_units: 10_712.25 },
    ];
    return {
        fee_adjusted: true,
        starting_balance_units: 10_000,
        expected_ending_balance_units: 10_712.25,
        trade_size_pct_of_equity: 50,
        trades_per_day: 5,
        trading_days: 2,
        expected_balance_growth_per_trade_pct: 0.69,
        points,
        ...overrides,
    };
}

test('growth chart model carries account balances and padded axis bounds', () => {
    const model = accountGrowthChartModel(projection());

    assert.deepEqual(model.points, projection().points);
    assert.equal(model.scale, 'linear');
    assert.equal(model.minBalance, 10_000);
    assert.equal(model.maxBalance, 10_712.25);
    assert.ok(model.axisMinimum < model.minBalance);
    assert.ok(model.axisMaximum > model.maxBalance);
});

test('growth chart model accepts a zero balance and keeps a finite display axis', () => {
    const model = accountGrowthChartModel(projection({
        expected_ending_balance_units: 0,
        points: [
            { trading_day: 0, expected_balance_units: 10_000 },
            { trading_day: 1, expected_balance_units: 0 },
            { trading_day: 2, expected_balance_units: 0 },
        ],
    }));

    assert.equal(model.maxBalance, 10_000);
    assert.equal(model.scale, 'linear');
    assert.ok(Number.isFinite(model.axisMinimum));
    assert.ok(Number.isFinite(model.axisMaximum));
});

test('growth chart model accepts the full ten-year daily horizon', () => {
    const tradingDays = 3_650;
    const points = Array.from({ length: tradingDays + 1 }, (_, trading_day) => ({
        trading_day,
        expected_balance_units: 100 * (1_000 ** (trading_day / tradingDays)),
    }));
    const model = accountGrowthChartModel(projection({
        starting_balance_units: 100,
        trading_days: tradingDays,
        expected_ending_balance_units: 100_000,
        points,
    }));

    assert.ok(model);
    assert.equal(model.points.length, 3_651);
    assert.equal(model.points.at(-1).trading_day, tradingDays);
    assert.equal(model.maxBalance, 100_000);
    assert.equal(model.scale, 'log');
    assert.ok(model.logAxisMinimum < Math.log10(model.minBalance));
    assert.ok(model.logAxisMaximum > Math.log10(model.maxBalance));
});

test('growth chart model rejects mismatched, malformed, or non-finite projections', () => {
    assert.equal(accountGrowthChartModel(projection({ trading_days: 3 })), null);
    assert.equal(accountGrowthChartModel(projection({ points: [null, null, null] })), null);
    assert.equal(accountGrowthChartModel(projection({
        points: [
            { trading_day: 0, expected_balance_units: 10_000 },
            { trading_day: 1, expected_balance_units: Number.NaN },
            { trading_day: 2, expected_balance_units: 10_712.25 },
        ],
    })), null);
    assert.equal(accountGrowthChartModel(projection({ fee_adjusted: 'unknown' })), null);
});

test('growth chart presenter renders its accessible projection and fee state', () => {
    const outputs = new Map();
    const accessibleTitle = { textContent: '' };
    const description = { textContent: '' };
    const plot = { innerHTML: '' };
    const chart = { classList: { remove: value => outputs.set('chart-class-removed', value) } };
    const nodes = new Map([
        ['#ruleSimulatorGrowthChart', chart],
        ['#ruleSimulatorChartPlot', plot],
        ['#ruleSimulatorChartAccessibleTitle', accessibleTitle],
        ['#ruleSimulatorChartDescription', description],
    ]);
    const render = createRuleSimulatorGrowthChart({
        dialog: { querySelector: selector => nodes.get(selector) },
        accountGrowthChartModel,
        formatBalance: value => `$${value.toFixed(2)}`,
        formatAxisBalance: value => `$${value.toFixed(2)}`,
        formatGrowthHorizon: days => `${days} modeled days`,
        getTradingDaysPerYear: () => 252,
        setOutput: (id, value) => outputs.set(id, value),
        clearGrowthChart: message => outputs.set('cleared', message),
    });

    render(projection(), { win_rate_pct: 55 }, { trade_size_pct_of_equity: 50 });

    assert.equal(accessibleTitle.textContent, 'Fee-adjusted expected account growth projection');
    assert.match(description.textContent, /55% win rate/);
    assert.match(plot.innerHTML, /<path d="M 100\.00/);
    assert.equal(outputs.get('ruleSimulatorEndingBalance'), 'Expected ending balance: $10712.25 USD');
    assert.match(outputs.get('ruleSimulatorChartNote'), /probability-weighted outcome/);
    assert.equal(outputs.get('chart-class-removed'), 'hidden');
});
