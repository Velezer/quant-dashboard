const RANGE_DAYS = Object.freeze({ '6m': 183, '1y': 365, '5y': 1_826, '10y': 3_653 });

export function observationsForRange(observations, range = '1y') {
    if (!Array.isArray(observations) || !observations.length) return [];
    const days = RANGE_DAYS[range] || RANGE_DAYS['1y'];
    const latestTime = Date.parse(`${observations.at(-1)?.date || ''}T00:00:00Z`);
    if (!Number.isFinite(latestTime)) return [];
    const cutoff = latestTime - days * 86_400_000;
    return observations.filter(row => {
        if (!/^\d{4}-\d{2}-\d{2}$/.test(row?.date || '') || !Number.isFinite(Number(row?.value))) return false;
        const time = Date.parse(`${row.date}T00:00:00Z`);
        return Number.isFinite(time) && time >= cutoff && time <= latestTime;
    });
}

export function formatYield(value) {
    return value == null || value === '' || !Number.isFinite(Number(value))
        ? 'Unavailable'
        : `${Number(value).toFixed(2)}%`;
}

export function formatBasisPoints(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    const rounded = Math.round(Number(value));
    return `${rounded > 0 ? '+' : ''}${rounded} bp`;
}

export function renderYieldChart(observations, title = '10-Year TIPS real yield') {
    const validRows = (Array.isArray(observations) ? observations : []).filter(row =>
        /^\d{4}-\d{2}-\d{2}$/.test(row?.date || '') && Number.isFinite(Number(row?.value)));
    if (!validRows.length) return '<p class="macro-chart-empty">No observations are available for this range.</p>';

    const width = 960, height = 360;
    const left = 74, right = 18, top = 20, bottom = 42;
    const plotWidth = width - left - right, plotHeight = height - top - bottom;
    const values = validRows.map(row => Number(row.value));
    const valueMin = Math.min(...values), valueMax = Math.max(...values);
    const spread = valueMax - valueMin;
    const padding = Math.max(spread * 0.08, 0.1);
    const lower = valueMin - padding, upper = valueMax + padding;
    const yAt = value => top + ((upper - value) / (upper - lower)) * plotHeight;
    const xAt = index => left + (validRows.length === 1 ? plotWidth / 2 : (index / (validRows.length - 1)) * plotWidth);
    const points = validRows.map((row, index) => `${xAt(index).toFixed(2)},${yAt(Number(row.value)).toFixed(2)}`).join(' ');
    const ticks = Array.from({ length: 5 }, (_, index) => {
        const value = upper - ((upper - lower) * index) / 4;
        const y = top + (plotHeight * index) / 4;
        return `<g><line class="macro-chart-grid" x1="${left}" y1="${y.toFixed(2)}" x2="${width - right}" y2="${y.toFixed(2)}"></line><text class="macro-chart-label" x="${left - 10}" y="${(y + 4).toFixed(2)}" text-anchor="end">${value.toFixed(2)}%</text></g>`;
    }).join('');
    const startDate = validRows[0].date;
    const endDate = validRows.at(-1).date;
    const point = validRows.length === 1
        ? `<circle class="macro-chart-point" cx="${xAt(0).toFixed(2)}" cy="${yAt(values[0]).toFixed(2)}" r="4"></circle>`
        : '';
    const svgLabel = `${title}, from ${startDate} through ${endDate}. Latest value ${formatYield(values.at(-1))}.`;
    return `<svg viewBox="0 0 ${width} ${height}" role="img" aria-label="${svgLabel}" xmlns="http://www.w3.org/2000/svg"><title>${svgLabel}</title>${ticks}<polyline class="macro-chart-line" points="${points}"></polyline>${point}<text class="macro-chart-label" x="${left}" y="${height - 10}" text-anchor="start">${startDate}</text><text class="macro-chart-label" x="${width - right}" y="${height - 10}" text-anchor="end">${endDate}</text></svg>`;
}
