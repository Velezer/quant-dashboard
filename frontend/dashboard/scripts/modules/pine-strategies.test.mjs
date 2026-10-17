import test from 'node:test';
import assert from 'node:assert/strict';
import { chronologicalValidationRows, closedTradeProfitPercent, displayCandleTimeUtc, magnifierCoverageSummary, monthlyBacktestRows, parseBacktestDateRange, tradeAnalysisRows, tradeDisplayValues, tradeReturnDistributionBins } from './pine-strategies.js';
import { renderPineBacktestSummaryMetrics } from './pine-strategy-presenters.js';

test('Pine backtest summary uses fee-aware closed-trade P&L and labels its basis', () => {
    const values = new Map();
    const summary = { querySelector: selector => ({ set textContent(value) { values.set(selector, value); } }) };
    const pnlLabel = { textContent: '' };
    const pnlPercentLabel = { textContent: '' };
    renderPineBacktestSummaryMetrics(summary, pnlLabel, pnlPercentLabel, {
        net_profit: 80, gross_profit: 100, gross_loss: 20, initial_capital: 1_000,
        closed_trades: 2, win_rate_pct: 50, profit_factor_unbounded: true,
    }, { known: true });
    assert.equal(values.get('[data-metric="net-profit"]'), '80.00');
    assert.equal(values.get('[data-metric="net-profit-pct"]'), '8.00%');
    assert.equal(values.get('[data-metric="profit-factor"]'), '∞');
    assert.equal(pnlLabel.textContent, 'Net closed-trade P&L');
    assert.equal(pnlPercentLabel.textContent, 'Net closed-trade return · % of initial capital');

    renderPineBacktestSummaryMetrics(summary, pnlLabel, pnlPercentLabel, {
        gross_profit: 100, gross_loss: 20, initial_capital: 1_000,
    }, { known: false });
    assert.equal(values.get('[data-metric="net-profit"]'), '80.00');
    assert.equal(pnlLabel.textContent, 'Gross closed-trade P&L · fees unknown');
    assert.equal(pnlPercentLabel.textContent, 'Gross closed-trade return · % of initial capital · fees unknown');
});

test('closed-trade return uses the displayed P&L basis over initial capital', () => {
    assert.equal(closedTradeProfitPercent(125, 1_000), 12.5);
    assert.equal(closedTradeProfitPercent(-50, 1_000), -5);
    assert.equal(closedTradeProfitPercent(10, 0), null);
    assert.equal(closedTradeProfitPercent(Number.NaN, 1_000), null);
});

test('Bar Magnifier summary identifies aggregate coverage and fallback OHLC bars', () => {
    const summary = magnifierCoverageSummary({
        bar_magnifier_enabled: true, bar_magnifier_timeframe: '15m',
        bar_magnifier_covered_chart_bars: 8, bar_magnifier_fallback_chart_bars: 2,
        bar_magnifier_chart_bars: 10,
    });
    assert.equal(summary.state, 'ready');
    assert.match(summary.text, /aggregate-matched on 8 of 10 chart bars; 2 bars used inferred chart OHLC/);
    assert.match(summary.text, /does not prove every lower-timeframe interval is present/);
});

test('Bar Magnifier summary handles disabled, missing, and inconsistent coverage', () => {
    assert.match(magnifierCoverageSummary({ bar_magnifier_enabled: false }).text, /disabled/);
    assert.equal(magnifierCoverageSummary({}).state, 'unavailable');
    assert.equal(magnifierCoverageSummary({
        bar_magnifier_enabled: true, bar_magnifier_covered_chart_bars: 4,
        bar_magnifier_fallback_chart_bars: 2, bar_magnifier_chart_bars: 10,
    }).state, 'unavailable');
});

