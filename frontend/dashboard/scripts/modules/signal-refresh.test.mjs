import test from 'node:test';
import assert from 'node:assert/strict';
import { installSignalRefresh } from './signal-refresh.js';
import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';

test('completed scan events deliver new all-app alerts and refresh readiness; malformed metadata is recoverable', async () => {
    const previous = Object.fromEntries(['document', 'window', 'EventSource', 'CustomEvent', 'fetch', 'DATA'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
    const events = [];
    let source;
    let readiness;
    class MockEventSource {
        handlers = new Map();
        constructor(url) { assert.equal(url, '/api/events'); source = this; }
        addEventListener(name, listener) { this.handlers.set(name, listener); }
        close() {}
    }
    globalThis.document = { getElementById: () => null };
    globalThis.window = { dispatchEvent: event => events.push(event) };
    globalThis.EventSource = MockEventSource;
    globalThis.CustomEvent = class { constructor(type, options = {}) { this.type = type; this.detail = options.detail; } };
    globalThis.fetch = async url => ({ ok: true, text: async () => JSON.stringify(url.startsWith('/api/signals/count')
        ? { signal_count: 0, stale: false }
        : { signals: [], summary: { stale_price_signal_count: 3 }, available_timeframes: ['15m'] }) });
    publishDashboardBindings('refresh-test', {
        getSignalScreenerQuery: { get: () => () => ({ exchange: 'pintu' }) },
        refreshAppSignalReadiness: { get: () => (app, summary, scope) => { readiness = { app, summary, scope }; } },
    });
    try {
        installSignalRefresh();
        getDashboardBinding('initEventSource')();
        source.handlers.get('open')();
        assert.deepEqual(events.pop().detail, { connected: true });
        const alerts = [{ alert_id: 1, exchange_id: 'pintu' }, { alert_id: 2, exchange_id: 'pluang' }];
        source.handlers.get('scan_completed')({ data: JSON.stringify({ new_signal_alerts: alerts }) });
        await new Promise(resolve => setImmediate(resolve));
        assert.deepEqual(events.find(event => event.type === 'dashboard:signal-alerts').detail, alerts);
        assert.deepEqual(readiness, { app: 'pintu', summary: { stale_price_signal_count: 3 }, scope: { exchange: 'pintu', visibleCount: 0 } });
        assert.doesNotThrow(() => source.handlers.get('scan_completed')({ data: 'invalid JSON' }));
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(events.filter(event => event.type === 'dashboard:signal-alerts').length, 1);
        source.onerror();
        assert.deepEqual(events.at(-1).detail, { connected: false });
    } finally {
        for (const [key, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, key, descriptor);
            else delete globalThis[key];
        }
    }
});

test('startup status exposes timeframes without duplicating the initial signal request', async () => {
    const previous = Object.fromEntries(['document', 'window', 'CustomEvent', 'fetch', 'DATA', 'setInterval', 'clearInterval'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
    const requests = [];
    let availableTimeframes = [];
    const timeframes = ['15m', '30m', '1h', '2h', '4h', '1d', '1w'];
    const response = payload => ({ ok: true, status: 200, text: async () => JSON.stringify(payload) });
    globalThis.document = {
        getElementById: id => id === 'countdown-timer' ? { textContent: '' } : null,
    };
    globalThis.window = { dispatchEvent: () => {} };
    globalThis.CustomEvent = class { constructor(type, options = {}) { this.type = type; this.detail = options.detail; } };
    globalThis.setInterval = () => 1;
    globalThis.clearInterval = () => {};
    globalThis.fetch = async url => {
        const path = String(url).split('?')[0];
        requests.push(path);
        if (path === '/api/status') {
            return response({ next_scan_epoch: Math.floor(Date.now() / 1000) + 60, available_timeframes: timeframes });
        }
        if (path === '/api/signals/count') return response({ signal_count: 0, stale: false });
        return response({ signals: [], summary: {}, available_timeframes: timeframes });
    };
    publishDashboardBindings('signal-refresh-startup-test', {
        setScreenerAvailableTimeframes: { get: () => values => { availableTimeframes = values; } },
    });
    try {
        installSignalRefresh();
        getDashboardBinding('startCountdown')();
        await getDashboardBinding('fetchFreshSignals')();
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(requests.filter(path => path === '/api/signals').length, 1);
        assert.deepEqual(availableTimeframes, timeframes);
    } finally {
        for (const [key, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, key, descriptor);
            else delete globalThis[key];
        }
    }
});

test('a new timeframe request aborts the pending query and only applies its snapshot', async () => {
    const previous = Object.fromEntries(['window', 'CustomEvent', 'fetch', 'DATA'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
    const requests = [];
    const applied = [];
    let timeframe = 'all';
    globalThis.window = { dispatchEvent: () => {} };
    globalThis.CustomEvent = class { constructor(type, options = {}) { this.type = type; this.detail = options.detail; } };
    globalThis.fetch = (url, options = {}) => {
        if (url.startsWith('/api/signals?')) {
            return new Promise((resolve, reject) => {
                options.signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
                requests.push({ resolve, signal: options.signal, url });
            });
        }
        return Promise.resolve({ ok: true, text: async () => JSON.stringify({ signal_count: 0 }) });
    };
    publishDashboardBindings('refresh-test', {
        getSignalScreenerQuery: { get: () => () => ({ timeframe }) },
        applySignalSnapshot: { get: () => payload => applied.push(payload) },
    });
    const response = () => ({ ok: true, text: async () => JSON.stringify({
        signals: [], summary: {}, available_timeframes: ['15m', '30m'],
    }) });
    try {
        installSignalRefresh();
        const first = getDashboardBinding('fetchFreshSignals')();
        timeframe = '30m';
        const second = getDashboardBinding('fetchFreshSignals')();
        assert.equal(requests.length, 2);
        assert.equal(requests[0].signal.aborted, true);
        assert.match(requests[0].url, /timeframe=all/);
        assert.match(requests[1].url, /timeframe=30m/);
        requests[1].resolve(response());
        await Promise.all([first, second]);
        assert.equal(applied.length, 1, 'the cancelled snapshot is not applied');
    } finally {
        for (const [key, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, key, descriptor);
            else delete globalThis[key];
        }
    }
});
