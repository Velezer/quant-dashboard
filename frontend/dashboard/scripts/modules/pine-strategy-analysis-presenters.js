import { displayCandleTimeUtc } from './pine-backtest-jobs.js';
import { displayCount, displayNumber, displayPercent } from './pine-strategy-presenters.js';

export function tradeAnalysisRows(analysis) {
    if (!analysis || !analysis.all || !analysis.long || !analysis.short) return null;
    return [['All', analysis.all], ['Long', analysis.long], ['Short', analysis.short]].map(([side, row]) => [
        side,
        displayCount(row.trades),
        displayCount(row.wins),
        displayCount(row.losses),
        displayPercent(row.win_rate_pct),
        displayPercent(row.average_win_return_pct),
        displayPercent(row.average_loss_return_pct),
        displayNumber(row.largest_win_pnl),
        displayNumber(row.largest_loss_pnl),
        displayNumber(row.max_favorable_excursion?.average),
        displayNumber(row.max_favorable_excursion?.maximum),
        displayNumber(row.max_adverse_excursion?.average),
        displayNumber(row.max_adverse_excursion?.maximum),
        displayPercent(row.favorable_excursion_pct?.average),
        displayPercent(row.adverse_excursion_pct?.average),
    ]);
}

export function tradeReturnDistributionBins(distribution) {
    if (!distribution?.available || !Array.isArray(distribution.bins)) return null;
    if (!Number.isSafeInteger(distribution.sample_trades) || distribution.sample_trades < 1) return null;
    const bins = distribution.bins.map(bin => ({
        lower: Number(bin.lower_return_pct),
        upper: Number(bin.upper_return_pct),
        count: Number(bin.count),
    }));
    if (!bins.length || bins.some(bin => !Number.isFinite(bin.lower) || !Number.isFinite(bin.upper)
        || bin.lower > bin.upper || !Number.isSafeInteger(bin.count) || bin.count < 0)
        || bins.reduce((total, bin) => total + bin.count, 0) !== distribution.sample_trades) return null;
    return bins;
}

