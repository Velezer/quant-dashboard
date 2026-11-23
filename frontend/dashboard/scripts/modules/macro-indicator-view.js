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

export function formatCurrency(value) {
    return value == null || value === '' || !Number.isFinite(Number(value))
        ? 'Unavailable'
        : `IDR ${Number(value).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} / USD`;
}

export function formatIndex(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    const index = Number(value);
    return `${Number.isInteger(index) ? index : index.toFixed(1)} / 100`;
}

export function formatIndexPoints(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    return `${Number(value).toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 2 })} points`;
}

export function formatUsdPerBarrel(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    return `$${Number(value).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} / bbl`;
}

export function formatMillionBarrels(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    return `${Number(value).toLocaleString(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} million bbl`;
}

export function formatUsdFlow(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    const amount = Number(value);
    const absolute = Math.abs(amount);
    const scaled = absolute >= 1_000_000_000
        ? `${(absolute / 1_000_000_000).toLocaleString(undefined, { maximumFractionDigits: 2 })}B`
        : `${(absolute / 1_000_000).toLocaleString(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}M`;
    const sign = amount > 0 ? '+' : amount < 0 ? '−' : '';
    return `${sign}$${scaled}`;
}

export function formatValue(value, valueFormat = 'percent') {
    if (valueFormat === 'idr_per_usd') return formatCurrency(value);
    if (valueFormat === 'usd_flow') return formatUsdFlow(value);
    if (valueFormat === 'index_0_100') return formatIndex(value);
    if (valueFormat === 'index_points') return formatIndexPoints(value);
    if (valueFormat === 'usd_per_barrel') return formatUsdPerBarrel(value);
    if (valueFormat === 'million_barrels') return formatMillionBarrels(value);
    return formatYield(value);
}

export function formatBasisPoints(value) {
    if (value == null || value === '' || !Number.isFinite(Number(value))) return 'Unavailable';
    const rounded = Math.round(Number(value));
    return `${rounded > 0 ? '+' : ''}${rounded} bp`;
}

export function formatObservationChange(change, latestValue, valueFormat = 'percent', period = '') {
    if (change == null || !Number.isFinite(Number(change))) return 'Unavailable';
    if (valueFormat === 'index_0_100') {
        const amount = Number(change);
        const signed = amount > 0 ? '+' : amount < 0 ? '−' : '';
        const formatted = `${signed}${Math.abs(amount).toFixed(Number.isInteger(amount) ? 0 : 1)} points`;
        return period ? `${formatted} ${period}` : formatted;
    }
    if (valueFormat === 'usd_flow') {
        const formatted = formatUsdFlow(Number(change));
        return period ? `${formatted} ${period}` : formatted;
    }
    if (valueFormat === 'index_points') {
        const amount = Number(change);
        const signed = amount > 0 ? '+' : amount < 0 ? '−' : '';
        const formatted = `${signed}${Math.abs(amount).toFixed(Number.isInteger(amount) ? 0 : 2)} points`;
        return period ? `${formatted} ${period}` : formatted;
    }
    if (valueFormat === 'usd_per_barrel') {
        const amount = Number(change);
        const signed = amount > 0 ? '+' : amount < 0 ? '−' : '';
        const formatted = `${signed}$${Math.abs(amount).toFixed(2)} / bbl`;
        return period ? `${formatted} ${period}` : formatted;
    }
    if (valueFormat === 'million_barrels') {
        const amount = Number(change);
        const signed = amount > 0 ? '+' : amount < 0 ? '−' : '';
        const formatted = `${signed}${Math.abs(amount).toFixed(1)} million bbl`;
        return period ? `${formatted} ${period}` : formatted;
    }
    if (valueFormat !== 'idr_per_usd') {
        const formatted = formatBasisPoints(Number(change) * 100);
        return period ? `${formatted} ${period}` : formatted;
    }

    const previousValue = Number(latestValue) - Number(change);
    const amount = Number(change);
    const absolute = Math.abs(amount).toLocaleString(undefined, {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2,
    });
    const signed = amount > 0 ? '+' : amount < 0 ? '−' : '';
    const percent = previousValue > 0 ? ` (${signed}${(Math.abs(amount) / previousValue * 100).toFixed(2)}%)` : '';
    return `${signed}${absolute} IDR/USD${percent}${period ? ` ${period}` : ''}`;
}

function axisValue(value, valueFormat) {
    if (valueFormat === 'idr_per_usd') {
        return Number(value).toLocaleString(undefined, { maximumFractionDigits: 0 });
    }
    if (valueFormat === 'usd_flow') return formatUsdFlow(value);
    if (valueFormat === 'index_0_100') return `${Math.round(Number(value))}`;
    if (valueFormat === 'index_points') return Number(value).toLocaleString(undefined, { maximumFractionDigits: 2 });
    if (valueFormat === 'usd_per_barrel') return `$${Number(value).toFixed(2)}`;
    if (valueFormat === 'million_barrels') return Number(value).toFixed(1);
    return `${Number(value).toFixed(2)}%`;
}