test('Pine trade analysis presents full-sample all, long, and short rows', () => {
    const rows = tradeAnalysisRows({
        all: { trades: 12, wins: 7, losses: 4, win_rate_pct: 58.333, average_win_return_pct: 3.2, average_loss_return_pct: -1.5, largest_win_pnl: 120, largest_loss_pnl: -45, max_favorable_excursion: { average: 18, maximum: 100 }, max_adverse_excursion: { average: 14, maximum: 70 }, favorable_excursion_pct: { average: 4.2 }, adverse_excursion_pct: { average: -3.1 } },
        long: { trades: 8, wins: 5, losses: 3, win_rate_pct: 62.5, average_win_return_pct: 3.8, average_loss_return_pct: -1.2, largest_win_pnl: 120, largest_loss_pnl: -25, max_favorable_excursion: { average: 20, maximum: 110 }, max_adverse_excursion: { average: 12, maximum: 60 }, favorable_excursion_pct: { average: 5 }, adverse_excursion_pct: { average: -2.8 } },
        short: { trades: 4, wins: 2, losses: 1, win_rate_pct: 50, average_win_return_pct: 2, average_loss_return_pct: -2.4, largest_win_pnl: 30, largest_loss_pnl: -45, max_favorable_excursion: { average: 14, maximum: 80 }, max_adverse_excursion: { average: 18, maximum: 90 }, favorable_excursion_pct: { average: 3 }, adverse_excursion_pct: { average: -3.7 } },
    });

    assert.deepEqual(rows.map(row => row[0]), ['All', 'Long', 'Short']);
    assert.equal(rows[0][1], '12');
    assert.equal(rows[0][4], '58.33%');
    assert.equal(rows[1][5], '3.80%');
    assert.equal(rows[2][8], '-45.00');
    assert.equal(rows[0][9], '18.00');
    assert.equal(rows[0][10], '100.00');
    assert.equal(rows[1][13], '5.00%');
    assert.equal(rows[2][14], '-3.70%');
});

test('Pine trade analysis stays unavailable for partial responses', () => {
    assert.equal(tradeAnalysisRows({ all: { trades: 1 }, long: { trades: 1 } }), null);
});

test('Pine closed-trade details show the selected P&L basis and signed net excursions', () => {
    const trade = {
        direction: 'SHORT', entry_id: 'S1', entry_price: 100, exit_price: 95, quantity: 2,
        gross_pnl: 10, net_pnl: 8, gross_return_pct: 5, net_return_pct: 4,
        max_runup: 12, max_drawdown: 6, max_runup_percent: 6, max_drawdown_percent: -3,
    };
    const values = tradeDisplayValues(trade, true, 4);
    assert.equal(values[0], '4 · SHORT · S1');
    assert.equal(values[6], '8.00');
    assert.equal(values[7], '4.00%');
    assert.equal(values[8], '12.00');
    assert.equal(values[9], '6.00');
    assert.equal(values[10], '6.00%');
    assert.equal(values[11], '-3.00%');

    assert.equal(tradeDisplayValues(trade, false)[6], '10.00');
    assert.equal(tradeDisplayValues({}, false)[8], '—');
});

test('Pine monthly performance groups by UTC month and uses the prior observed equity as its base', () => {
    const points = [
        { time: Date.UTC(2025, 0, 31, 23) * 1_000_000, equity: 1_100, buy_and_hold_equity: 1_100 },
        { time: Date.UTC(2025, 1, 1) * 1_000_000, equity: 1_210, buy_and_hold_equity: 1_210 },
        { time: Date.UTC(2025, 1, 28, 23) * 1_000_000, equity: 1_331, buy_and_hold_equity: 1_221 },
    ];
    const rows = monthlyBacktestRows(points, 1_000);
    assert.deepEqual(rows.map(row => row.month), ['2025-01', '2025-02']);
    assert.equal(rows[0].net_pnl, 100);
    assert.ok(Math.abs(rows[0].return_pct - 10) < 1e-10);
    assert.equal(rows[1].net_pnl, 231);
    assert.ok(Math.abs(rows[1].return_pct - 21) < 1e-10);
    assert.ok(Math.abs(rows[1].buy_and_hold_return_pct - 11) < 1e-10);
});

test('Pine monthly performance rejects malformed equity samples and handles missing benchmark values', () => {
    assert.equal(monthlyBacktestRows([], 1_000), null);
    assert.equal(monthlyBacktestRows([{ time: 1, equity: 1_000 }], 0), null);
    assert.equal(monthlyBacktestRows([{ time: 2, equity: 1_000 }, { time: 1, equity: 1_100 }], 1_000), null);
    const nonpositiveRows = monthlyBacktestRows([
        { time: Date.UTC(2025, 0, 31) * 1_000_000, equity: 0 },
        { time: Date.UTC(2025, 1, 1) * 1_000_000, equity: 25 },
    ], 1_000);
    assert.equal(nonpositiveRows[0].return_pct, -100);
    assert.equal(nonpositiveRows[1].return_pct, null);
    assert.equal(nonpositiveRows[1].net_pnl, 25);
    const [row] = monthlyBacktestRows([{ time: 1_700_000_000_000_000_000, equity: 1_050 }], 1_000);
    assert.ok(Math.abs(row.return_pct - 5) < 1e-10);
    assert.equal(row.buy_and_hold_return_pct, null);
});

