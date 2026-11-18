const MISSING_STORED_SERIES = /No approved stored ([\w]+) candle series is available for this selected app instrument\./i;

export function missingPineSecurityTimeframes(result, availableTimeframes = []) {
    const allowed = new Set((Array.isArray(availableTimeframes) ? availableTimeframes : [])
        .map(value => String(value || '').toLowerCase()));
    const found = [];
    for (const diagnostic of Array.isArray(result?.diagnostics) ? result.diagnostics : []) {
        const timeframe = String(diagnostic?.message || '').match(MISSING_STORED_SERIES)?.[1]?.toLowerCase();
        if (timeframe && (!allowed.size || allowed.has(timeframe)) && !found.includes(timeframe)) found.push(timeframe);
    }
    return found;
}

export async function runPineWithStoredTimeframes(run, loadTimeframe, availableTimeframes = []) {
    let result = await run();
    const attempted = new Set();
    while (!result?.valid) {
        const missing = missingPineSecurityTimeframes(result, availableTimeframes)
            .filter(timeframe => !attempted.has(timeframe));
        if (!missing.length || typeof loadTimeframe !== 'function') break;
        let loadedAny = false;
        for (const timeframe of missing) {
            attempted.add(timeframe);
            loadedAny = await loadTimeframe(timeframe) || loadedAny;
        }
        if (!loadedAny) break;
        result = await run();
    }
    return result;
}
