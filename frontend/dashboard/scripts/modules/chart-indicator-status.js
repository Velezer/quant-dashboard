/** Describes the newest built-in NWE point without changing chart state. */
export function describeNweEnvelopeStatus({
    candles = [],
    envelope = null,
    timeframe = '--',
    repaintingMode = false,
    endpointWarmupBars = NaN,
    stale = false,
}) {
    const latest = candles[candles.length - 1];
    let location = 'unavailable';
    if (!latest) {
        location = 'no bars';
    } else if (![envelope?.upper, envelope?.lower].every(Number.isFinite)) {
        const requiredBars = repaintingMode ? 2 : endpointWarmupBars;
        location = Number.isFinite(requiredBars)
            ? `warming up · requires ${requiredBars} history bars`
            : 'unavailable on selected bar';
    } else if (envelope.crossed_above_upper) {
        location = 'crossed above upper · SHORT';
    } else if (envelope.crossed_below_lower) {
        location = 'crossed below lower · LONG';
    } else if (latest.close > envelope.upper) {
        location = 'above upper';
    } else if (latest.close < envelope.lower) {
        location = 'below lower';
    } else {
        location = 'within envelope';
    }

    const normalizedTimeframe = String(timeframe || '--').toUpperCase();
    const modeLabel = repaintingMode ? 'REPAINT' : 'ENDPOINT';
    const staleText = stale ? ' · stale bars' : '';
    const behavior = repaintingMode
        ? 'Historical points use later bars and can repaint.'
        : 'Endpoint values use past bars only.';
    return {
        text: `Latest NWE ${normalizedTimeframe} · ${modeLabel}: ${location}${staleText}`,
        ariaLabel: `Latest Nadaraya-Watson research study on ${normalizedTimeframe} bars in ${modeLabel.toLowerCase()} mode: ${location}${stale ? ', using stale chart data' : ''}. This summary describes the newest bar; the OHLC strip follows the selected chart bar. ${behavior} Crossing markers are study events, not trade instructions.`,
    };
}

/** Formats the causal Markov ribbon's current state for status and assistive text. */
export function describeMarkovRegimeStatus({ enabled, signal, timeframe = '--' }) {
    const normalizedTimeframe = String(timeframe || '--').toUpperCase();
    if (!enabled) {
        return {
            text: 'Markov ribbon off',
            ariaLabel: 'Markov regime ribbon is off.',
        };
    }
    if (!signal) {
        return {
            text: 'Markov: select a signal',
            ariaLabel: 'Select a signal to show its Markov regime ribbon.',
        };
    }
    const history = signal.markov_regimes_by_tf?.[timeframe];
    if (!Array.isArray(history) || history.length === 0) {
        return {
            text: `Markov ${normalizedTimeframe}: waiting for chart data`,
            ariaLabel: `Markov regime history for ${normalizedTimeframe} is waiting for chart data.`,
        };
    }
    const latest = history[history.length - 1]?.state;
    const stale = signal.chart_freshness?.[timeframe]?.is_stale === true;
    const label = latest ? latest.replaceAll('_', ' ') : 'no classified state · warm-up/data unavailable';
    return {
        text: `Markov ${normalizedTimeframe}: ${label}${stale ? ' · stale bars' : ''}`,
        ariaLabel: `Causal Markov regime on ${normalizedTimeframe}: ${label}${stale ? ', using stale chart data' : ''}. Historical state context, not a forecast.`,
    };
}
