export function rustIndicatorCacheForSeries(cache, timeframe, seriesKey) {
    const entry = cache?.[String(timeframe || '').toLowerCase()];
    return entry?.seriesKey === seriesKey ? entry : null;
}

export function clearRustIndicatorCache(signal) {
    if (signal) signal._rustIndicatorCache = {};
    return signal?._rustIndicatorCache || {};
}
