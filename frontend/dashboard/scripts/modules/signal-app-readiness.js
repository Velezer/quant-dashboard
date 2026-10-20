import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { APP_LABELS } from './signal-screener-model.js';
import { readApiResponse } from './api.js';

export function appSignalReadiness(app, payload, summary = {}) {
    const products = (Array.isArray(payload?.products) ? payload.products : [])
        .filter(product => product.exchange_id === app);
    const sum = key => products.reduce((total, product) => total + (Number(product[key]) || 0), 0);
    const registered = sum('registered');
    const confirmed = sum('confirmed');
    const history = sum('with_15m_candles');
    const error = String(payload?.refresh_errors?.[app] || '');
    const label = APP_LABELS[app] || app;
    const supportsListingRefresh = ['stockbit', 'pintu', 'tokocrypto'].includes(app);
    const coverage = `${label}: ${confirmed} verified of ${registered} registered instruments; ${history} with stored 15m history.`;
    const reason = !registered ? error ? `Listing provider failed: ${error}. No verified listings are available for this app.`
        : supportsListingRefresh ? 'No instruments are registered for this app yet. Retry its listing refresh.'
            : 'No instruments are registered for this app. Its catalog must be restored before scanning can produce signals; use Recheck app data to check again.'
        : !confirmed ? 'Listings are still pending verification; they cannot generate signals yet.'
        : !history ? 'Market history is being gathered. Signals need fresh matching candles before scanning can produce a setup.'
        : Number(summary.stale_price_signal_count) > 0
            ? `${summary.stale_price_signal_count} saved signals are hidden because their price bars are stale. A market closure or provider delay can cause this; fresh matching data and a new scan are required.`
            : 'Models run for this app on each scan. A fresh, qualifying setup is required; an empty result can be valid.';
    return { message: `${coverage} ${reason}${registered && error ? ` Latest listing refresh failed: ${error}. Previous verified listings are retained.` : ''}`,
        retryListings: supportsListingRefresh && Boolean(error || !registered || !confirmed) };
}

export function installSignalAppReadiness() {
    const status = document.getElementById('appSignalReadinessStatus');
    const recheck = document.getElementById('appSignalReadinessRetry');
    const listingRetry = document.getElementById('appSignalListingRetry');
    let selected = 'stockbit';
    let selectedSummary = {};
    let serial = 0;

    async function refresh(app = selected, summary = selectedSummary) {
        selected = app;
        selectedSummary = summary;
        const request = ++serial;
        if (!status) return;
        status.textContent = `Checking ${APP_LABELS[app] || app} listing and data readiness…`;
        listingRetry.hidden = true;
        recheck.disabled = true;
        try {
            const response = await fetch(`/api/listing-status?exchange=${encodeURIComponent(app)}`, { cache: 'no-store' });
            const payload = await readApiResponse(response);
            if (!response.ok || !Array.isArray(payload?.products)) throw new Error(payload?.error || 'Readiness response unavailable');
            if (request !== serial) return;
            const readiness = appSignalReadiness(app, payload, summary);
            status.textContent = readiness.message;
            listingRetry.hidden = !readiness.retryListings;
        } catch (_) {
            if (request === serial) status.textContent = `${APP_LABELS[app] || app} readiness could not be checked. Recheck app data to retry; the signal snapshot remains separate.`;
        } finally {
            if (request === serial) recheck.disabled = false;
        }
    }
    recheck?.addEventListener('click', () => { void refresh(); });
    listingRetry?.addEventListener('click', async () => {
        const app = selected;
        listingRetry.disabled = true;
        status.textContent = `Retrying ${APP_LABELS[app]} listings…`;
        try {
            const response = await fetch(`/api/listing-status/refresh?exchange=${encodeURIComponent(app)}`, { method: 'POST' });
            const payload = await readApiResponse(response);
            if (!response.ok) throw new Error(payload?.error || 'Listing refresh failed');
            if (app === selected) {
                await refresh();
                await getDashboardBinding('triggerManualRefresh')?.();
            }
        } catch (error) {
            if (app === selected) status.textContent = `Listing refresh failed for ${APP_LABELS[app]}: ${String(error?.message || error).slice(0, 250)}. No listing changes were confirmed. Recheck app data or retry when the provider recovers.`;
        } finally {
            listingRetry.disabled = false;
        }
    });
    publishDashboardBindings('signal-app-readiness', { refreshAppSignalReadiness: { get: () => refresh } });
}
