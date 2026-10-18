/**
 * Estimates per-bar volume delta from chart OHLCV. A chart candle stands in for
 * all of its lower-timeframe bars, so this is a coarse directional estimate.
 */
export function estimateVolumeDeltaBars(candles) {
    let previousClose = null;
    return (Array.isArray(candles) ? candles : []).map(candle => {
        const volume = Number(candle?.volume);
        const open = Number(candle?.open);
        const close = Number(candle?.close);
        const hasVolume = Number.isFinite(volume) && volume >= 0;
        const hasPrice = Number.isFinite(open) && Number.isFinite(close);
        const delta = hasVolume && hasPrice
            ? (close > (Number.isFinite(previousClose) ? previousClose : open) ? volume
                : close < (Number.isFinite(previousClose) ? previousClose : open) ? -volume
                    : close >= open ? volume : -volume)
            : null;
        previousClose = Number.isFinite(close) ? close : previousClose;
        return {
            open: Number.isFinite(delta) ? 0 : null,
            high: Number.isFinite(delta) ? Math.max(0, delta) : null,
            low: Number.isFinite(delta) ? Math.min(0, delta) : null,
            close: delta,
            volume: hasVolume ? volume : null,
        };
    });
}

export function volumeDeltaExtent(bars) {
    const normalized = Array.isArray(bars) ? bars : [];
    const values = normalized.map(bar => bar?.close).filter(Number.isFinite);
    return {
        maxAbsDelta: values.reduce((max, value) => Math.max(max, Math.abs(value)), 0),
        hasVolume: normalized.some(bar => Number.isFinite(Number(bar?.volume)) && Number(bar.volume) > 0),
    };
}