function escapeSvg(value) {
    return String(value).replace(/[&<>"']/g, character => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;',
    })[character]);
}

export function renderMacroChart(observations, title, valueFormat = 'percent', referenceLine = null) {
    const validRows = (Array.isArray(observations) ? observations : []).filter(row =>
        /^\d{4}-\d{2}-\d{2}$/.test(row?.date || '') && Number.isFinite(Number(row?.value)));
    if (!validRows.length) return '<p class="macro-chart-empty">No observations are available for this range.</p>';

    const width = 960, height = 360;
    const left = 90, right = 18, top = 20, bottom = 42;
    const plotWidth = width - left - right, plotHeight = height - top - bottom;
    const values = validRows.map(row => Number(row.value));
    const valueMin = Math.min(...values), valueMax = Math.max(...values);
    const referenceValue = referenceLine != null && referenceLine !== '' && Number.isFinite(Number(referenceLine))
        ? Number(referenceLine)
        : null;
    const spread = valueMax - valueMin;
    const padding = Math.max(
        spread * 0.08,
        valueFormat === 'idr_per_usd' ? 1 : valueFormat === 'usd_flow' ? 1_000_000 : 0.1,
    );
    const lower = valueFormat === 'index_0_100'
        ? 0
        : valueFormat === 'usd_flow'
            ? Math.min(0, valueMin - padding)
            : referenceValue == null ? valueMin - padding : Math.min(valueMin - padding, referenceValue - padding);
    const upper = valueFormat === 'index_0_100'
        ? 100
        : valueFormat === 'usd_flow'
            ? Math.max(0, valueMax + padding)
            : referenceValue == null ? valueMax + padding : Math.max(valueMax + padding, referenceValue + padding);
    const yAt = value => top + ((upper - value) / (upper - lower)) * plotHeight;
    const xAt = index => left + (validRows.length === 1 ? plotWidth / 2 : (index / (validRows.length - 1)) * plotWidth);
    const points = validRows.map((row, index) => `${xAt(index).toFixed(2)},${yAt(Number(row.value)).toFixed(2)}`).join(' ');
    const ticks = Array.from({ length: 5 }, (_, index) => {
        const value = upper - ((upper - lower) * index) / 4;
        const y = top + (plotHeight * index) / 4;
        return `<g><line class="macro-chart-grid" x1="${left}" y1="${y.toFixed(2)}" x2="${width - right}" y2="${y.toFixed(2)}"></line><text class="macro-chart-label" x="${left - 10}" y="${(y + 4).toFixed(2)}" text-anchor="end">${axisValue(value, valueFormat)}</text></g>`;
    }).join('');
    const startDate = validRows[0].date;
    const endDate = validRows.at(-1).date;
    const point = validRows.length === 1
        ? `<circle class="macro-chart-point" cx="${xAt(0).toFixed(2)}" cy="${yAt(values[0]).toFixed(2)}" r="4"></circle>`
        : '';
    const zeroLine = valueFormat === 'usd_flow'
        ? `<line class="macro-chart-zero" x1="${left}" y1="${yAt(0).toFixed(2)}" x2="${width - right}" y2="${yAt(0).toFixed(2)}"></line>`
        : '';
    const referenceLineSvg = referenceValue != null
        ? `<line class="macro-chart-reference" x1="${left}" y1="${yAt(referenceValue).toFixed(2)}" x2="${width - right}" y2="${yAt(referenceValue).toFixed(2)}"></line><text class="macro-chart-reference-label" x="${width - right - 4}" y="${(yAt(referenceValue) - 6).toFixed(2)}" text-anchor="end">${axisValue(referenceValue, valueFormat)} · reference</text>`
        : '';
    const safeTitle = escapeSvg(title || 'Macroeconomic indicator');
    const referenceDescription = referenceValue == null
        ? ''
        : ` Reference line at ${axisValue(referenceValue, valueFormat)}.`;
    const latestDescription = formatValue(values.at(-1), valueFormat);
    const svgLabel = `${safeTitle}, from ${startDate} through ${endDate}.`
        + ` Latest value ${latestDescription}.${referenceDescription}`;
    const safeLabel = escapeSvg(svgLabel);
    return `<svg viewBox="0 0 ${width} ${height}" role="img" aria-label="${safeLabel}" xmlns="http://www.w3.org/2000/svg"><title>${safeLabel}</title>${ticks}${zeroLine}${referenceLineSvg}<polyline class="macro-chart-line" points="${points}"></polyline>${point}<text class="macro-chart-label" x="${left}" y="${height - 10}" text-anchor="start">${startDate}</text><text class="macro-chart-label" x="${width - right}" y="${height - 10}" text-anchor="end">${endDate}</text></svg>`;
}
