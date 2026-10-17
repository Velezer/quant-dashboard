import { isIntradayChartTimeframe } from './chart-candle-time.js';

export function stockbitExpectedSessionDate(timeframe, now = new Date()) {
    const normalizedTimeframe = String(timeframe || '').toLowerCase();
    if (!isIntradayChartTimeframe(normalizedTimeframe) && normalizedTimeframe !== '1d') return null;
    const parts = new Intl.DateTimeFormat('en-US', {
        timeZone: 'Asia/Jakarta', year: 'numeric', month: '2-digit', day: '2-digit',
        hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    }).formatToParts(now).reduce((result, part) => {
        result[part.type] = part.value;
        return result;
    }, {});
    const expected = new Date(Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day)));
    const weekday = expected.getUTCDay();
    const minuteOfDay = Number(parts.hour) * 60 + Number(parts.minute);
    const beforeExpectedBar = normalizedTimeframe === '1d'
        ? minuteOfDay < 15 * 60 + 30
        : minuteOfDay < 9 * 60 + 30;
    if (weekday === 0 || weekday === 6 || beforeExpectedBar) expected.setUTCDate(expected.getUTCDate() - 1);
    while (expected.getUTCDay() === 0 || expected.getUTCDay() === 6) expected.setUTCDate(expected.getUTCDate() - 1);
    return expected.toISOString().slice(0, 10);
}