export function renderTradeReturnDistribution(target, distribution) {
    if (!target) return;
    if (distribution === null) {
        target.textContent = 'Run a backtest to view the full-sample return distribution.';
        return;
    }
    const bins = tradeReturnDistributionBins(distribution);
    if (!bins) {
        target.textContent = distribution?.reason
            ? `Return distribution unavailable: ${distribution.reason}.`
            : `No finite closed-trade returns are available for the histogram (${displayCount(distribution?.excluded_nonfinite_returns)} excluded).`;
        return;
    }
    const minimum = bins[0].lower;
    const maximum = bins.at(-1).upper;
    const maxCount = Math.max(...bins.map(bin => bin.count), 1);
    const width = 640;
    const height = 168;
    const left = 28;
    const right = 12;
    const top = 10;
    const axisY = 128;
    const plotWidth = width - left - right;
    const plotHeight = axisY - top;
    const binWidth = plotWidth / bins.length;
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    svg.setAttribute('preserveAspectRatio', 'none');
    svg.setAttribute('class', 'block h-28 w-full rounded border border-gray-800 bg-gray-950');
    svg.setAttribute('role', 'img');
    svg.setAttribute('aria-label', `Histogram of ${distribution.sample_trades} closed trade returns in ${bins.length} equal-width bins. ${bins.map(bin => `${displayPercent(bin.lower)} to ${displayPercent(bin.upper)}: ${bin.count} trades`).join('; ')}. Basis: ${distribution.basis || 'reported gross or net return'}.`);
    const baseline = document.createElementNS(svg.namespaceURI, 'line');
    baseline.setAttribute('x1', String(left)); baseline.setAttribute('x2', String(width - right));
    baseline.setAttribute('y1', String(axisY)); baseline.setAttribute('y2', String(axisY));
    baseline.setAttribute('stroke', '#4b5563'); baseline.setAttribute('stroke-width', '1');
    svg.append(baseline);
    const returnRange = maximum - minimum;
    if (minimum <= 0 && maximum >= 0) {
        const zeroX = returnRange === 0 ? left + plotWidth / 2 : left + (-minimum / returnRange) * plotWidth;
        const zero = document.createElementNS(svg.namespaceURI, 'line');
        zero.setAttribute('x1', String(zeroX)); zero.setAttribute('x2', String(zeroX));
        zero.setAttribute('y1', String(top)); zero.setAttribute('y2', String(axisY));
        zero.setAttribute('stroke', '#9ca3af'); zero.setAttribute('stroke-dasharray', '3 3');
        svg.append(zero);
    }
    for (const [index, bin] of bins.entries()) {
        const barHeight = bin.count / maxCount * plotHeight;
        const midpoint = bin.lower / 2 + bin.upper / 2;
        const rect = document.createElementNS(svg.namespaceURI, 'rect');
        rect.setAttribute('x', String(left + index * binWidth + binWidth * 0.12));
        rect.setAttribute('y', String(axisY - barHeight));
        rect.setAttribute('width', String(Math.max(1, binWidth * 0.76)));
        rect.setAttribute('height', String(Math.max(0, barHeight)));
        rect.setAttribute('fill', midpoint < 0 ? '#fb7185' : midpoint > 0 ? '#34d399' : '#22d3ee');
        rect.setAttribute('fill-opacity', '0.8');
        const title = document.createElementNS(svg.namespaceURI, 'title');
        title.textContent = `${displayPercent(bin.lower)} to ${displayPercent(bin.upper)}: ${bin.count} trades`;
        rect.append(title);
        svg.append(rect);
        const count = document.createElementNS(svg.namespaceURI, 'text');
        count.setAttribute('x', String(left + index * binWidth + binWidth / 2));
        count.setAttribute('y', String(Math.max(top + 9, axisY - barHeight - 3)));
        count.setAttribute('fill', '#d1d5db'); count.setAttribute('font-size', '9'); count.setAttribute('text-anchor', 'middle');
        count.textContent = String(bin.count);
        svg.append(count);
    }
    const caption = document.createElement('p');
    caption.className = 'mt-1 text-[9px] text-gray-400';
    const excluded = Number(distribution.excluded_nonfinite_returns) || 0;
    caption.textContent = `${distribution.sample_trades} full-sample closed trades · ${bins.length} equal-width bins · ${displayPercent(minimum)} to ${displayPercent(maximum)} · ${distribution.basis || 'reported gross or net basis'}${excluded ? ` · ${excluded} nonfinite returns excluded` : ''}`;
    target.replaceChildren(svg, caption);
}

export function renderTradeAnalysis(target, analysis) {
    if (!target) return;
    if (analysis === null) {
        target.textContent = 'Run a backtest to view full-sample trade statistics.';
        return;
    }
    const rows = tradeAnalysisRows(analysis);
    if (!rows) {
        target.textContent = 'Full-sample trade breakdown is unavailable in this response.';
        return;
    }
    const table = document.createElement('table');
    table.className = 'w-full min-w-[680px] border-collapse text-left text-[9px]';
    const caption = document.createElement('caption');
    caption.className = 'sr-only';
    caption.textContent = 'Full-sample closed-trade statistics for all trades, long trades, and short trades';
    table.append(caption);
    const headers = ['Side', 'Trades', 'Wins', 'Losses', 'Win rate', 'Avg win return', 'Avg loss return', 'Largest win', 'Largest loss', 'Avg MFE', 'Max MFE', 'Avg MAE', 'Max MAE', 'Avg favorable %', 'Avg adverse %'];
    const header = document.createElement('tr');
    for (const title of headers) {
        const cell = document.createElement('th');
        cell.className = 'px-2 py-1.5 text-gray-500';
        cell.textContent = title;
        header.append(cell);
    }
    const thead = document.createElement('thead');
    thead.append(header);
    const tbody = document.createElement('tbody');
    for (const values of rows) {
        const tr = document.createElement('tr');
        tr.className = 'border-t border-gray-800 text-gray-300';
        for (const [index, value] of values.entries()) {
            const cell = document.createElement(index === 0 ? 'th' : 'td');
            if (index === 0) cell.scope = 'row';
            cell.className = 'whitespace-nowrap px-2 py-1.5 font-mono';
            cell.textContent = value;
            tr.append(cell);
        }
        tbody.append(tr);
    }
    table.append(thead, tbody);
    const basis = document.createElement('p');
    basis.className = 'px-2 py-1.5 text-[9px] text-gray-500';
    basis.textContent = `Full-sample returns and P&L use ${analysis.pnl_basis || 'the reported cost basis'}; monetary values are simulator price and quantity units.`;
    target.replaceChildren(basis, table);
}

