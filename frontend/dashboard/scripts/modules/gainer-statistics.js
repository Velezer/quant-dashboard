export const GAINER_PROTOCOL = 'tokocrypto_signed_equal_weight_next_4h_rank_v2';

// Approximate Newey–West interval for a time-ordered cohort mean. Four lags
// on the fixed 4-day grid cover 16 days; small samples remain unavailable.
export function meanInterval(values) {
    if (!values.length) return { count: 0, mean: null, lower: null, upper: null };
    const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
    if (values.length < 30) return { count: values.length, mean, lower: null, upper: null };
    const centered = values.map(value => value - mean);
    let variance = centered.reduce((sum, value) => sum + value * value, 0) / values.length;
    const lags = Math.min(4, values.length - 1);
    for (let lag = 1; lag <= lags; lag++) {
        let covariance = 0;
        for (let index = lag; index < values.length; index++) covariance += centered[index] * centered[index - lag];
        variance += 2 * (1 - lag / (lags + 1)) * covariance / values.length;
    }
    const radius = 1.96 * Math.sqrt(Math.max(0, variance) / values.length);
    return { count: values.length, mean, lower: mean - radius, upper: mean + radius };
}

function ranks(values) {
    const sorted = values.map((value, index) => ({ value, index })).sort((a, b) => a.value - b.value);
    const result = Array(values.length);
    for (let start = 0; start < sorted.length;) {
        let end = start + 1;
        while (end < sorted.length && sorted[end].value === sorted[start].value) end++;
        for (let index = start; index < end; index++) result[sorted[index].index] = (start + end - 1) / 2;
        start = end;
    }
    return result;
}

export function rankCorrelation(rows) {
    if (rows.length < 3) return null;
    const a = ranks(rows.map(row => row.score)), b = ranks(rows.map(row => row.gross_return));
    const mean = (rows.length - 1) / 2;
    let covariance = 0, varianceA = 0, varianceB = 0;
    for (let index = 0; index < rows.length; index++) {
        const x = a[index] - mean, y = b[index] - mean;
        covariance += x * y; varianceA += x * x; varianceB += y * y;
    }
    return varianceA > 0 && varianceB > 0 ? covariance / Math.sqrt(varianceA * varianceB) : null;
}

export function summarizeCohorts(cohorts, universeSize) {
    const dates = [...cohorts.keys()].sort((a, b) => a - b);
    const validationStart = dates[Math.floor(dates.length * 0.6)];
    const holdoutStart = dates[Math.floor(dates.length * 0.8)];
    const windows = { training: [], validation: [], holdout: [] };
    let excluded = 0, purged = 0;
    for (const time of dates) {
        const window = time < validationStart ? 'training' : time < holdoutStart ? 'validation' : 'holdout';
        const boundary = window === 'training' ? validationStart : window === 'validation' ? holdoutStart : Infinity;
        // Signals and exits must belong to the same chronological partition.
        const start = window === 'validation' ? validationStart : window === 'holdout' ? holdoutStart : -Infinity;
        const rows = cohorts.get(time).filter(row => row.exit_time < boundary && row.signal_time >= start);
        purged += cohorts.get(time).length - rows.length;
        if (rows.length < Math.max(10, Math.ceil(universeSize * 0.8))) { excluded++; continue; }
        const byScore = [...rows].sort((a, b) => b.score - a.score || a.symbol.localeCompare(b.symbol)).filter(row => row.score > 0);
        if (!byScore.length) { excluded++; continue; }
        const byMomentum = [...rows].sort((a, b) => b.momentum - a.momentum || a.symbol.localeCompare(b.symbol));
        const byOutcome = [...rows].sort((a, b) => b.gross_return - a.gross_return || a.symbol.localeCompare(b.symbol));
        const winners = new Set(byOutcome.slice(0, Math.max(1, Math.ceil(rows.length / 10))).map(row => row.symbol));
        const mean = (list, key) => list.reduce((sum, row) => sum + row[key], 0) / list.length;
        const top = byScore.slice(0, 5);
        const top5 = mean(top, 'net_return'), universe = mean(rows, 'net_return'), momentum = mean(byMomentum.slice(0, 5), 'net_return');
        windows[window].push({ time, top1: byScore[0].net_return, top5, grossTop5: mean(top, 'gross_return') * 100,
            universe, momentum, excessUniverse: top5 - universe, excessMomentum: top5 - momentum,
            positiveFrequency: byScore[0].net_return > 0 ? 100 : 0,
            decilePrecision: top.filter(row => winners.has(row.symbol)).length / top.length * 100,
            rankCorrelation: rankCorrelation(rows) });
    }
    const metrics = {}, splits = {};
    for (const [name, rows] of Object.entries(windows)) {
        splits[name] = { count: rows.length, first: rows[0]?.time ?? null, last: rows.at(-1)?.time ?? null };
        metrics[name] = {};
        for (const key of ['top1', 'top5', 'grossTop5', 'universe', 'momentum', 'excessUniverse', 'excessMomentum', 'positiveFrequency', 'decilePrecision', 'rankCorrelation']) {
            metrics[name][key] = meanInterval(rows.map(row => row[key]).filter(Number.isFinite));
        }
    }
    return { metrics, splits, excluded, purged };
}
