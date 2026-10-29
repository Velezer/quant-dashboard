/** Owns the screener's scan status and visible-row KPI presentation. */
export function createSignalScreenerSummary({
    getSummary,
    getRequestState,
    getRequestError,
    getRows,
    getVisibleSignals,
    stockbitSignalModelsForView,
    signalPriceCoverageMessage,
    ageLabel,
    formatTime,
    signalScoreState,
    scoreModelFor,
    averageNormalizedScore,
    escape,
}) {
    function updateStatusText(visibleCount) {
        const status = document.getElementById('screener-status-text');
        const freshness = document.getElementById('screener-freshness');
        const retry = document.getElementById('screener-retry');
        const summary = getSummary();
        const rows = getRows();
        const requestState = getRequestState();
        const summaryStatus = summary.status || {};
        const generatedAt = summary.generated_at || summaryStatus.last_success_at;
        const stale = !!summary.stale || summaryStatus.last_attempt_status === 'error';
        const stalePriceSignalCount = Number(summary.stale_price_signal_count || 0);
        const priceDataStatus = String(summaryStatus.price_data_status || 'ready');
        const priceDataUnavailableCount = Number(summaryStatus.price_data_unavailable_count || 0);
        const priceCoverageMessage = signalPriceCoverageMessage(priceDataStatus, priceDataUnavailableCount);
        const attemptError = String(summaryStatus.last_error || getRequestError() || '').trim();
        const stockbitSignalSelected = stockbitSignalModelsForView().length > 0;
        if (requestState === 'loading' && rows.length === 0) {
            status.textContent = summary.generated_at || summaryStatus.last_success_at
                ? 'Loading the latest signal snapshot…'
                : 'Loading the first signal snapshot. Use Refresh signals above to start a scan.';
            retry.classList.add('hidden');
        } else if (requestState === 'error') {
            status.textContent = rows.length
                ? `Could not refresh signals. The last saved snapshot is still shown. ${attemptError}`
                : `Signal data is unavailable. ${attemptError || 'Check the service and retry.'}`;
            retry.classList.remove('hidden');
        } else if (summaryStatus.last_attempt_status === 'waiting') {
            status.textContent = 'Waiting for the first scheduled scan. Use Refresh signals to run one now.';
            retry.classList.remove('hidden');
        } else if (summaryStatus.last_attempt_status === 'error') {
            status.textContent = `The latest scan failed. Showing the last successful snapshot where available. ${attemptError}`;
            retry.classList.remove('hidden');
        } else if (stale) {
            status.textContent = `Snapshot is stale. Last successful scan ${ageLabel(generatedAt)}. Retry starts a new scan.`;
            retry.classList.remove('hidden');
        } else if (priceCoverageMessage) {
            const currentViewMessage = visibleCount > 0
                ? 'Visible signals remain labeled with their own source and freshness.'
                : 'The selected filters have no current rows. Retry after market data recovers.';
            status.textContent = `${priceCoverageMessage} ${currentViewMessage}`;
            retry.classList.remove('hidden');
        } else {
            const staleReference = stalePriceSignalCount > 0
                ? ` · ${stalePriceSignalCount} setup${stalePriceSignalCount === 1 ? '' : 's'} shown with stale reference prices; not market entries`
                : '';
            status.textContent = `Updated ${ageLabel(generatedAt)}${staleReference}`;
            retry.classList.add('hidden');
        }
        freshness.textContent = generatedAt
            ? stockbitSignalSelected
                ? `Scan ${formatTime(generatedAt)} WIB · per timeframe`
                : `Scan ${formatTime(generatedAt)} WIB · latest-bar signals`
            : requestState === 'loading'
                ? ''
                : 'No scan yet.';
        const panel = document.getElementById('screener-status');
        panel?.classList.toggle('screener-status-stale', stale || requestState === 'error' || stalePriceSignalCount > 0 || Boolean(priceCoverageMessage));
        panel?.classList.toggle('screener-status-ready', !stale && requestState === 'ready' && stalePriceSignalCount === 0 && !priceCoverageMessage);
    }

    function renderKpis(visibleSignals = getVisibleSignals()) {
        const grid = document.getElementById('kpi-grid');
        if (!grid) return;
        const currentSignals = visibleSignals.filter(signal => signalScoreState(signal)?.current !== false);
        const longCount = currentSignals.filter(signal => signal.direction === 'LONG').length;
        const shortCount = currentSignals.filter(signal => signal.direction === 'SHORT').length;
        const avg = averageNormalizedScore(currentSignals.map(signal => ({
            ...signal,
            scoreModel: scoreModelFor(signal),
        })));
        const cells = [
            ['SIGNAL ROWS', String(visibleSignals.length), 'Matching app and filters'],
            ['LONG MARKET ENTRIES', String(longCount), 'Current technical signals only'],
            ['SHORT MARKET ENTRIES', String(shortCount), 'Current technical signals only'],
            ['MEAN SETUP STRENGTH', avg === null ? '—' : `${avg.toFixed(1)}%`, 'Across current entry signals; not win probability'],
        ];
        grid.innerHTML = cells.map(([label, value, detail]) => `<section class="signal-kpi"><p>${escape(label)}</p><strong>${escape(value)}</strong><span>${escape(detail)}</span></section>`).join('');
    }

    return { updateStatusText, renderKpis };
}
