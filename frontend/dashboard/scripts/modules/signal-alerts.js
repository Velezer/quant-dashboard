import { getDashboardBinding } from './runtime-context.js';
import { readApiResponse } from './api.js';
import { createBrowserSignalNotifications } from './browser-signal-notifications.js';

/** Shows durable signal alerts from the app database. */
export function installSignalAlerts() {
    const details = document.getElementById('signalAlertsDetails');
    const signalCountBadge = document.getElementById('signalCurrentCount');
    const status = document.getElementById('signalAlertsStatus');
    const retryButton = document.getElementById('signalAlertsRetry');
    const markReadButton = document.getElementById('signalAlertsMarkRead');
    const list = document.getElementById('signalAlertsList');
    let alerts = [];
    let unreadCount = 0;
    let currentSignalCount = null;
    let currentSignalCountStale = true;
    let currentSignalCountError = '';
    let hasLoaded = false;
    let requestInFlight = false;
    let refreshQueued = false;
    let navigationInProgress = false;
    let statusError = '';
    const browserToggle = document.getElementById('browserAlertsToggle');
    const browserTest = document.getElementById('browserAlertsTest');
    const browserStatus = document.getElementById('browserAlertsStatus');
    const browserAlerts = createBrowserSignalNotifications({
        onOpen() { window.focus(); if (details) details.open = true; },
        onChange(state) {
            if (browserStatus) browserStatus.textContent = state.message;
            if (browserToggle) {
                browserToggle.disabled = !state.supported || state.pending;
                browserToggle.textContent = state.enabled ? 'Disable browser alerts' : 'Enable browser alerts';
                browserToggle.setAttribute('aria-pressed', String(state.enabled));
            }
            if (browserTest) browserTest.disabled = !state.enabled || state.pending;
        },
    });
    browserToggle?.addEventListener('click', () => {
        if (browserAlerts.state().enabled) browserAlerts.disable();
        else void browserAlerts.enable();
    });
    browserTest?.addEventListener('click', () => browserAlerts.test());
    window.addEventListener('dashboard:signal-stream', event => browserAlerts.connection(event.detail?.connected));
    browserAlerts.connection(false);

    function setStatus(message, state = 'ready') {
        if (!status) return;
        status.textContent = message;
        status.dataset.state = state;
        status.className = `mt-2 text-xs ${state === 'error' || state === 'stale' ? 'text-amber-300' : 'text-gray-400'}`;
    }

    function renderSignalCount() {
        if (!signalCountBadge) return;
        const countKnown = Number.isSafeInteger(currentSignalCount) && currentSignalCount >= 0;
        const badgeState = !countKnown || currentSignalCountStale ? 'stale' : currentSignalCount > 0 ? 'active' : 'empty';
        signalCountBadge.textContent = countKnown ? String(currentSignalCount) : '—';
        signalCountBadge.setAttribute('aria-label', countKnown
            ? `${currentSignalCount} current signal${currentSignalCount === 1 ? '' : 's'}${currentSignalCountStale ? ', snapshot stale' : ''}`
            : `Current signal count unavailable${currentSignalCountError ? `: ${currentSignalCountError}` : ''}`);
        signalCountBadge.title = countKnown
            ? `Current screener signal count${currentSignalCountStale ? ' · snapshot may be stale' : ''}`
            : `Current signal count unavailable${currentSignalCountError ? ` · ${currentSignalCountError}` : ''}`;
        signalCountBadge.dataset.state = badgeState;
        signalCountBadge.className = `ml-1 rounded-full px-1.5 py-0.5 text-[10px] font-mono ${badgeState === 'active' ? 'bg-cyan-900 text-cyan-100' : badgeState === 'stale' ? 'bg-amber-900 text-amber-100' : 'bg-gray-700 text-gray-300'}`;
        const summary = document.getElementById('signalAlertsToggle');
        if (summary) summary.setAttribute('aria-label', countKnown
            ? `Open notifications; ${currentSignalCount} current signal${currentSignalCount === 1 ? '' : 's'}`
            : 'Open notifications; current signal count unavailable');
    }

    function formatTimestamp(value, nanoseconds = false) {
        const numeric = Number(value);
        if (!Number.isFinite(numeric) || numeric <= 0) return 'time unavailable';
        const milliseconds = nanoseconds
            ? (numeric > 1e15 ? numeric / 1e6 : numeric > 1e12 ? numeric : numeric * 1000)
            : numeric * 1000;
        const date = new Date(milliseconds);
        return Number.isNaN(date.getTime()) ? 'time unavailable' : date.toLocaleString();
    }

    function appendText(parent, tag, text, className = '') {
        const element = document.createElement(tag);
        element.textContent = String(text ?? '');
        if (className) element.className = className;
        parent.append(element);
        return element;
    }

    function isIndicatorAlert(alert) {
        return alert?.alert_kind === 'custom_indicator'
            || String(alert?.signal_strategy_id || '').startsWith('user_indicator_combo_v1:');
    }

    function lifecyclePresentation(alert) {
        const state = String(alert?.lifecycle_state || 'SIGNAL').toUpperCase();
        if (isIndicatorAlert(alert) && state !== 'EXPIRED') {
            return {
                state: 'INDICATOR_ALERT',
                label: 'Indicator alert',
                detail: 'A custom indicator condition was met. It does not create a scored screener entry.',
                classes: 'border-violet-800 bg-violet-950/40 text-violet-200',
            };
        }
        const labels = {
            SIGNAL: { label: 'Signal', detail: 'Informational condition from stored candle data.', classes: 'border-sky-800 bg-sky-950/40 text-sky-200' },
            EXPIRED: { label: 'Expired', detail: 'This signal is older than the two-hour retention window.', classes: 'border-gray-700 bg-gray-900 text-gray-300' },
        };
        return { state, ...(labels[state] || labels.SIGNAL) };
    }

    function renderAlerts() {
        if (!signalCountBadge || !list) return;
        renderSignalCount();
        if (markReadButton) markReadButton.disabled = unreadCount === 0;
        list.replaceChildren();

        for (const alert of alerts) {
            const item = document.createElement('li');
            item.className = `rounded-lg border p-2.5 ${alert.is_read ? 'border-gray-800 bg-gray-900/60' : 'border-amber-800/80 bg-amber-950/20'}`;
            const heading = document.createElement('div');
            heading.className = 'flex items-center justify-between gap-2';
            const directionClass = alert.direction === 'LONG' ? 'text-emerald-300' : 'text-rose-300';
            const indicatorAlert = isIndicatorAlert(alert);
            appendText(heading, 'strong', `${alert.direction} ${indicatorAlert ? 'indicator alert' : 'signal'} · ${alert.symbol}`, `text-xs font-bold ${directionClass}`);
            const appIdentity = [alert.exchange_id, alert.product_id].filter(Boolean).join(' · ') || 'app';
            appendText(heading, 'span', `${appIdentity} · ${alert.timeframe || 'timeframe'}`, 'text-[9px] font-mono text-gray-500');
            item.append(heading);
            appendText(item, 'div', alert.signal_strategy_name || 'Automatic screener signal', 'mt-1 text-[11px] font-semibold text-gray-200');

            const lifecycle = lifecyclePresentation(alert);
            const lifecyclePanel = document.createElement('div');
            lifecyclePanel.className = `mt-2 rounded-md border px-2 py-1.5 ${lifecycle.classes}`;
            appendText(lifecyclePanel, 'div', lifecycle.label, 'text-[10px] font-bold');
            appendText(lifecyclePanel, 'div', lifecycle.detail, 'mt-0.5 text-[9px] leading-relaxed opacity-90');
            if (lifecycle.state === 'EXPIRED' && alert.delete_after) {
                appendText(lifecyclePanel, 'div', `Auto-removal after ${formatTimestamp(alert.delete_after)}.`, 'mt-1 text-[9px] opacity-75');
            }
            item.append(lifecyclePanel);

            const labels = Array.isArray(alert.indicators)
                ? alert.indicators.map(indicator => indicator?.label || indicator?.id).filter(Boolean)
                : [];
            appendText(item, 'div', labels.length ? `Indicators: ${labels.join(' · ')}` : 'Signal details unavailable', 'mt-1 text-[10px] leading-relaxed text-gray-400');

            const evidence = alert.validation_status === 'VALIDATED' ? 'matching evidence validated' : 'matching evidence pending';
            const priceNumber = Number(alert.price);
            const signalPrice = alert.price !== null && alert.price !== undefined && alert.price !== '' && Number.isFinite(priceNumber)
                ? `Signal price ${priceNumber.toLocaleString(undefined, { maximumSignificantDigits: 8 })} · `
                : '';
            appendText(item, 'div', `${signalPrice}candle ${formatTimestamp(alert.signal_bar_time, true)} · detected ${formatTimestamp(alert.created_at)} · ${evidence}`, 'mt-1 text-[9px] leading-relaxed text-gray-500');
            if (!indicatorAlert) {
                const openSignalButton = appendText(item, 'button', 'View current signal', 'mt-2 rounded border border-cyan-800 px-2 py-1 text-[10px] font-semibold text-cyan-200 hover:bg-cyan-950 focus:outline focus:outline-2 focus:outline-cyan-400 disabled:cursor-wait disabled:opacity-50');
                openSignalButton.type = 'button';
                openSignalButton.dataset.openSignalAlert = String(alert.alert_id);
                openSignalButton.disabled = navigationInProgress;
                openSignalButton.setAttribute('aria-label', `Open current ${alert.symbol} ${alert.timeframe} signal in the screener`);
            } else {
                appendText(item, 'div', 'This alert tracks a custom indicator condition and has no scored screener row.', 'mt-2 text-[10px] leading-relaxed text-violet-200');
            }
            const source = [alert.data_source, alert.data_provider]
                .filter(value => value && value !== 'unknown')
                .join(' · ');
            appendText(item, 'div', `Market data: ${source || 'source unavailable'}`, 'mt-1 text-[9px] leading-relaxed text-gray-500');
            appendText(item, 'div', indicatorAlert
                ? 'Indicator condition from stored candle data. It is not a scored screener signal or an order instruction.'
                : 'Informational screener alert from stored candle data. It is not an order instruction.', 'mt-1 text-[9px] leading-relaxed text-amber-200/90');
            list.append(item);
        }

        if (!hasLoaded && !statusError) setStatus('Loading saved signal alerts…', 'loading');
        else if (statusError) setStatus(hasLoaded
            ? `Alert history may be stale. ${statusError}`
            : `Signal alert history is unavailable. ${statusError} Retry to recover.`, hasLoaded ? 'stale' : 'error');
        else if (alerts.length === 0) setStatus('No signal notifications yet. New alerts appear after a completed scan.', 'empty');
        else setStatus(`Showing ${alerts.length} latest signal ${alerts.length === 1 ? 'notification' : 'notifications'} · ${unreadCount} unread across retained history.`, 'ready');
        if (retryButton) retryButton.classList.toggle('hidden', !statusError);
    }

    async function loadAlerts() {
        if (requestInFlight) {
            refreshQueued = true;
            return;
        }
        requestInFlight = true;
        refreshQueued = false;
        statusError = '';
        renderAlerts();
        try {
            const response = await fetch('/api/signal-alerts?limit=50', { cache: 'no-store' });
            const payload = await readApiResponse(response);
            if (!response.ok) throw new Error(payload?.error || `HTTP ${response.status}`);
            if (!payload || !Array.isArray(payload.alerts)) throw new Error('The alert response was incomplete.');
            alerts = payload.alerts;
            unreadCount = Number.isFinite(Number(payload.unread_count)) ? Number(payload.unread_count) : 0;
            hasLoaded = true;
        } catch (error) {
            statusError = String(error?.message || 'request failed').slice(0, 180);
        } finally {
            requestInFlight = false;
            renderAlerts();
            if (refreshQueued) void loadAlerts();
        }
    }

    async function markAllRead() {
        if (!unreadCount || !markReadButton) return;
        markReadButton.disabled = true;
        try {
            const response = await fetch('/api/signal-alerts/ack', { method: 'POST' });
            const payload = await readApiResponse(response);
            if (!response.ok) throw new Error(payload?.error || `HTTP ${response.status}`);
            alerts = alerts.map(alert => ({ ...alert, is_read: true }));
            unreadCount = 0;
            statusError = '';
            renderAlerts();
        } catch (error) {
            statusError = String(error?.message || 'could not update alert status').slice(0, 180);
            renderAlerts();
        }
    }

    if (retryButton) retryButton.addEventListener('click', () => void loadAlerts());
    window.addEventListener('dashboard:signal-inbox-refresh', () => void loadAlerts());
    window.addEventListener('dashboard:current-signal-count', event => {
        const count = event.detail?.count;
        if (Number.isSafeInteger(count) && count >= 0) {
            currentSignalCount = count;
            currentSignalCountStale = event.detail?.stale === true;
            currentSignalCountError = '';
        } else {
            currentSignalCount = null;
            currentSignalCountStale = true;
            currentSignalCountError = String(event.detail?.error || '').slice(0, 160);
        }
        renderSignalCount();
    });
    if (markReadButton) markReadButton.addEventListener('click', () => void markAllRead());
    if (list) list.addEventListener('click', async event => {
        const button = event.target.closest('[data-open-signal-alert]');
        if (!button || navigationInProgress) return;
        const alert = alerts.find(item => String(item.alert_id) === button.dataset.openSignalAlert);
        if (!alert) return;
        const openSignal = getDashboardBinding('openSignalFromAlert');
        navigationInProgress = true;
        renderAlerts();
        setStatus(`Finding the current ${alert.symbol} ${alert.timeframe} signal…`, 'loading');
        let result = 'unavailable';
        try {
            if (typeof openSignal === 'function') result = await openSignal(alert);
        } catch (_) {
            result = 'unavailable';
        }
        navigationInProgress = false;
        renderAlerts();
        if (result === 'opened') {
            if (details) details.open = false;
        } else if (result === 'not_current') {
            setStatus(`This notification is history; ${alert.symbol} ${alert.timeframe} is no longer in the current screener.`, 'stale');
        } else if (result === 'unavailable') {
            setStatus(`Could not check the current ${alert.symbol} signal. Use Refresh signals, then try again.`, 'error');
        }
    });
    if (details) details.addEventListener('toggle', () => {
        if (details.open) void loadAlerts();
    });
    window.addEventListener('dashboard:signal-alerts', event => {
        const incoming = Array.isArray(event.detail) ? event.detail : [];
        if (!incoming.length) return;
        browserAlerts.notify(incoming);
        const first = incoming[0];
        const count = incoming.length;
        const indicatorCount = incoming.filter(isIndicatorAlert).length;
        const alertType = indicatorCount === count ? 'indicator alert' : indicatorCount === 0 ? 'signal alert' : 'market alert';
        const model = first.signal_strategy_name || 'Signal update';
        const toast = getDashboardBinding('showToast');
        if (typeof toast === 'function') {
            toast(`${count} new ${alertType}${count === 1 ? '' : 's'} · ${first.symbol || 'asset'} ${first.direction || ''} · ${model}. Informational only; not an order.`, 'info');
        }
        if (status) setStatus(`${count} new ${alertType}${count === 1 ? '' : 's'} detected.`, 'ready');
        void loadAlerts();
    });

    renderAlerts();
    void loadAlerts();
}
