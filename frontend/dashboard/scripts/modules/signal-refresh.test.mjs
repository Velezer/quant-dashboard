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
    globalThis.CustomEvent = class { constructor(type, options) { this.type = type; this.detail = options.detail; } };
    globalThis.fetch = async () => ({ ok: true, text: async () => JSON.stringify({ signals: [], summary: { stale_price_signal_count: 3 }, available_timeframes: ['15m'] }) });
    publishDashboardBindings('refresh-test', {
        getSignalScreenerQuery: { get: () => () => ({ exchange: 'pintu' }) },
        refreshAppSignalReadiness: { get: () => (app, summary) => { readiness = { app, summary }; } },
    });
    try {
        installSignalRefresh();
        getDashboardBinding('initEventSource')();
        source.handlers.get('open')();
        assert.deepEqual(events.pop().detail, { connected: true });
        const alerts = [{ alert_id: 1, exchange_id: 'pintu' }, { alert_id: 2, exchange_id: 'pluang' }];
        source.handlers.get('scan_completed')({ data: JSON.stringify({ new_signal_alerts: alerts }) });
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(events[0].type, 'dashboard:signal-alerts');
        assert.deepEqual(events[0].detail, alerts);
        assert.deepEqual(readiness, { app: 'pintu', summary: { stale_price_signal_count: 3 } });
        assert.doesNotThrow(() => source.handlers.get('scan_completed')({ data: 'invalid JSON' }));
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(events.length, 1);
        source.onerror();
        assert.deepEqual(events.at(-1).detail, { connected: false });
    } finally {
        for (const [key, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, key, descriptor);
            else delete globalThis[key];
        }
    }
});
