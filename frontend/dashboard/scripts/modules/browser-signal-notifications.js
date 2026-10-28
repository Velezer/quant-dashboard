import { APP_LABELS } from './signal-screener-model.js';

/** Session-only browser delivery; fetched inbox history is never passed here. */
export function createBrowserSignalNotifications({
    notificationApi = globalThis.Notification,
    secure = globalThis.isSecureContext === true,
    now = () => Date.now(), onChange = () => {}, onOpen = () => {},
} = {}) {
    let enabled = false;
    let pending = false;
    let connected = false;
    let error = '';
    const seen = new Map();
    const supported = secure && typeof notificationApi === 'function';
    const permission = () => supported ? notificationApi.permission : 'unavailable';
    const isIndicatorAlert = alert => alert?.alert_kind === 'custom_indicator'
        || String(alert?.signal_strategy_id || '').startsWith('user_indicator_combo_v1:');

    function state() {
        if (enabled && permission() !== 'granted') enabled = false;
        const message = !secure ? 'Browser alerts require HTTPS or localhost. The inbox still works.'
            : !supported ? 'Browser notifications are unavailable here. Use the signal inbox.'
            : error || (pending ? 'Waiting for browser permission…'
                : permission() === 'denied' ? 'Notifications are blocked. Allow them in this site’s browser settings, then enable alerts.'
                : enabled ? connected ? 'Browser alerts enabled for all apps while this dashboard stays open.'
                    : 'Browser alerts enabled; the signal stream is reconnecting. Delivery resumes when connected.'
                : 'Browser alerts are off. Enable them to receive new signals from every app while this tab is open.');
        return { supported, enabled, pending, permission: permission(), message };
    }
    const publish = () => onChange(state());

    async function enable() {
        if (!supported || pending) return state();
        error = '';
        pending = true;
        publish();
        try {
            if (permission() === 'default') await notificationApi.requestPermission();
            enabled = permission() === 'granted';
        } catch (_) {
            error = 'Could not request notification permission. Check this site’s browser settings and retry.';
        } finally {
            pending = false;
            publish();
        }
        return state();
    }

    function display(title, body, tag) {
        try {
            const notice = new notificationApi(title, { body, tag, requireInteraction: true });
            notice.onclick = () => { onOpen(); notice.close(); };
            notice.onerror = () => { error = 'The browser could not display a notification. Check browser and operating-system notification settings.'; publish(); };
            error = '';
            publish();
            return true;
        } catch (_) {
            error = 'This browser could not display notifications. Check site permissions; the signal inbox remains available.';
            publish();
            return false;
        }
    }

    function notify(incoming) {
        if (!state().enabled || !Array.isArray(incoming)) return 0;
        const batchKeys = new Set();
        const fresh = incoming.filter(alert => {
            const created = Number(alert?.created_at);
            const id = Number(alert?.alert_id);
            const key = `${id}|${created}`;
            // This is a delivery/replay bound; the server owns financial expiry.
            if (!Number.isSafeInteger(id) || id <= 0 || !Number.isFinite(created)
                || now() - created * 1000 > 5 * 60 * 1000 || created * 1000 > now() + 60_000
                || !Object.hasOwn(APP_LABELS, alert.exchange_id)
                || !['symbol', 'timeframe', 'product_id', 'data_source'].every(field => typeof alert[field] === 'string' && alert[field].trim())
                || !['LONG', 'SHORT'].includes(alert.direction)
                || ['EXPIRED', 'STALE'].includes(String(alert.lifecycle_state || '').toUpperCase())
                || ['stale', 'not_qualifying'].includes(alert.score_status) || seen.has(key) || batchKeys.has(key)) return false;
            batchKeys.add(key);
            return true;
        });
        if (!fresh.length) return 0;
        const apps = [...new Set(fresh.map(alert => APP_LABELS[alert.exchange_id]))];
        const indicatorCount = fresh.filter(isIndicatorAlert).length;
        const batchType = indicatorCount === fresh.length ? 'indicator alerts'
            : indicatorCount === 0 ? 'signals' : 'market alerts';
        const title = fresh.length === 1
            ? `${apps[0]} · ${fresh[0].symbol} ${fresh[0].direction} ${isIndicatorAlert(fresh[0]) ? 'indicator alert' : 'signal'}`
            : `${fresh.length} new ${batchType} · ${apps.join(', ')}`;
        const body = fresh.slice(0, 3).map(alert =>
            `${isIndicatorAlert(alert) ? 'Indicator alert' : 'Saved screener alert'} · ${APP_LABELS[alert.exchange_id]} ${alert.symbol} ${alert.direction} · ${alert.timeframe} · ${alert.product_id} · ${alert.data_source}`
        ).join('\n') + (fresh.length > 3 ? `\n+${fresh.length - 3} more in the notification inbox.` : '');
        const tag = `quant-signals-${fresh.map(alert => `${alert.alert_id}-${alert.created_at}`).join('-').slice(0, 180)}`;
        if (!display(title, body, tag)) return 0;
        for (const key of batchKeys) seen.set(key, now());
        while (seen.size > 512) seen.delete(seen.keys().next().value);
        return fresh.length;
    }

    return {
        state, enable, notify,
        disable() { enabled = false; error = ''; publish(); },
        connection(value) { connected = Boolean(value); publish(); },
        test() {
            if (!state().enabled) return false;
            return display('Signal Screener · browser alerts connected',
                'New signal batches from all apps will appear here while the dashboard is open. Click to open the inbox.', 'quant-signal-test');
        },
    };
}
