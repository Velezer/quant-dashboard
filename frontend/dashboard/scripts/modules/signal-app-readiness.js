import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { APP_LABELS } from './signal-screener-model.js';
import { readApiResponse } from './api.js';
import { appSignalReadiness, readinessTime } from './signal-readiness-model.js';
export { appSignalReadiness } from './signal-readiness-model.js';

const LIVE_SCORING_CACHE_TTL_MS = 30_000;

export function installSignalAppReadiness() {
    const panel = document.getElementById('appSignalReadinessPanel');
    const resultsPanel = document.getElementById('signalResultsPanel');
    const scanInfoToggle = document.getElementById('scanInfoToggle');
    const scanInfoSummary = document.getElementById('scanInfoSummary');
    const status = document.getElementById('appSignalReadinessStatus');
    const recheck = document.getElementById('appSignalReadinessRetry');
    const scan = document.getElementById('appSignalScanRetry');
    const listingRetry = document.getElementById('appSignalListingRetry');
    const listingCache = new Map();
    let liveScoringCache = null;
    let liveScoringRequest = null;
    let liveScoringCacheLoadedAt = 0;
    let selected = 'stockbit';
    let selectedSummary = {};
    let selectedScope = {};
    let shownKey = '';
    let serial = 0;
    let liveScoringLoadScheduleVersion = 0;
    resultsPanel?.setAttribute('data-scan-info-open', 'false');

    function setScanInfoOpen(open, restoreFocus = false) {
        resultsPanel?.setAttribute('data-scan-info-open', String(open));
        scanInfoToggle?.setAttribute('aria-expanded', String(open));
        if (scanInfoToggle && restoreFocus) scanInfoToggle.focus();
    }

    function renderMetrics(container, items, highlight = false) {
        container.replaceChildren(...items.map(metric => {
            const cell = document.createElement('div');
            if (metric.highlight) cell.classList.add('signal-readiness-metric-highlight');
            if (metric.label === 'Qualifying rows at scan') cell.classList.add('signal-readiness-metric-count');
            const term = document.createElement('dt');
            term.textContent = highlight && metric.label === 'Qualifying rows at scan' ? 'Signals found at last scan' : metric.label;
            const value = document.createElement('dd');
            value.textContent = metric.value;
            if (metric.detail) {
                value.title = metric.detail;
                value.setAttribute('aria-label', `${metric.value}. ${metric.detail}`);
            }
            cell.append(term, value);
            if (metric.detail && metric.highlight) {
                const detail = document.createElement('small');
                detail.textContent = metric.detail;
                cell.append(detail);
            }
            return cell;
        }));
        container.hidden = items.length === 0;
    }

    function renderCurrent() {
        const summary = liveScoringCache
            ? { ...selectedSummary, live_scoring: liveScoringCache }
            : selectedSummary;
        render(appSignalReadiness(selected, listingCache.get(selected), summary, selectedScope));
    }

    async function loadLiveScoring() {
        if (Date.now() - liveScoringCacheLoadedAt < LIVE_SCORING_CACHE_TTL_MS) return liveScoringCache;
        if (!liveScoringRequest) {
            liveScoringRequest = (async () => {
                const response = await fetch('/api/entry-validation/status', { cache: 'no-store' });
                const payload = await readApiResponse(response);
                if (response.ok && payload?.live_scoring) liveScoringCache = payload.live_scoring;
                liveScoringCacheLoadedAt = Date.now();
                return liveScoringCache;
            })().catch(() => {
                liveScoringCacheLoadedAt = Date.now();
                return liveScoringCache;
            }).finally(() => { liveScoringRequest = null; });
        }
        return liveScoringRequest;
    }

    function scheduleLiveScoringLoad(app, showSerial) {
        const scheduleVersion = ++liveScoringLoadScheduleVersion;
        let started = false;
        let fallbackTimer = 0;
        const start = () => {
            if (started) return;
            started = true;
            window.clearTimeout(fallbackTimer);
            if (scheduleVersion !== liveScoringLoadScheduleVersion
                || showSerial !== serial || selected !== app
                || Date.now() - liveScoringCacheLoadedAt < LIVE_SCORING_CACHE_TTL_MS) return;
            void loadLiveScoring().then(scoring => {
                if (scoring && serial === showSerial && selected === app) renderCurrent();
            });
        };
        fallbackTimer = window.setTimeout(start, 50);
        if (typeof window.requestAnimationFrame === 'function') {
            window.requestAnimationFrame(() => window.setTimeout(start, 0));
        } else {
            window.setTimeout(start, 0);
        }
    }

    function render(readiness) {
        if (!status || !panel) return;
        panel.dataset.tone = readiness.tone;
        if (scanInfoSummary) scanInfoSummary.textContent = readiness.title;
        if (scanInfoToggle) {
            scanInfoToggle.dataset.tone = readiness.tone;
            scanInfoToggle.setAttribute('aria-label', `${scanInfoToggle.getAttribute('aria-expanded') === 'true' ? 'Hide' : 'Show'} scan info. ${readiness.title}.`);
        }
        document.getElementById('appSignalReadinessTitle').textContent = readiness.title;
        document.getElementById('appSignalReadinessScope').textContent = readiness.scopeLabel;
        status.textContent = readiness.message;
        document.getElementById('appSignalReadinessNextStep').textContent = readiness.nextStep;
        listingRetry.hidden = !readiness.retryListings;
        const highlightMetrics = readiness.metrics.filter(metric => metric.highlight || metric.label === 'Qualifying rows at scan');
        const diagnosticMetrics = readiness.metrics.filter(metric => !metric.highlight && metric.label !== 'Qualifying rows at scan');
        renderMetrics(document.getElementById('appSignalReadinessHighlights'), highlightMetrics, true);
        renderMetrics(document.getElementById('appSignalReadinessMetrics'), diagnosticMetrics);
        const details = document.getElementById('appSignalReadinessDetails');
        details.hidden = !readiness.checkedAt && readiness.examples.length === 0 && diagnosticMetrics.length === 0;
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
        renderCurrent();
        if (Date.now() - liveScoringCacheLoadedAt >= LIVE_SCORING_CACHE_TTL_MS) {
            const showSerial = serial;
            scheduleLiveScoringLoad(app, showSerial);
        }
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
            render(appSignalReadiness(app, payload, liveScoringCache
                ? { ...summary, live_scoring: liveScoringCache } : summary, scope));
        } catch (error) {
            if (request === serial) unavailable(`App data checks could not be loaded: ${String(error?.message || error).slice(0, 200)}`);
        } finally {
            if (request === serial) recheck.disabled = false;
        }
    }

    function unavailable(error) {
        serial++;
        const summary = liveScoringCache
            ? { ...selectedSummary, live_scoring: liveScoringCache }
            : selectedSummary;
        render({ ...appSignalReadiness(selected, listingCache.get(selected), summary, selectedScope),
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
    scanInfoToggle?.addEventListener('click', () => {
        setScanInfoOpen(scanInfoToggle.getAttribute('aria-expanded') !== 'true');
        const title = scanInfoSummary?.textContent || 'Signal availability';
        scanInfoToggle.setAttribute('aria-label', `${scanInfoToggle.getAttribute('aria-expanded') === 'true' ? 'Hide' : 'Show'} scan info. ${title}.`);
    });
    document.addEventListener('pointerdown', event => {
        if (scanInfoToggle?.getAttribute('aria-expanded') !== 'true'
            || panel?.contains(event.target) || scanInfoToggle?.contains(event.target)) return;
        setScanInfoOpen(false);
        scanInfoToggle?.setAttribute('aria-label', `Show scan info. ${scanInfoSummary?.textContent || 'Signal availability'}.`);
    });
    document.addEventListener('keydown', event => {
        if (event.key !== 'Escape' || scanInfoToggle?.getAttribute('aria-expanded') !== 'true') return;
        setScanInfoOpen(false, true);
        scanInfoToggle?.setAttribute('aria-label', `Show scan info. ${scanInfoSummary?.textContent || 'Signal availability'}.`);
    });
    publishDashboardBindings('signal-app-readiness', {
        refreshAppSignalReadiness: { get: () => refresh },
        showAppSignalReadiness: { get: () => show },
        setAppSignalReadinessError: { get: () => unavailable },
    });
}
