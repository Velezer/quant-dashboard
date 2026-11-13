const INTRADAY_CHART_TIMEFRAMES = new Set(['15m', '30m', '1h', '2h', '4h', '6h', '12h']);
const NANOSECONDS_PER_MILLISECOND = 1_000_000;
const CHART_TIMEFRAME_MILLISECONDS = new Map([
    ['15m', 15 * 60_000],
    ['30m', 30 * 60_000],
    ['1h', 60 * 60_000],
    ['2h', 2 * 60 * 60_000],
    ['4h', 4 * 60 * 60_000],
    ['6h', 6 * 60 * 60_000],
    ['12h', 12 * 60 * 60_000],
    ['1d', 24 * 60 * 60_000],
    ['1w', 7 * 24 * 60 * 60_000],
]);
const MAX_CONTINUOUS_GAP_INTERVALS = 3;

function candleTimestampMilliseconds(candle) {
    const timestamp = Number(candle?.time);
    if (!Number.isFinite(timestamp)) return null;
    if (timestamp > 1e15) return timestamp / NANOSECONDS_PER_MILLISECOND;
    if (timestamp > 1e12) return timestamp;
    if (timestamp > 1e9) return timestamp * 1000;
    return null;
}

export function isIntradayChartTimeframe(timeframe) {
    return INTRADAY_CHART_TIMEFRAMES.has(String(timeframe || '').toLowerCase());
}

/** Keep a continuous-market chart on its newest uninterrupted candle segment. */
export function latestContinuousCandleSegment(candles, timeframe) {
    const rows = Array.isArray(candles) ? candles : [];
    const intervalMilliseconds = CHART_TIMEFRAME_MILLISECONDS.get(String(timeframe || '').toLowerCase());
    if (rows.length < 2 || !intervalMilliseconds) {
        return { candles: rows, startIndex: 0, omittedBars: 0, gapMilliseconds: 0, gapFrom: null, gapTo: null };
    }

    let startIndex = 0;
    let gapMilliseconds = 0;
    for (let index = 1; index < rows.length; index += 1) {
        const previous = candleTimestampMilliseconds(rows[index - 1]);
        const current = candleTimestampMilliseconds(rows[index]);
        // Unknown or out-of-order timestamps make continuity ambiguous. Keep the
        // provider's series intact rather than silently discarding bars.
        if (previous === null || current === null || current <= previous) {
            return { candles: rows, startIndex: 0, omittedBars: 0, gapMilliseconds: 0, gapFrom: null, gapTo: null };
        }
        const distance = current - previous;
        if (distance > intervalMilliseconds * MAX_CONTINUOUS_GAP_INTERVALS) {
            startIndex = index;
            gapMilliseconds = distance;
        }
    }

    return startIndex === 0
        ? { candles: rows, startIndex: 0, omittedBars: 0, gapMilliseconds: 0, gapFrom: null, gapTo: null }
        : {
            candles: rows.slice(startIndex),
            startIndex,
            omittedBars: startIndex,
            gapMilliseconds,
            gapFrom: rows[startIndex - 1],
            gapTo: rows[startIndex],
        };
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
