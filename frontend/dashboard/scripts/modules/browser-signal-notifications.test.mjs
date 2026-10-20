import test from 'node:test';
import assert from 'node:assert/strict';
import { createBrowserSignalNotifications } from './browser-signal-notifications.js';

const now = 1_792_500_000_000;
function harness(permission = 'default', extra = {}) {
    const notices = [];
    let requests = 0;
    class Notification {
        static permission = permission;
        static async requestPermission() { requests++; this.permission = 'granted'; return 'granted'; }
        constructor(title, options) { this.title = title; this.options = options; notices.push(this); }
        close() { this.closed = true; }
    }
    const controller = createBrowserSignalNotifications({ notificationApi: Notification, secure: true, now: () => now, ...extra });
    return { controller, notices, Notification, requests: () => requests };
}
function alert(id = 1, app = 'pintu', extra = {}) {
    return { alert_id: id, created_at: now / 1000, exchange_id: app, product_id: `${app}_spot`,
        symbol: 'BTC-USD', timeframe: '15m', direction: 'LONG', data_source: `${app}_yfinance_fallback`, ...extra };
}

test('permission is requested only when enabled; disable and revocation stop delivery', async () => {
    const h = harness();
    assert.equal(h.requests(), 0);
    assert.equal(h.controller.notify([alert()]), 0);
    await h.controller.enable();
    assert.equal(h.requests(), 1);
    assert.equal(h.controller.state().enabled, true);
    h.controller.connection(true);
    assert.match(h.controller.state().message, /all apps/);
    h.controller.disable();
    assert.equal(h.controller.notify([alert()]), 0);
    await h.controller.enable();
    assert.equal(h.requests(), 1);
    h.Notification.permission = 'denied';
    assert.equal(h.controller.notify([alert()]), 0);
    assert.match(h.controller.state().message, /blocked/);
});

test('all apps share one batch notification, with provenance, deduplication, and inbox click', async () => {
    let opened = 0;
    const h = harness('granted', { onOpen: () => opened++ });
    await h.controller.enable();
    const batch = ['stockbit', 'pintu', 'pluang', 'tokocrypto'].map((app, i) => alert(i + 1, app));
    assert.equal(h.controller.notify([...batch, batch[0]]), 4);
    assert.equal(h.notices.length, 1);
    assert.match(h.notices[0].title, /4 new signals.*Stockbit.*Pintu.*Pluang.*Tokocrypto/);
    assert.match(h.notices[0].options.body, /15m.*stockbit_spot.*stockbit_yfinance_fallback/);
    assert.match(h.notices[0].options.body, /\+1 more/);
    assert.equal(h.notices[0].options.requireInteraction, true);
    h.notices[0].onclick();
    assert.equal(opened, 1);
    assert.equal(h.notices[0].closed, true);
    assert.equal(h.controller.notify(batch), 0);
    // Alert IDs may be reused after old inbox rows expire; detection time remains part of identity.
    assert.equal(h.controller.notify([alert(1, 'pintu', { created_at: now / 1000 + 1 })]), 1);
});

test('stale, expired, replayed, malformed, and non-qualifying events never notify', async () => {
    const h = harness('granted');
    await h.controller.enable();
    for (const bad of [null, {}, alert(0), alert(1, 'unknown'), alert(1, 'toString'), alert(1, 'pintu', { symbol: {} }), alert(1, 'pintu', { created_at: now / 1000 - 301 }),
        alert(1, 'pintu', { created_at: now / 1000 + 61 }), alert(1, 'pintu', { lifecycle_state: 'EXPIRED' }),
        alert(1, 'pintu', { lifecycle_state: 'STALE' }), alert(1, 'pintu', { score_status: 'stale' }),
        alert(1, 'pintu', { score_status: 'not_qualifying' }), alert(1, 'pintu', { data_source: '' }),
        alert(1, 'pintu', { direction: 'BUY' })]) {
        assert.equal(h.controller.notify([bad]), 0);
    }
    assert.equal(h.notices.length, 0);
    assert.equal(h.controller.notify([alert()]), 1);
});

test('unsupported, insecure, denied, and request failures have recovery messages', async () => {
    const unsupported = createBrowserSignalNotifications({ notificationApi: null, secure: true });
    assert.equal((await unsupported.enable()).supported, false);
    assert.match(unsupported.state().message, /unavailable/);
    const insecure = harness('granted', { secure: false });
    assert.match((await insecure.controller.enable()).message, /HTTPS/);
    const denied = harness('denied');
    assert.match((await denied.controller.enable()).message, /browser settings/);
    assert.equal(denied.requests(), 0);
    const failed = harness();
    failed.Notification.requestPermission = async () => { throw Error('unavailable'); };
    assert.match((await failed.controller.enable()).message, /Could not request/);
});

test('test notifications contain no financial setup and failed delivery can be retried', async () => {
    const h = harness('granted');
    assert.equal(h.controller.test(), false);
    await h.controller.enable();
    assert.equal(h.controller.test(), true);
    assert.match(h.notices[0].title, /browser alerts connected/);
    assert.doesNotMatch(h.notices[0].options.body, /BTC|LONG|SHORT/);
    h.notices[0].onerror();
    assert.match(h.controller.state().message, /operating-system/);
    let attempts = 0;
    class FailingNotification {
        static permission = 'granted';
        constructor() { if (++attempts === 1) throw Error('delivery failed'); }
    }
    const failing = createBrowserSignalNotifications({ notificationApi: FailingNotification, secure: true, now: () => now });
    await failing.enable();
    assert.equal(failing.notify([alert()]), 0);
    assert.match(failing.state().message, /could not display/);
    assert.equal(failing.notify([alert()]), 1);
    assert.equal(failing.notify([alert()]), 0);
    assert.doesNotMatch(failing.state().message, /could not display/);
});
