const INTRADAY_CHART_TIMEFRAMES = new Set(['15m', '30m', '1h', '2h', '4h', '6h', '12h']);
const NANOSECONDS_PER_MILLISECOND = 1_000_000;

export function isIntradayChartTimeframe(timeframe) {
    return INTRADAY_CHART_TIMEFRAMES.has(String(timeframe || '').toLowerCase());
}

export function formatChartCandleTimestamp(timestampNanoseconds, timeframe, { includeWib = false } = {}) {
    const timestamp = Number(timestampNanoseconds);
    if (!Number.isFinite(timestamp)) return null;
    const date = new Date(timestamp / NANOSECONDS_PER_MILLISECOND);
    if (Number.isNaN(date.getTime())) return null;
    const intraday = isIntradayChartTimeframe(timeframe);
    const parts = new Intl.DateTimeFormat('en-US', {
        timeZone: 'Asia/Jakarta',
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        ...(intraday ? { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' } : {}),
    }).formatToParts(date).reduce((result, part) => {
        result[part.type] = part.value;
        return result;
    }, {});
    const label = intraday
        ? `${parts.month}-${parts.day} ${parts.hour}:${parts.minute}`
        : `${parts.year}-${parts.month}-${parts.day}`;
    return includeWib && intraday ? `${label} WIB` : label;
}

export function formatChartCandleLabel(candle, timeframe) {
    const storedLabel = String(candle?.date || '').trim();
    const intraday = isIntradayChartTimeframe(timeframe);
    if (storedLabel && storedLabel !== '--') {
        return intraday && !/\bWIB$/i.test(storedLabel) ? `${storedLabel} WIB` : storedLabel;
    }
    return formatChartCandleTimestamp(candle?.time, timeframe, { includeWib: true }) || 'Latest Bar';
}
