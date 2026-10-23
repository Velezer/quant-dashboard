import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { APP_LABELS } from './signal-screener-model.js';
import { readApiResponse } from './api.js';
import { appSignalReadiness, readinessTime } from './signal-readiness-model.js';
export { appSignalReadiness } from './signal-readiness-model.js';

export function installSignalAppReadiness() {
    const panel = document.getElementById('appSignalReadinessPanel');
    const status = document.getElementById('appSignalReadinessStatus');
    const recheck = document.getElementById('appSignalReadinessRetry');
    const scan = document.getElementById('appSignalScanRetry');
    const listingRetry = document.getElementById('appSignalListingRetry');
    const listingCache = new Map();
    let selected = 'stockbit';
    let selectedSummary = {};
    let selectedScope = {};
    let shownKey = '';
    let serial = 0;

    function render(readiness) {
        if (!status || !panel) return;
        panel.dataset.tone = readiness.tone;
        document.getElementById('appSignalReadinessTitle').textContent = readiness.title;
        document.getElementById('appSignalReadinessScope').textContent = readiness.scopeLabel;
        status.textContent = readiness.message;
        document.getElementById('appSignalReadinessNextStep').textContent = readiness.nextStep;
        listingRetry.hidden = !readiness.retryListings;
        const metrics = document.getElementById('appSignalReadinessMetrics');
        metrics.replaceChildren(...readiness.metrics.map(metric => {
            const cell = document.createElement('div');
            const term = document.createElement('dt');
            term.textContent = metric.label;
            const value = document.createElement('dd');
            value.textContent = metric.value;
            cell.append(term, value);
            return cell;
        }));
        metrics.hidden = readiness.metrics.length === 0;
        const details = document.getElementById('appSignalReadinessDetails');
        details.hidden = !readiness.checkedAt && readiness.examples.length === 0;
        document.getElementById('appSignalReadinessCheckedAt').textContent = readiness.checkedAt;
        document.getElementById('appSignalReadinessExamples').replaceChildren(...readiness.examples.map(example => {
            const item = document.createElement('li');
            const title = document.createElement('strong');
            title.textContent = [example.symbol, example.timeframe, String(example.product_id || '').replaceAll('_', ' ')].filter(Boolean).join(' · ');
            const reason = document.createElement('p');
            reason.textContent = String(example.reason || 'Data unavailable').replaceAll('_', ' ');
            const evidence = document.createElement('p');
            const success = example.last_success_at && Date.parse(String(example.last_success_at).replace(' UTC', 'Z').replace(' ', 'T'));
            evidence.textContent = [example.source && `Source: ${example.source}`,
                example.data_provider && `Provider: ${example.data_provider}`,
                example.last_bar_time && `Last candle: ${readinessTime(example.last_bar_time / 1_000_000_000)}`,
                success && `Last successful refresh: ${readinessTime(success / 1000)}`,
                example.required_bars && `History: ${example.available_bars ?? 'unknown'} / ${example.required_bars} required bars`].filter(Boolean).join(' · ');
            item.append(title, reason, evidence);
            if (example.error) {
                const error = document.createElement('p');
                error.className = 'signal-readiness-error';
                error.textContent = `Observed error: ${String(example.error).slice(0, 320)}`;
                item.append(error);
            }
            return item;
        }));
    }

    function show(app, summary = {}, scope = {}) {
        const key = [app, scope.product, scope.timeframe, scope.search, scope.visibleCount].join('|');
        if (key === shownKey && summary === selectedSummary) return;
        shownKey = key;
        selected = app;
        selectedSummary = summary;
        selectedScope = scope;
        serial++;
        if (recheck) recheck.disabled = false;
        render(appSignalReadiness(app, listingCache.get(app), summary, scope));
    }

    async function refresh(app = selected, summary = selectedSummary, scope = selectedScope) {
        show(app, summary, scope);
        const request = ++serial;
        if (!status) return;
        recheck.disabled = true;
        const current = appSignalReadiness(app, listingCache.get(app), summary, scope);
        if (current.hasScanDetails) { recheck.disabled = false; return; }
        status.textContent = `Checking ${APP_LABELS[app] || app} listings; detailed scan checks are not recorded yet…`;
        try {
            const response = await fetch(`/api/listing-status?exchange=${encodeURIComponent(app)}`, { cache: 'no-store' });
            const payload = await readApiResponse(response);
            if (!response.ok || !Array.isArray(payload?.products)) throw new Error(payload?.error || 'Readiness response unavailable');
            if (request !== serial) return;
            listingCache.set(app, payload);
            render(appSignalReadiness(app, payload, summary, scope));
        } catch (error) {
            if (request === serial) unavailable(`App data checks could not be loaded: ${String(error?.message || error).slice(0, 200)}`);
        } finally {
            if (request === serial) recheck.disabled = false;
        }
    }

    function unavailable(error) {
        serial++;
        render({ ...appSignalReadiness(selected, listingCache.get(selected), selectedSummary, selectedScope),
            title: 'Signal availability could not be checked', tone: 'error',
            message: `${String(error || 'The signal service is unavailable').slice(0, 250)}. Any saved checks below may be outdated.`,
            nextStep: 'Recheck status to retry the signal service.' });
        if (recheck) recheck.disabled = false;
    }

    recheck?.addEventListener('click', () => {
        const fetchSignals = getDashboardBinding('fetchFreshSignals');
        if (!fetchSignals) { void refresh(); return; }
        recheck.disabled = true;
        status.textContent = 'Rechecking the signal snapshot and app data checks…';
        void fetchSignals({ quiet: true });
    });
    scan?.addEventListener('click', async () => {
        scan.disabled = true;
        try { await getDashboardBinding('triggerManualRefresh')?.(); }
        finally { scan.disabled = false; }
    });
    listingRetry?.addEventListener('click', async () => {
        const app = selected;
        listingRetry.disabled = true;
        status.textContent = `Retrying ${APP_LABELS[app]} listings…`;
        try {
            const response = await fetch(`/api/listing-status/refresh?exchange=${encodeURIComponent(app)}`, { method: 'POST' });
            const payload = await readApiResponse(response);
            if (!response.ok) throw new Error(payload?.error || 'Listing refresh failed');
            listingCache.delete(app);
            if (app === selected) {
                await refresh();
                await getDashboardBinding('triggerManualRefresh')?.();
            }
        } catch (error) {
            if (app === selected) unavailable(`Listing refresh failed for ${APP_LABELS[app]}: ${String(error?.message || error).slice(0, 200)}`);
        } finally { listingRetry.disabled = false; }
    });
    publishDashboardBindings('signal-app-readiness', {
        refreshAppSignalReadiness: { get: () => refresh },
        showAppSignalReadiness: { get: () => show },
        setAppSignalReadinessError: { get: () => unavailable },
    });
}
