import { CHART_INDICATOR_IDS } from './chart-indicator-data.js';
import { NWE_LOWER_SOURCE_TITLE, NWE_UPPER_SOURCE_TITLE } from './pine-sources.js';

export function finiteNumber(value) {
    if (value === null || value === undefined) return null;
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : null;
}

export function pineWorkerCandle(candle) {
    return {
        open: candle.open,
        high: candle.high,
        low: candle.low,
        close: candle.close,
        volume: candle.volume,
        time: Number.isFinite(Number(candle.time)) ? Number(candle.time) : null,
    };
}

export function pineWorkerInput({
    source,
    signal,
    candles,
    timeframe,
    repainting,
    modeOverride = null,
    scriptType = 'indicator',
}) {
    const timeframes = Object.fromEntries(Object.entries(signal?.candles_by_tf || {}).map(([key, bars]) => [
        key,
        Array.isArray(bars) ? bars.map(pineWorkerCandle) : [],
    ]));
    if (!Array.isArray(timeframes[timeframe]) || timeframes[timeframe].length === 0) {
        timeframes[timeframe] = candles.map(pineWorkerCandle);
    }
    const indicatorMode = modeOverride || (repainting ? 'repainting' : 'endpoint');
    const input_sources = {
        [NWE_UPPER_SOURCE_TITLE]: candles.map(candle => finiteNumber(
            candle?.indicators?.[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]?.[indicatorMode]?.upper,
        )),
        [NWE_LOWER_SOURCE_TITLE]: candles.map(candle => finiteNumber(
            candle?.indicators?.[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]?.[indicatorMode]?.lower,
        )),
    };
    return {
        source,
        candles: candles.map(pineWorkerCandle),
        timeframe,
        timeframes,
        input_sources,
        mode: scriptType,
    };
}
