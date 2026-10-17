export const CHART_INDICATOR_IDS = Object.freeze({
    nadarayaWatsonEnvelope: 'nadaraya_watson_envelope',
});

export function finiteChartIndicatorValue(value) {
    if (value == null) return null;
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : null;
}

/** Keeps only the supported finite NWE bands and explicit crossing flags. */
export function normalizeChartIndicators(raw) {
    if (!raw || typeof raw !== 'object') return {};
    const normalized = {};
    const envelope = raw[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope];
    if (envelope && typeof envelope === 'object') {
        const modes = {};
        ['repainting', 'endpoint'].forEach(mode => {
            const rawMode = envelope[mode];
            if (!rawMode || typeof rawMode !== 'object') return;
            const values = Object.fromEntries(['upper', 'lower']
                .map(key => [key, finiteChartIndicatorValue(rawMode[key])])
                .filter(([, value]) => value !== null));
            ['crossed_above_upper', 'crossed_below_lower'].forEach(key => {
                if (rawMode[key] === true) values[key] = true;
            });
            if (Object.keys(values).length) modes[mode] = values;
        });
        if (Object.keys(modes).length) normalized[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope] = modes;
    }
    return normalized;
}
