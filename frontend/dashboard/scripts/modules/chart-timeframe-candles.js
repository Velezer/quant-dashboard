export function chartCandlesForTimeframe(signal, timeframe) {
    const frame = String(timeframe || '').trim().toLowerCase();
    const byTimeframe = signal?.candles_by_tf;
    if (byTimeframe && Object.prototype.hasOwnProperty.call(byTimeframe, frame)) {
        return Array.isArray(byTimeframe[frame]) ? byTimeframe[frame] : [];
    }
    const assignedFrame = String(signal?._chartCandlesTimeframe || '').toLowerCase();
    if (assignedFrame) {
        return assignedFrame === frame && Array.isArray(signal.candles) ? signal.candles : [];
    }
    return String(signal?.timeframe || '').toLowerCase() === frame && Array.isArray(signal?.candles)
        ? signal.candles
        : [];
}

export function chartTimeframeWasLoaded(signal, timeframe) {
    const frame = String(timeframe || '').trim().toLowerCase();
    return signal?.chart_data_loaded_by_tf?.[frame] === true
        && !!signal?.candles_by_tf
        && Object.prototype.hasOwnProperty.call(signal.candles_by_tf, frame);
}