export function summarizeEquityCurve(target, points) {
    if (!target) return;
    const finitePoints = points.filter(point => Number.isFinite(Number(point.equity)));
    if (!finitePoints.length) {
        target.textContent = 'No equity points were returned for this sample.';
        return;
    }
    const values = finitePoints.map(point => Number(point.equity));
    const benchmarkValues = finitePoints.map(point => Number(point.buy_and_hold_equity));
    const hasBenchmark = benchmarkValues.every(Number.isFinite);
    const first = values[0];
    const last = values.at(-1);
    const low = Math.min(...values, ...(hasBenchmark ? benchmarkValues : []));
    const high = Math.max(...values, ...(hasBenchmark ? benchmarkValues : []));
    const width = 640;
    const height = 140;
    const padding = 8;
    const valueRange = high - low || 1;
    const toPointsAttribute = series => series.map((value, index) => {
        const x = padding + (values.length <= 1 ? 0 : index / (values.length - 1) * (width - padding * 2));
        const y = height - padding - (value - low) / valueRange * (height - padding * 2);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
    }).join(' ');
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    svg.setAttribute('preserveAspectRatio', 'none');
    svg.setAttribute('class', 'block h-28 w-full rounded border border-gray-800 bg-gray-950');
    svg.setAttribute('role', 'img');
    svg.setAttribute('aria-label', `Strategy equity across ${finitePoints.length} candles; first ${displayNumber(first)}, latest ${displayNumber(last)}${hasBenchmark ? `; buy-and-hold reference latest ${displayNumber(benchmarkValues.at(-1))}` : ''}; plotted range ${displayNumber(low)} to ${displayNumber(high)}.`);
    const baseline = document.createElementNS(svg.namespaceURI, 'line');
    baseline.setAttribute('x1', '0'); baseline.setAttribute('x2', String(width));
    baseline.setAttribute('y1', String(height - padding)); baseline.setAttribute('y2', String(height - padding));
    baseline.setAttribute('stroke', '#374151'); baseline.setAttribute('stroke-width', '1');
    const curve = document.createElementNS(svg.namespaceURI, 'polyline');
    curve.setAttribute('points', toPointsAttribute(values)); curve.setAttribute('fill', 'none');
    curve.setAttribute('stroke', '#22d3ee'); curve.setAttribute('stroke-width', '2');
    curve.setAttribute('vector-effect', 'non-scaling-stroke');
    svg.append(baseline, curve);
    if (hasBenchmark) {
        const benchmark = document.createElementNS(svg.namespaceURI, 'polyline');
        benchmark.setAttribute('points', toPointsAttribute(benchmarkValues)); benchmark.setAttribute('fill', 'none');
        benchmark.setAttribute('stroke', '#a78bfa'); benchmark.setAttribute('stroke-width', '1.5');
        benchmark.setAttribute('stroke-dasharray', '5 4'); benchmark.setAttribute('vector-effect', 'non-scaling-stroke');
        svg.append(benchmark);
    }
    const legend = document.createElement('p');
    legend.className = 'mt-1 text-[9px] text-gray-400';
    legend.textContent = hasBenchmark
        ? 'Strategy equity · cyan  |  Buy and hold · dashed violet (same initial capital, raw price change; no costs)'
        : 'Strategy equity · cyan';
    const caption = document.createElement('p');
    caption.className = 'mt-1 text-[9px] text-gray-400';
    caption.textContent = `${finitePoints.length} candles · ${displayCandleTimeUtc(finitePoints[0].time)} to ${displayCandleTimeUtc(finitePoints.at(-1).time)} · strategy first ${displayNumber(first)} · latest ${displayNumber(last)} · plotted range ${displayNumber(low)}–${displayNumber(high)}`;
    target.replaceChildren(svg, legend, caption);
}

