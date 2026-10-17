const LOG_SCALE_MIN_GROWTH_RANGE_DECADES = 2;
const LOG_AXIS_PADDING_DECADES = 0.08;
const USD_BALANCE_FORMATTER = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
});

export function formatUsdBalance(value) {
    return Number.isFinite(value) ? USD_BALANCE_FORMATTER.format(value) : 'Unavailable';
}

export function accountGrowthChartModel(projection) {
    if (!projection || !Array.isArray(projection.points) || projection.points.length < 2) return null;
    const { points, starting_balance_units, expected_ending_balance_units, trading_days } = projection;
    if (!Number.isInteger(trading_days) || trading_days < 1 || points.length !== trading_days + 1) return null;
    if (projection.fee_adjusted !== true && projection.fee_adjusted !== false) return null;
    if (!Number.isFinite(starting_balance_units) || starting_balance_units <= 0
        || !Number.isFinite(expected_ending_balance_units) || expected_ending_balance_units < 0) return null;
    if (!Number.isInteger(projection.trades_per_day) || projection.trades_per_day < 1
        || !Number.isFinite(projection.trade_size_pct_of_equity)
        || projection.trade_size_pct_of_equity <= 0
        || !Number.isFinite(projection.expected_balance_growth_per_trade_pct)) return null;
    if (points.some((point, index) => !point || point.trading_day !== index
        || !Number.isFinite(point.expected_balance_units) || point.expected_balance_units < 0)) return null;
    if (points[0].expected_balance_units !== starting_balance_units
        || points[points.length - 1].expected_balance_units !== expected_ending_balance_units) return null;

    const balances = points.map(point => point.expected_balance_units);
    const minBalance = Math.min(...balances);
    const maxBalance = Math.max(...balances);
    const balanceRangeDecades = minBalance > 0
        ? Math.log10(maxBalance) - Math.log10(minBalance)
        : 0;
    const scale = balanceRangeDecades >= LOG_SCALE_MIN_GROWTH_RANGE_DECADES ? 'log' : 'linear';
    const padding = Math.max((maxBalance - minBalance) * 0.12, maxBalance * 0.02, 0.01);
    const axisMinimum = Math.max(0, minBalance - padding);
    const axisMaximum = maxBalance + padding;
    if (!Number.isFinite(axisMaximum) || axisMaximum <= axisMinimum) return null;
    const logAxisMinimum = scale === 'log'
        ? Math.log10(minBalance) - LOG_AXIS_PADDING_DECADES
        : null;
    const logAxisMaximum = scale === 'log'
        ? Math.log10(maxBalance) + LOG_AXIS_PADDING_DECADES
        : null;
    return {
        points,
        minBalance,
        maxBalance,
        axisMinimum,
        axisMaximum,
        scale,
        logAxisMinimum,
        logAxisMaximum,
    };
}
