import { displayCandleTimeUtc } from './pine-backtest-jobs.js';

export function displayPercent(value) {
    const number = value == null ? NaN : Number(value);
    return Number.isFinite(number) ? `${number.toFixed(2)}%` : '—';
}

export function displayNumber(value) {
    const number = value == null ? NaN : Number(value);
    return Number.isFinite(number) ? number.toFixed(2) : '—';
}

export function displayCount(value) {
    const number = value == null ? NaN : Number(value);
    return Number.isSafeInteger(number) && number >= 0 ? String(number) : '—';
}

export function closedTradeProfitPercent(pnl, initialCapital) {
    const profit = Number(pnl);
    const capital = Number(initialCapital);
    return Number.isFinite(profit) && Number.isFinite(capital) && capital > 0
        ? profit / capital * 100
        : null;
}

export function renderPineBacktestSummaryMetrics(summary, pnlLabel, pnlPercentLabel, result, fees) {
    const closedTradePnl = fees.known === true
        ? result.net_profit
        : Number(result.gross_profit) - Number(result.gross_loss);
    const fields = {
        trades: result.closed_trades,
        'win-rate': result.win_rate_pct == null ? null : `${displayPercent(result.win_rate_pct)} · ${result.closed_trades} trades`,
        'mean-return': result.mean_trade_return_pct == null ? null : `${displayPercent(result.mean_trade_return_pct)} · ${result.return_basis || 'unknown basis'}`,
        'profit-factor': result.profit_factor_unbounded === true ? '∞' : result.profit_factor,
        'net-profit': closedTradePnl,
        'net-profit-pct': closedTradeProfitPercent(closedTradePnl, result.initial_capital),
        'max-drawdown': result.max_drawdown_pct == null ? null : displayPercent(result.max_drawdown_pct),
        'buy-hold-return': result.buy_and_hold_return_pct == null ? null : displayPercent(result.buy_and_hold_return_pct),
        'close-drawdown': result.close_drawdown?.available
            ? `${displayPercent(result.close_drawdown.max_drawdown_pct)} · ${displayCount(result.close_drawdown.duration_bars)} bars to trough`
            : null,
        'current-drawdown': result.close_drawdown?.available
            ? `${displayPercent(result.close_drawdown.current_drawdown_pct)} · ${displayCount(result.close_drawdown.current_duration_bars)} bars`
            : null,
    };
    for (const [key, value] of Object.entries(fields)) {
        const target = summary.querySelector(`[data-metric="${key}"]`);
        if (target) target.textContent = key === 'trades'
            ? displayCount(value)
            : key === 'net-profit-pct'
                ? displayPercent(value)
                : key === 'win-rate' || key === 'mean-return' || key === 'max-drawdown' || key === 'buy-hold-return' || key === 'close-drawdown' || key === 'current-drawdown' || key === 'profit-factor' && value === '∞'
                    ? (value ?? '—')
                    : displayNumber(value);
    }
    if (pnlLabel) pnlLabel.textContent = fees.known === true
        ? 'Net closed-trade P&L'
        : 'Gross closed-trade P&L · fees unknown';
    if (pnlPercentLabel) pnlPercentLabel.textContent = `${fees.known === true ? 'Net' : 'Gross'} closed-trade return · % of initial capital${fees.known === true ? '' : ' · fees unknown'}`;
}

export function magnifierCoverageSummary(assumptions) {
    if (assumptions?.bar_magnifier_enabled !== true) {
        return assumptions?.bar_magnifier_enabled === false
            ? { state: 'ready', text: 'Bar Magnifier disabled · fills use inferred chart OHLC paths.' }
            : { state: 'unavailable', text: 'Bar Magnifier coverage is unavailable in this response.' };
    }
    const covered = assumptions.bar_magnifier_covered_chart_bars;
    const fallback = assumptions.bar_magnifier_fallback_chart_bars;
    const chartBars = assumptions.bar_magnifier_chart_bars;
    if (![covered, fallback, chartBars].every(value => Number.isSafeInteger(value) && value >= 0) || covered + fallback !== chartBars) {
        return { state: 'unavailable', text: 'Bar Magnifier coverage counts are unavailable or inconsistent. Treat fill-path provenance as unknown for this run.' };
    }
    const timeframe = assumptions.bar_magnifier_timeframe || 'lower timeframe';
    return {
        state: 'ready',
        text: `Bar Magnifier · ${timeframe} OHLC aggregate-matched on ${displayCount(covered)} of ${displayCount(chartBars)} chart bars; ${displayCount(fallback)} bars used inferred chart OHLC. Aggregation matching does not prove every lower-timeframe interval is present. The simulator uses an inferred OHLC path within each lower-timeframe candle; this is not TradingView broker-emulator parity.`,
    };
}

export function renderTrades(body, trades, feesKnown = null, pnlHeading = null, returnHeading = null) {
    if (!body) return;
    if (typeof feesKnown === 'boolean') {
        if (pnlHeading) pnlHeading.textContent = `${feesKnown ? 'Net' : 'Gross'} P&L`;
        if (returnHeading) returnHeading.textContent = `${feesKnown ? 'Net' : 'Gross'} return`;
    }
    if (!trades.length) {
        const row = document.createElement('tr');
        const empty = document.createElement('td');
        empty.colSpan = 12;
        empty.className = 'px-2 py-3 text-center text-gray-600';
        empty.textContent = 'No closed trades in this historical sample.';
        row.append(empty);
        body.replaceChildren(row);
        return;
    }
    const fragment = document.createDocumentFragment();
    for (const [index, trade] of trades.entries()) {
        const row = document.createElement('tr');
        row.className = 'border-t border-gray-800 text-gray-300';
        const values = tradeDisplayValues(trade, Boolean(feesKnown), index + 1);
        for (const value of values) {
            const cell = document.createElement('td');
            cell.className = 'whitespace-nowrap px-2 py-1.5 font-mono';
            cell.textContent = value;
            row.append(cell);
        }
        fragment.append(row);
    }
    body.replaceChildren(fragment);
}

export function tradeDisplayValues(trade, feesKnown = false, index = 1) {
    return [
        `${index} · ${trade?.direction || '—'} · ${trade?.entry_id || 'entry'}`,
        displayCandleTimeUtc(trade?.entry_time), displayNumber(trade?.entry_price),
        displayCandleTimeUtc(trade?.exit_time), displayNumber(trade?.exit_price),
        displayNumber(trade?.quantity),
        displayNumber(feesKnown ? trade?.net_pnl : trade?.gross_pnl),
        displayPercent(feesKnown ? trade?.net_return_pct : trade?.gross_return_pct),
        displayNumber(trade?.max_runup), displayNumber(trade?.max_drawdown),
        displayPercent(trade?.max_runup_percent), displayPercent(trade?.max_drawdown_percent),
    ];
}

