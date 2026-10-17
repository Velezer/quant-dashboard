/** Renders the illustrative account-growth projection in the simulator dialog. */
export function createRuleSimulatorGrowthChart({
    dialog,
    accountGrowthChartModel,
    formatBalance,
    formatAxisBalance,
    formatGrowthHorizon,
    getTradingDaysPerYear,
    setOutput,
    clearGrowthChart,
}) {
    return function renderGrowthChart(projection, rule, growthSettings) {
        const chart = dialog.querySelector('#ruleSimulatorGrowthChart');
        const plot = dialog.querySelector('#ruleSimulatorChartPlot');
        const model = accountGrowthChartModel(projection);
        if (!model) {
            clearGrowthChart('Chart unavailable for the current calculation.');
            return;
        }
        const height = 360;
        const plotLeft = 100;
        const plotRight = 694;
        const plotTop = 30;
        const plotBottom = 290;
        const yAxisMinimum = model.scale === 'log' ? model.logAxisMinimum : model.axisMinimum;
        const yAxisMaximum = model.scale === 'log' ? model.logAxisMaximum : model.axisMaximum;
        const xForDay = day => plotLeft + (day / projection.trading_days) * (plotRight - plotLeft);
        const yForBalance = balance => {
            const axisValue = model.scale === 'log' ? Math.log10(balance) : balance;
            return plotBottom
                - ((axisValue - yAxisMinimum) / (yAxisMaximum - yAxisMinimum)) * (plotBottom - plotTop);
        };
        const yTicks = Array.from({ length: 5 }, (_, index) =>
            yAxisMinimum + ((yAxisMaximum - yAxisMinimum) * index) / 4)
            .map(value => model.scale === 'log' ? 10 ** value : value);
        const xTickDays = [...new Set([
            0,
            Math.round(projection.trading_days / 3),
            Math.round((projection.trading_days * 2) / 3),
            projection.trading_days,
        ])];
        const tradingDaysPerYear = getTradingDaysPerYear();
        const xAxisUsesYears = Number.isInteger(tradingDaysPerYear)
            && tradingDaysPerYear > 0
            && projection.trading_days >= tradingDaysPerYear;
        const formatXAxisTick = day => xAxisUsesYears
            ? (day / tradingDaysPerYear).toFixed(1)
            : day.toLocaleString('en-US');
        const xAxisLabel = xAxisUsesYears
            ? `Years · ${tradingDaysPerYear.toLocaleString('en-US')} modeled days/year`
            : 'Trading day';
        const path = projection.points.map((point, index) =>
            `${index === 0 ? 'M' : 'L'} ${xForDay(point.trading_day).toFixed(2)} ${yForBalance(point.expected_balance_units).toFixed(2)}`
        ).join(' ');
        const positive = projection.expected_ending_balance_units >= projection.starting_balance_units;
        const color = !projection.fee_adjusted ? '#94a3b8' : positive ? '#34d399' : '#fb7185';
        const { minBalance, maxBalance } = model;
        dialog.querySelector('#ruleSimulatorChartAccessibleTitle').textContent = projection.fee_adjusted
            ? 'Fee-adjusted expected account growth projection'
            : 'Gross-only expected account growth projection';
        dialog.querySelector('#ruleSimulatorChartDescription').textContent =
            `Expected USD account balance, from ${formatBalance(projection.starting_balance_units)} ` +
            `at day 0 to ${formatBalance(projection.expected_ending_balance_units)} at ${formatGrowthHorizon(projection.trading_days)}. ` +
            `Assumes ${rule.win_rate_pct}% win rate, ${projection.trade_size_pct_of_equity}% of current balance per trade, ` +
            `and ${projection.trades_per_day} trades each trading day. The vertical axis shows ${formatBalance(minBalance)} ` +
            `to ${formatBalance(maxBalance)}${model.scale === 'log' ? ' and uses logarithmic scale.' : ' and is focused on this range.'}`;
        setOutput('ruleSimulatorEndingBalance', `Expected ending balance: ${formatBalance(projection.expected_ending_balance_units)} USD`);
        setOutput('ruleSimulatorChartState', projection.fee_adjusted
            ? `After selected entry and exit fees · ${formatGrowthHorizon(projection.trading_days)}`
            : `Gross-only · venue fees unavailable · ${formatGrowthHorizon(projection.trading_days)}`);
        const axisScaleNote = model.scale === 'log'
            ? 'The USD axis uses a logarithmic scale; equal distances represent equal proportional changes, not equal dollar changes.'
            : model.axisMinimum === 0
                ? 'The vertical axis includes zero.'
                : 'The vertical axis is focused on this balance range and does not start at zero.';
        setOutput('ruleSimulatorChartNote', projection.fee_adjusted
            ? `Expected balance compounds the probability-weighted outcome at ${rule.win_rate_pct}% assumed wins. Each of ${projection.trades_per_day} daily trades uses ${growthSettings.trade_size_pct_of_equity}% of the then-current balance. ${axisScaleNote}`
            : `Gross-only projection excludes fees because both fee rates are not available. Enter both fees above to see the fee-adjusted curve. ${axisScaleNote}`);

        plot.innerHTML = `
            <text x="${plotLeft}" y="18" fill="#9ca3af" font-size="11">Account balance · USD${model.scale === 'log' ? ' · log scale' : ''}</text>
            ${yTicks.map(value => {
                const y = yForBalance(value).toFixed(2);
                return `<line x1="${plotLeft}" y1="${y}" x2="${plotRight}" y2="${y}" stroke="#263244" stroke-width="1" />
                    <text x="${plotLeft - 10}" y="${Number(y) + 4}" text-anchor="end" fill="#9ca3af" font-size="11">${formatAxisBalance(value, model.scale)}</text>`;
            }).join('')}
            <line x1="${plotLeft}" y1="${plotTop}" x2="${plotLeft}" y2="${plotBottom}" stroke="#64748b" />
            <line x1="${plotLeft}" y1="${plotBottom}" x2="${plotRight}" y2="${plotBottom}" stroke="#64748b" />
            ${xTickDays.map(day => {
                const x = xForDay(day).toFixed(2);
                return `<line x1="${x}" y1="${plotBottom}" x2="${x}" y2="${plotBottom + 5}" stroke="#64748b" />
                    <text x="${x}" y="${plotBottom + 20}" text-anchor="middle" fill="#9ca3af" font-size="11">${formatXAxisTick(day)}</text>`;
            }).join('')}
            <path d="${path}" fill="none" stroke="${color}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />
            <circle cx="${xForDay(0)}" cy="${yForBalance(projection.starting_balance_units)}" r="4" fill="${color}" />
            <circle cx="${xForDay(projection.trading_days)}" cy="${yForBalance(projection.expected_ending_balance_units)}" r="5" fill="${color}" />
            <text x="${(plotLeft + plotRight) / 2}" y="${height - 12}" text-anchor="middle" fill="#9ca3af" font-size="12">${xAxisLabel}</text>`;
        chart.classList.remove('hidden');
    };
}