test('Pine backtest candle timestamps render from nanoseconds as UTC', () => {
    assert.equal(displayCandleTimeUtc(1_700_000_000_000_000_000), '2023-11-14 22:13 UTC');
    assert.equal(displayCandleTimeUtc(null), '—');
});

test('chronological validation rows show selected periods and comparison metrics', () => {
    const rows = chronologicalValidationRows({
        available: true,
        training_start_time: Date.UTC(2025, 0, 1) * 1_000_000,
        training_end_time: Date.UTC(2025, 0, 20) * 1_000_000,
        validation_start_time: Date.UTC(2025, 0, 21) * 1_000_000,
        validation_end_time: Date.UTC(2025, 0, 31) * 1_000_000,
        training_bars: 20,
        validation_bars: 11,
        split_index: 4,
        training: {
            closed_trades: 4, win_rate_pct: 50, win_rate_interval_95_pct: [15, 85], closed_trade_pnl: 125,
            marked_return_pct: 12.5, profit_factor: 1.8,
            close_drawdown: { available: true, max_drawdown_pct: 4.2 },
            monthly_marked_returns: [
                { month: '2024-11', return_pct: 5 },
                { month: '2024-12', return_pct: 4 },
                { month: '2025-01', return_pct: 3 },
            ],
        },
        validation: {
            closed_trades: 2, win_rate_pct: 50, win_rate_interval_95_pct: [9, 91], closed_trade_pnl: -30,
            marked_return_pct: -3, profit_factor: null, profit_factor_unbounded: false,
            close_drawdown: { available: true, max_drawdown_pct: 6 },
            monthly_marked_returns: [
                { month: '2025-01', return_pct: -1 },
                { month: '2025-02', return_pct: -3 },
            ],
        },
    });

    assert.equal(rows.length, 3);
    assert.equal(rows[0].period, 'Training');
    assert.match(rows[0].dates, /2025-01-01 00:00 UTC/);
    assert.equal(rows[0].bars, '20');
    assert.equal(rows[0].closedPnl, '125.00');
    assert.equal(rows[0].winRateInterval, '15.00%–85.00%');
    assert.equal(rows[0].profitFactor, '1.80');
    assert.equal(rows[1].period, 'Validation tail');
    assert.equal(rows[1].markedReturn, '-3.00%');
    assert.equal(rows[1].closeDrawdown, '6.00%');
    assert.equal(rows[2].period, 'Tail vs recent training');
    assert.equal(rows[2].closeDrawdown, '—');
    assert.match(rows[2].monthlyStability, /mean monthly marked return/);
});

test('chronological validation rows remain unavailable for missing or short samples', () => {
    assert.equal(chronologicalValidationRows(null), null);
    assert.equal(chronologicalValidationRows({ available: false, reason: 'sample too short' }), null);
    assert.equal(chronologicalValidationRows({ available: true, training: {}, validation: null }), null);
});

test('Pine return distribution validates all full-sample bin counts', () => {
    const bins = tradeReturnDistributionBins({
        available: true,
        sample_trades: 7,
        bins: [
            { lower_return_pct: -10, upper_return_pct: -3.33, count: 2 },
            { lower_return_pct: -3.33, upper_return_pct: 3.33, count: 4 },
            { lower_return_pct: 3.33, upper_return_pct: 10, count: 1 },
        ],
    });
    assert.equal(bins.length, 3);
    assert.equal(bins.reduce((total, bin) => total + bin.count, 0), 7);
    assert.equal(tradeReturnDistributionBins({ available: true, sample_trades: 7, bins: [{ lower_return_pct: 0, upper_return_pct: 1, count: 6 }] }), null);
    assert.equal(tradeReturnDistributionBins({ available: false, sample_trades: 0, bins: [] }), null);
});