export function monthlyBacktestRows(points, initialCapital) {
    if (!Array.isArray(points) || !points.length
        || !Number.isFinite(Number(initialCapital)) || Number(initialCapital) <= 0) return null;
    const rows = [];
    let previousEquity = Number(initialCapital);
    let previousBenchmark = Number(initialCapital);
    let previousTime = -Infinity;
    for (const point of points) {
        const time = Number(point?.time);
        const equity = Number(point?.equity);
        if (!Number.isInteger(time) || time <= previousTime || !Number.isFinite(equity)) return null;
        const date = new Date(time / 1_000_000);
        if (!Number.isFinite(date.getTime())) return null;
        const month = date.toISOString().slice(0, 7);
        let row = rows.at(-1);
        if (!row || row.month !== month) {
            const benchmark = Number(point.buy_and_hold_equity);
            row = {
                month,
                start_equity: previousEquity,
                start_benchmark: previousBenchmark,
                end_equity: equity,
                net_pnl: equity - previousEquity,
                return_pct: previousEquity > 0 ? (equity / previousEquity - 1) * 100 : null,
                buy_and_hold_return_pct: Number.isFinite(benchmark) && previousBenchmark > 0
                    ? (benchmark / previousBenchmark - 1) * 100
                    : null,
            };
            rows.push(row);
        } else {
            row.end_equity = equity;
            row.net_pnl = equity - row.start_equity;
            row.return_pct = row.start_equity > 0 ? (equity / row.start_equity - 1) * 100 : null;
            const benchmark = Number(point.buy_and_hold_equity);
            row.buy_and_hold_return_pct = Number.isFinite(benchmark) && row.start_benchmark > 0
                ? (benchmark / row.start_benchmark - 1) * 100
                : null;
        }
        previousEquity = equity;
        const currentBenchmark = Number(point.buy_and_hold_equity);
        if (Number.isFinite(currentBenchmark) && currentBenchmark > 0) previousBenchmark = currentBenchmark;
        previousTime = time;
    }
    return rows;
}

export function renderMonthlyPerformance(target, points, initialCapital) {
    if (!target) return;
    const rows = monthlyBacktestRows(points, initialCapital);
    if (!rows) {
        target.textContent = 'Monthly performance is unavailable because the backtest equity sample is missing or invalid.';
        return;
    }
    if (!rows.length) {
        target.textContent = 'No monthly observations were returned for this sample.';
        return;
    }
    const table = document.createElement('table');
    table.className = 'w-full min-w-[560px] border-collapse text-left text-[9px]';
    const caption = document.createElement('caption');
    caption.className = 'sr-only';
    caption.textContent = 'UTC monthly marked strategy equity changes and same-period raw underlying price returns';
    table.append(caption);
    const headers = ['UTC month', 'Marked equity change', 'Marked return', 'Underlying return · no costs'];
    const header = document.createElement('tr');
    for (const title of headers) {
        const cell = document.createElement('th');
        cell.className = 'px-2 py-1.5 text-gray-500';
        cell.textContent = title;
        header.append(cell);
    }
    const thead = document.createElement('thead');
    thead.append(header);
    const tbody = document.createElement('tbody');
    for (const row of rows) {
        const tr = document.createElement('tr');
        tr.className = 'border-t border-gray-800 text-gray-300';
        const values = [row.month, displayNumber(row.net_pnl), displayPercent(row.return_pct), displayPercent(row.buy_and_hold_return_pct)];
        for (const [index, value] of values.entries()) {
            const cell = document.createElement(index === 0 ? 'th' : 'td');
            if (index === 0) cell.scope = 'row';
            cell.className = 'whitespace-nowrap px-2 py-1.5 font-mono';
            cell.textContent = value;
            tr.append(cell);
        }
        tbody.append(tr);
    }
    table.append(thead, tbody);
    target.replaceChildren(table);
}

