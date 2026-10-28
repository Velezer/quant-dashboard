import test from 'node:test';
import assert from 'node:assert/strict';
import { installSignalRefresh } from './signal-refresh.js';
import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';

test('automatic refresh waits for a slow request and backs off after a failure', async () => {
    const keys = ['document', 'window', 'CustomEvent', 'fetch', 'DATA', 'setInterval', 'clearInterval', 'setTimeout', 'clearTimeout'];
    const previous = Object.fromEntries(keys.map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
    const originalNow = Date.now;
    const intervals = [];
    const requests = [];
    const snapshots = [];
    const timeouts = new Map();
    let timeoutId = 0;
    let now = 100_000;
    Date.now = () => now;
    globalThis.document = { getElementById: id => id === 'countdown-timer' ? {} : null };
    globalThis.window = { dispatchEvent() {} };
    globalThis.CustomEvent = class { constructor(type, options = {}) { this.type = type; this.detail = options.detail; } };
    globalThis.setInterval = callback => { intervals.push(callback); return intervals.length; };
    globalThis.clearInterval = () => {};
    globalThis.setTimeout = callback => { timeouts.set(++timeoutId, callback); return timeoutId; };
    globalThis.clearTimeout = id => timeouts.delete(id);
    globalThis.fetch = (url, options) => {
        if (url.startsWith('/api/signals?')) {
            return new Promise((resolve, reject) => {
                options.signal.addEventListener('abort', () => reject(new Error('aborted')));
                requests.push({ resolve, reject });
            });
        }
        const data = url.startsWith('/api/signals/count') ? { signal_count: 0 } : {};
        return Promise.resolve({ ok: true, text: async () => JSON.stringify(data) });
    };
    publishDashboardBindings('countdown-test', {
        applySignalSnapshot: { get: () => payload => snapshots.push(payload) },
    });
    const response = () => ({ ok: true, text: async () => JSON.stringify({
        signals: [], summary: {}, available_timeframes: ['15m'],
    }) });
    const flush = () => new Promise(resolve => setImmediate(resolve));
    try {
        installSignalRefresh();
        getDashboardBinding('startCountdown')();
        assert.equal(requests.length, 1);
        now += 40_000;
        intervals[0]();
        assert.equal(requests.length, 1, 'a slow request is not superseded by the timer');
        requests[0].resolve(response());
        await flush();
        assert.equal(snapshots.length, 1, 'the slow response is applied');

        now += 20_000;
        intervals[0]();
        assert.equal(requests.length, 2);
        requests[1].reject(new Error('network unavailable'));
        await flush();
        for (let i = 0; i < 19; i += 1) {
            now += 1_000;
            intervals[0]();
        }
        assert.equal(requests.length, 2, 'failures do not trigger one request per tick');
        now += 1_000;
        intervals[0]();
        assert.equal(requests.length, 3, 'the timer retries after the cooldown');
        now += 60_000;
        assert.equal(timeouts.size, 1);
        [...timeouts.values()][0]();
        await flush();
        intervals[0]();
        assert.equal(requests.length, 4, 'a timed-out request does not block recovery');
        requests[3].resolve(response());
        await flush();
        assert.equal(snapshots.length, 2, 'a later successful response restores the screener');
    } finally {
        Date.now = originalNow;
        for (const [key, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, key, descriptor);
            else delete globalThis[key];
        }
    }
});
