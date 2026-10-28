import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { readApiResponse } from './api.js';

const SIGNAL_COUNT_REFRESH_MIN_INTERVAL_MS = 30_000;

/** Coordinates the 15-minute scan view, refresh action, and reconnect recovery. */
export function installSignalRefresh() {
    let lastRequestAt = 0;
    let requestSerial = 0;
    let signalRequestsInFlight = 0;
    let nextScanEpoch = 0;
    let countdownPoll = null;
    let eventSource = null;
    let signalCountRequestInFlight = false;
    let signalCountRefreshPending = false;
    let lastSignalCountRequestAt = 0;

    async function fetchCurrentSignalCount(force = false) {
        if (!force && Date.now() - lastSignalCountRequestAt < SIGNAL_COUNT_REFRESH_MIN_INTERVAL_MS) return;
        if (signalCountRequestInFlight) {
            if (force) signalCountRefreshPending = true;
            return;
        }
        signalCountRequestInFlight = true;
        lastSignalCountRequestAt = Date.now();
        try {
            const response = await fetch('/api/signals/count', { cache: 'no-store' });
            const payload = await readApiResponse(response);
            const count = payload?.signal_count;
            if (!response.ok || !Number.isSafeInteger(count) || count < 0) {
                throw new Error(payload?.error || `HTTP ${response.status}`);
            }
            window.dispatchEvent(new CustomEvent('dashboard:current-signal-count', {
                detail: { count, stale: payload.stale === true, generated_at: payload.generated_at },
            }));
        } catch (error) {
            window.dispatchEvent(new CustomEvent('dashboard:current-signal-count', {
                detail: { count: null, stale: true, error: String(error?.message || 'count unavailable').slice(0, 160) },
            }));
        } finally {
            signalCountRequestInFlight = false;
            if (signalCountRefreshPending) {
                signalCountRefreshPending = false;
                void fetchCurrentSignalCount(true);
            }
        }
    }

    async function fetchFreshSignals({ quiet = false, append = false, offset = 0 } = {}) {
        const currentRequest = ++requestSerial;
        lastRequestAt = Date.now();
        signalRequestsInFlight += 1;
        if (!quiet && !getDashboardBinding('DATA')) getDashboardBinding('setScreenerLoading')?.();
        try {
            const query = getDashboardBinding('getSignalScreenerQuery')?.() || {};
            const params = new URLSearchParams({
                exchange: String(query.exchange || 'stockbit'),
                product: String(query.product || 'all'),
                timeframe: String(query.timeframe || 'all'),
                search: String(query.search || ''),
                offset: String(offset),
                limit: String(query.limit || 100),
            });
            const response = await fetch(`/api/signals?${params}`, { cache: 'no-store' });
            const payload = await readApiResponse(response);
            if (!response.ok) throw new Error(payload?.error || `HTTP ${response.status}`);
            if (!payload || !Array.isArray(payload.signals) || typeof payload.summary !== 'object'
                || !Array.isArray(payload.available_timeframes) || payload.available_timeframes.length === 0
                || payload.available_timeframes.some(frame => typeof frame !== 'string' || !frame.trim())) {
                throw new Error('The signal service returned an incomplete screener snapshot.');
            }
            if (currentRequest !== requestSerial) return;
            if (append && globalThis.DATA && Array.isArray(globalThis.DATA.signals)) {
                const merged = new Map(globalThis.DATA.signals.map(signal => [
                    [signal.exchange_id, signal.product_id, signal.asset_category_id, signal.symbol,
                        signal.signal_strategy_id, signal.timeframe].join('|'), signal,
                ]));
                for (const signal of payload.signals) {
                    merged.set([signal.exchange_id, signal.product_id, signal.asset_category_id,
                        signal.symbol, signal.signal_strategy_id, signal.timeframe].join('|'), signal);
                }
                payload.signals = [...merged.values()];
            }
            globalThis.DATA = payload;
            if (payload.master_data && typeof payload.master_data === 'object') {
                getDashboardBinding('applyMasterData')?.(payload.master_data);
            }
            getDashboardBinding('applySignalSnapshot')?.(payload, { append });
            void getDashboardBinding('refreshAppSignalReadiness')?.(String(query.exchange || 'stockbit'), payload.summary,
                { ...query, visibleCount: getDashboardBinding('getFilteredSignals')?.().length ?? payload.signals.length });
            if (!append) void fetchCurrentSignalCount();
            lastRequestAt = Date.now();
        } catch (error) {
            if (currentRequest === requestSerial) {
                getDashboardBinding('setScreenerFetchError')?.(error?.message || error);
                getDashboardBinding('setAppSignalReadinessError')?.(error?.message || error);
            }
        } finally {
            signalRequestsInFlight -= 1;
        }
    }

    async function fetchRuntimeStatus() {
        try {
            const response = await fetch('/api/status', { cache: 'no-store' });
            const status = await readApiResponse(response);
            if (!response.ok) throw new Error(status?.error || `HTTP ${response.status}`);
            nextScanEpoch = Number(status.next_scan_epoch) || 0;
            getDashboardBinding('setNextScanEpoch')?.(nextScanEpoch);
            if (status.is_scanning) showScanBanner(status.scan_progress_pct || 0, status.current_scan_symbol || 'Scanning signals');
            else hideScanBanner();
            return status;
        } catch (error) {
            console.warn('[Screener] Runtime status unavailable:', error);
            return null;
        }
    }

    function renderCountdown() {
        const timer = document.getElementById('countdown-timer');
        if (!timer) return;
        const remaining = nextScanEpoch > 0
            ? Math.max(0, Math.ceil(nextScanEpoch - Date.now() / 1000))
            : 0;
        const minutes = Math.floor(remaining / 60);
        const seconds = String(remaining % 60).padStart(2, '0');
        timer.textContent = `${minutes}m ${seconds}s`;
        if (remaining === 0 && signalRequestsInFlight === 0 && Date.now() - lastRequestAt >= 20_000) {
            void fetchFreshSignals({ quiet: true });
            void fetchRuntimeStatus();
        }
    }

    function startCountdown() {
        renderCountdown();
        if (countdownPoll) clearInterval(countdownPoll);
        countdownPoll = setInterval(() => {
            renderCountdown();
            if (signalRequestsInFlight === 0 && Date.now() - lastRequestAt >= 60_000) void fetchFreshSignals({ quiet: true });
        }, 1_000);
        setInterval(() => { void fetchRuntimeStatus(); }, 30_000);
        setInterval(() => {
            void fetch('/api/heartbeat', { method: 'POST' }).catch(() => {});
        }, 30_000);
    }

    function showScanBanner(progress, symbol) {
        const banner = document.getElementById('scan-progress-banner');
        if (!banner) return;
        banner.classList.remove('hidden');
        const title = document.getElementById('scan-progress-title');
        const subtitle = document.getElementById('scan-progress-subtitle');
        const currentSymbol = document.getElementById('scan-current-symbol');
        const bar = document.getElementById('scan-progress-bar');
        const pct = Math.max(0, Math.min(100, Number(progress) || 0));
        if (title) title.textContent = 'Refreshing signal screener…';
        if (subtitle) subtitle.firstChild.textContent = 'Scanning stored market data · ';
        if (currentSymbol) currentSymbol.textContent = String(symbol || 'Scanning registered instruments');
        if (bar) bar.style.width = `${pct}%`;
        const percentage = document.getElementById('scan-progress-pct');
        if (percentage) percentage.textContent = `${Math.round(pct)}%`;
    }

    function hideScanBanner() {
        document.getElementById('scan-progress-banner')?.classList.add('hidden');
        const icon = document.getElementById('refreshIcon');
        icon?.classList.remove('animate-spin');
    }

    function updateSseBadge(connected) {
        window.dispatchEvent(new CustomEvent('dashboard:signal-stream', { detail: { connected } }));
        const badge = document.getElementById('sse-badge');
        const dot = document.getElementById('sse-dot');
        const label = document.getElementById('sse-status');
        if (!badge || !dot || !label) return;
        label.textContent = connected ? 'EVENTS ON' : 'RECONNECTING';
        badge.setAttribute('aria-label', connected ? 'App update stream connected' : 'App update stream reconnecting');
        dot.className = `w-2 h-2 rounded-full ${connected ? 'bg-emerald-400 animate-live' : 'bg-amber-400'}`;
        badge.classList.toggle('bg-emerald-950', connected);
        badge.classList.toggle('text-emerald-400', connected);
        badge.classList.toggle('border-emerald-800', connected);
        badge.classList.toggle('bg-amber-950', !connected);
        badge.classList.toggle('text-amber-300', !connected);
        badge.classList.toggle('border-amber-800', !connected);
    }

    function showToast(message, severity = 'info') {
        let host = document.getElementById('screener-toast-host');
        if (!host) {
            host = document.createElement('div');
            host.id = 'screener-toast-host';
            host.className = 'fixed bottom-4 right-4 z-50 flex max-w-sm flex-col gap-2';
            host.setAttribute('aria-live', 'polite');
            document.body.append(host);
        }
        const toast = document.createElement('div');
        toast.className = `rounded-lg border px-3 py-2 text-xs shadow-xl ${severity === 'error' ? 'border-rose-800 bg-rose-950 text-rose-100' : severity === 'warning' ? 'border-amber-800 bg-amber-950 text-amber-100' : 'border-gray-700 bg-gray-900 text-gray-100'}`;
        toast.textContent = String(message || 'Signal screener updated');
        host.append(toast);
        setTimeout(() => toast.remove(), 5_000);
    }

    async function triggerManualRefresh() {
        const icon = document.getElementById('refreshIcon');
        icon?.classList.add('animate-spin');
        try {
            const response = await fetch('/api/refresh', { method: 'POST' });
            const payload = await readApiResponse(response).catch(() => ({}));
            if (response.status === 202) {
                showToast('Signal scan queued. Market data refreshes in the background.');
                showScanBanner(0, 'Starting scan');
                void fetchRuntimeStatus();
                return;
            }
            if (response.status === 409) {
                showToast('A signal scan is already running.');
                showScanBanner(payload.progress_pct || 0, payload.current_symbol || 'Scanning registered instruments');
                return;
            }
            throw new Error(payload?.error || `HTTP ${response.status}`);
        } catch (error) {
            icon?.classList.remove('animate-spin');
            showToast(`Could not start a scan: ${String(error?.message || error).slice(0, 160)}`, 'error');
            await fetchFreshSignals();
        }
    }

    function initEventSource() {
        if (eventSource) eventSource.close();
        eventSource = new EventSource('/api/events');
        eventSource.addEventListener('open', () => {
            updateSseBadge(true);
            void fetchCurrentSignalCount(true);
        });
        eventSource.addEventListener('connected', event => {
            updateSseBadge(true);
            try {
                const data = JSON.parse(event.data);
                if (data.next_scan_epoch) {
                    nextScanEpoch = Number(data.next_scan_epoch);
                    getDashboardBinding('setNextScanEpoch')?.(nextScanEpoch);
                }
                if (data.is_scanning === true) {
                    showScanBanner(
                        data.scan_progress_pct || 0,
                        data.current_scan_symbol || 'Scanning registered instruments',
                    );
                } else if (data.is_scanning === false) {
                    hideScanBanner();
                }
            } catch (_) { /* Keep the status stream alive if optional event metadata is malformed. */ }
        });
        eventSource.addEventListener('scan_started', () => showScanBanner(0, 'Starting scan'));
        eventSource.addEventListener('scan_progress', event => {
            try {
                const data = JSON.parse(event.data);
                showScanBanner(data.progress_pct || 0, data.symbol || data.current_symbol || 'Scanning registered instruments');
            } catch (_) { showScanBanner(0, 'Scanning registered instruments'); }
        });
        eventSource.addEventListener('scan_completed', event => {
            hideScanBanner();
            window.dispatchEvent(new CustomEvent('dashboard:signal-inbox-refresh'));
            void fetchCurrentSignalCount(true);
            try {
                const data = JSON.parse(event.data);
                nextScanEpoch = Number(data.next_scan_epoch) || nextScanEpoch;
                getDashboardBinding('setNextScanEpoch')?.(nextScanEpoch);
                if (Array.isArray(data.new_signal_alerts) && data.new_signal_alerts.length) {
                    window.dispatchEvent(new CustomEvent('dashboard:signal-alerts', { detail: data.new_signal_alerts }));
                }
            } catch (_) { /* The API snapshot remains authoritative. */ }
            lastRequestAt = 0;
            void fetchFreshSignals({ quiet: true });
        });
        eventSource.addEventListener('scan_failed', () => {
            hideScanBanner();
            void fetchCurrentSignalCount(true);
            lastRequestAt = 0;
            void fetchFreshSignals({ quiet: true });
        });
        eventSource.onerror = () => updateSseBadge(false);
    }

    publishDashboardBindings('signal-refresh', {
        fetchFreshSignals: { get: () => fetchFreshSignals },
        startCountdown: { get: () => startCountdown },
        initEventSource: { get: () => initEventSource },
        triggerManualRefresh: { get: () => triggerManualRefresh },
        showScanBanner: { get: () => showScanBanner },
        updateSseBadge: { get: () => updateSseBadge },
        showToast: { get: () => showToast },
    });
    void fetchCurrentSignalCount();
}