export function chronologicalValidationRows(validation) {
    if (!validation?.available || !validation.training || !validation.validation) return null;
    const rows = [
        {
            period: 'Training',
            start: validation.training_start_time,
            end: validation.training_end_time,
            bars: validation.training_bars,
            ...validation.training,
        },
        {
            period: 'Validation tail',
            start: validation.validation_start_time,
            end: validation.validation_end_time,
            bars: validation.validation_bars,
            ...validation.validation,
        },
    ].map(row => ({
        period: row.period,
        dates: `${displayCandleTimeUtc(row.start)}–${displayCandleTimeUtc(row.end)}`,
        bars: displayCount(row.bars),
        closedTrades: displayCount(row.closed_trades),
        winRate: displayPercent(row.win_rate_pct),
        winRateInterval: Array.isArray(row.win_rate_interval_95_pct) && row.win_rate_interval_95_pct.length === 2
            ? `${displayPercent(row.win_rate_interval_95_pct[0])}–${displayPercent(row.win_rate_interval_95_pct[1])}`
            : '—',
        closedPnl: displayNumber(row.closed_trade_pnl),
        markedReturn: displayPercent(row.marked_return_pct),
        profitFactor: row.profit_factor_unbounded ? '∞' : displayNumber(row.profit_factor),
        closeDrawdown: row.close_drawdown?.available ? displayPercent(row.close_drawdown.max_drawdown_pct) : '—',
        monthlyStability: '—',
    }));
    const trainingMonthly = Array.isArray(validation.training.monthly_marked_returns)
        ? validation.training.monthly_marked_returns : [];
    const validationMonthly = Array.isArray(validation.validation.monthly_marked_returns)
        ? validation.validation.monthly_marked_returns : [];
    const uniqueMonths = new Set([...trainingMonthly, ...validationMonthly].map(row => row?.month).filter(Boolean));
    const trainingTailMonths = trainingMonthly.slice(-3);
    const validationStartMonth = validationMonthly[0]?.month;
    const validationMonthlyOnly = validationMonthly.filter(row => row?.month !== validationStartMonth);
    const validationMonths = validationMonthlyOnly.length ? validationMonthlyOnly : validationMonthly;
    const finiteReturns = rows => rows.map(row => Number(row?.return_pct)).filter(Number.isFinite);
    const means = values => values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
    const trainingReturns = finiteReturns(trainingTailMonths);
    const tailReturns = finiteReturns(validationMonths);
    const trainMean = means(trainingReturns);
    const validationMean = means(tailReturns);
    const sampleNote = `${trainingReturns.length} training months · ${tailReturns.length} validation months`;
    const months = uniqueMonths.size;
    const drift = Number.isFinite(trainMean) && Number.isFinite(validationMean)
        ? `mean monthly marked return ${displayPercent(trainMean)} → ${displayPercent(validationMean)} (${displayPercent(validationMean - trainMean)} change)`
        : 'insufficient complete monthly observations to compare mean returns';
    return [...rows, {
        period: 'Tail vs recent training',
        dates: `${months} distinct UTC months`,
        bars: '—',
        closedTrades: '—',
        winRate: '—',
        closedPnl: '—',
        markedReturn: sampleNote,
        profitFactor: '—',
        closeDrawdown: '—',
        monthlyStability: drift,
    }];
}

export function renderChronologicalValidation(target, validation, equityCurve = [], initialCapital = null) {
    if (!target) return;
    if (validation === null) {
        target.textContent = 'Run a backtest to view its chronological training and validation tail breakdown.';
        return;
    }
    const rows = chronologicalValidationRows(validation);
    if (!rows) {
        target.textContent = validation?.reason
            ? `Chronological tail check unavailable: ${validation.reason}`
            : 'Chronological tail check is unavailable in this response.';
        return;
    }
    const table = document.createElement('table');
    table.className = 'w-full min-w-[860px] border-collapse text-left text-[9px]';
    const caption = document.createElement('caption');
    caption.className = 'sr-only';
    caption.textContent = 'Chronological training and validation tail backtest comparison';
    table.append(caption);
    const headers = ['Period', 'UTC sample', 'Bars', 'Closed trades', 'Win rate', '95% Wilson interval', 'Closed P&L', 'Marked return', 'Profit factor', 'Close drawdown', 'Monthly stability'];
    const thead = document.createElement('thead');
    const header = document.createElement('tr');
    for (const title of headers) {
        const cell = document.createElement('th');
        cell.className = 'px-2 py-1.5 text-gray-500';
        cell.textContent = title;
        header.append(cell);
    }
    thead.append(header);
    const tbody = document.createElement('tbody');
    for (const row of rows) {
        const tr = document.createElement('tr');
        tr.className = 'border-t border-gray-800 text-gray-300';
        for (const [index, value] of [row.period, row.dates, row.bars, row.closedTrades, row.winRate, row.winRateInterval, row.closedPnl, row.markedReturn, row.profitFactor, row.closeDrawdown, row.monthlyStability].entries()) {
            const cell = document.createElement(index === 0 ? 'th' : 'td');
            if (index === 0) cell.scope = 'row';
            cell.className = 'whitespace-nowrap px-2 py-1.5 font-mono';
            cell.textContent = value;
            tr.append(cell);
        }
        tbody.append(tr);
    }
    table.append(thead, tbody);
    const notes = document.createElement('p');
    notes.className = 'mt-1 text-[9px] leading-relaxed text-gray-500';
    notes.textContent = `Win-rate intervals use a 95% Wilson calculation and assume independent closed trades; serial dependence can make them too narrow. They are descriptive uncertainty, not predictive confidence. ${validation.training?.cross_boundary_trades_excluded || 0} training trades crossed the split and were excluded from training closed-trade metrics. The final row compares up to the last three training months with validation months; a validation boundary month may contain both periods and is excluded when fully separable months exist. ${validation.warning || ''}`;
    target.replaceChildren(table, notes);
}

export function renderDrawdownCurve(target, analysis) {
    if (!target) return;
    if (analysis === null) {
        target.textContent = 'Run a backtest to view candle-close drawdown periods.';
        return;
    }
    if (!analysis?.available || !Array.isArray(analysis.series) || !analysis.series.length) {
        target.textContent = 'Candle-close drawdown series is unavailable in this response.';
        return;
    }
    const values = analysis.series.map(point => Number(point.drawdown_pct));
    if (values.some(value => !Number.isFinite(value) || value < 0)) {
        target.textContent = 'Candle-close drawdown series contains invalid values.';
        return;
    }
    const width = 640;
    const height = 112;
    const padding = 8;
    const maximum = Math.max(...values, 0.01);
    const points = values.map((value, index) => {
        const x = padding + (values.length <= 1 ? 0 : index / (values.length - 1) * (width - padding * 2));
        const y = padding + value / maximum * (height - padding * 2);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
    }).join(' ');
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    svg.setAttribute('preserveAspectRatio', 'none');
    svg.setAttribute('class', 'block h-20 w-full rounded border border-gray-800 bg-gray-950');
    svg.setAttribute('role', 'img');
    svg.setAttribute('aria-label', `Close-to-close drawdown across ${values.length} candles; maximum ${displayPercent(analysis.max_drawdown_pct)} over ${displayCount(analysis.duration_bars)} bars from peak to trough; current ${displayPercent(analysis.current_drawdown_pct)} for ${displayCount(analysis.current_duration_bars)} bars.`);
    const baseline = document.createElementNS(svg.namespaceURI, 'line');
    baseline.setAttribute('x1', '0'); baseline.setAttribute('x2', String(width));
    baseline.setAttribute('y1', String(padding)); baseline.setAttribute('y2', String(padding));
    baseline.setAttribute('stroke', '#374151'); baseline.setAttribute('stroke-width', '1');
    const curve = document.createElementNS(svg.namespaceURI, 'polyline');
    curve.setAttribute('points', points); curve.setAttribute('fill', 'none');
    curve.setAttribute('stroke', '#fb7185'); curve.setAttribute('stroke-width', '1.75');
    curve.setAttribute('vector-effect', 'non-scaling-stroke');
    svg.append(baseline, curve);
    const caption = document.createElement('p');
    caption.className = 'mt-1 text-[9px] text-gray-400';
    const recovery = analysis.recovery_time == null
        ? 'not recovered by sample end'
        : `recovered ${displayCandleTimeUtc(analysis.recovery_time)} after ${displayCount(analysis.recovery_bars)} bars`;
    const averageDuration = analysis.average_recovered_drawdown_duration_bars == null
        ? 'no recovered drawdown periods to average'
        : `average recovered period ${displayNumber(analysis.average_recovered_drawdown_duration_bars)} bars`;
    caption.textContent = `Close-to-close maximum ${displayPercent(analysis.max_drawdown_pct)} · peak ${displayCandleTimeUtc(analysis.peak_time)} · trough ${displayCandleTimeUtc(analysis.trough_time)} · ${displayCount(analysis.duration_bars)} bars peak-to-trough · ${displayCount(analysis.max_period_duration_bars)} bars peak-to-recovery/sample end · ${recovery} · ${displayCount(analysis.completed_drawdown_periods)} recovered periods · ${averageDuration}`;
    target.replaceChildren(svg, caption);
}

