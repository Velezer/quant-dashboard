import test from 'node:test';
import assert from 'node:assert/strict';
import { installSignalScreener, SIGNAL_LIST_PAGE_SIZE } from './signal-screener.js';
import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';

function createScreenerHarness({ exchange = 'stockbit', now = 1_000_000 } = {}) {
    const previous = Object.fromEntries(
        ['document', 'window', 'DATA', 'selectedSignal'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
    );
    const previousNow = Date.now;
    const previousSetTimeout = globalThis.setTimeout;
    const previousClearTimeout = globalThis.clearTimeout;
    const timers = [];
    let epoch = now;
    const setTimeoutStub = (callback, delay = 0) => {
        const timer = { callback, delay, cancelled: false };
        timers.push(timer);
        return timer;
    };
    const clearTimeoutStub = timer => { if (timer) timer.cancelled = true; };
    const elements = new Map();
    const makeElement = id => {
        const classes = new Set();
        return {
            id,
            dataset: {},
            listeners: new Map(),
            classList: {
                add: value => classes.add(value),
                remove: value => classes.delete(value),
                toggle: (value, force) => {
                    if (force === undefined ? !classes.has(value) : force) classes.add(value);
                    else classes.delete(value);
                },
                contains: value => classes.has(value),
            },
            addEventListener(type, callback) { this.listeners.set(type, callback); },
            setAttribute(name, value) { this[name] = value; },
            removeAttribute(name) { delete this[name]; },
            replaceChildren(...children) {
                this.children = children;
                this.firstElementChild = children[0] || null;
            },
            focus() {},
            innerHTML: '',
            textContent: '',
            value: '',
            hidden: false,
            disabled: false,
        };
    };
    Date.now = () => epoch;
    globalThis.setTimeout = setTimeoutStub;
    globalThis.clearTimeout = clearTimeoutStub;
    globalThis.document = {
        body: { classList: makeElement('body').classList },
        addEventListener() {},
        getElementById(id) {
            if (!elements.has(id)) elements.set(id, makeElement(id));
            return elements.get(id);
        },
        querySelectorAll: () => [],
        createElement: makeElement,
    };
    globalThis.window = {
        addEventListener() {},
        setTimeout: setTimeoutStub,
        clearTimeout: clearTimeoutStub,
        dispatchEvent() {},
    };
    globalThis.selectedSignal = null;
    publishDashboardBindings('signal-screener-regression-test', {
        activeExchangeFilter: { get: () => exchange },
        activeProductFilter: { get: () => 'all' },
    });
    installSignalScreener();
    return {
        elements,
        timers,
        setNow(value) { epoch = value; },
        fire(timer) {
            timer.cancelled = true;
            timer.callback();
        },
        fireDelay(delay) {
            for (const timer of timers.filter(item => !item.cancelled && item.delay === delay)) {
                this.fire(timer);
            }
        },
        restore() {
            Date.now = previousNow;
            globalThis.setTimeout = previousSetTimeout;
            globalThis.clearTimeout = previousClearTimeout;
            for (const [key, descriptor] of Object.entries(previous)) {
                if (descriptor) Object.defineProperty(globalThis, key, descriptor);
                else delete globalThis[key];
            }
        },
    };
}

function screenerPayload(exchange, signals, summary) {
    const stockbit = exchange === 'stockbit';
    const product = stockbit ? 'stockbit_regular' : 'pintu_retail_spot';
    const category = stockbit ? 'idx_stock' : 'crypto_spot';
    const strategy = `${exchange}_test_4h`;
    return {
        available_timeframes: ['4h'],
        master_data: {
            signal_score_models: [{
                id: strategy,
                exchange_ids: [exchange],
                product_ids: [product],
                asset_category_ids: [category],
                timeframe: '4h',
                score_scale_max: 10,
                minimum_score_out_of_10: 1,
                minimum_history_bars: 50,
            }],
        },
        signals: signals.map((signal, index) => ({
            exchange_id: exchange,
            product_id: product,
            asset_category_id: category,
            symbol: `${stockbit ? 'TEST' : 'BTC'}${index || ''}`,
            storage_symbol: `${stockbit ? 'TEST' : 'BTC'}${index || ''}`,
            timeframe: '4h',
            direction: 'LONG',
            signal_strategy_id: strategy,
            score: 6,
            setup_strength: 6,
            score_basis: {
                score_model_status: 'current',
                score_scale_max: 10,
                minimum_score_out_of_10: 1,
            },
            persistent_score: stockbit,
            score_status: 'qualifying',
            price_is_fresh: true,
            price_fresh_until_ms: 1_001_000,
            current_price: 100,
            last_updated_at: 1_000,
            ...signal,
        })),
        summary: {
            stale: false,
            generated_at: 1_000,
            matching_rows: signals.length,
            offset: 0,
            next_offset: signals.length,
            limit: SIGNAL_LIST_PAGE_SIZE,
            has_more: false,
            stale_price_signal_count: 0,
            stale_score_signal_count: 0,
            status: {
                last_attempt_status: 'ready',
                last_success_at: 1_000,
                price_data_status: 'ready',
                price_data_unavailable_count: 0,
            },
            ...summary,
        },
    };
}

test('persistent Stockbit score rerenders with a stale no-entry badge at local price expiry', () => {
    const harness = createScreenerHarness();
    try {
        const payload = screenerPayload('stockbit', [{
            persistent_score: true,
            price_fresh_until_ms: 1_001_000,
        }], { stale_price_signal_count: 0 });
        globalThis.DATA = payload;
        getDashboardBinding('applySignalSnapshot')(payload);
        const list = harness.elements.get('signalsList');
        assert.match(list.innerHTML, /market entry now/i);
        assert.doesNotMatch(list.innerHTML, /signal-score-state-stale/);

        const expiryTimer = harness.timers.find(timer => !timer.cancelled && timer.delay === 1_001);
        assert.ok(expiryTimer);
        harness.setNow(1_001_001);
        harness.fire(expiryTimer);

        assert.match(list.innerHTML, /signal-score-state-stale/);
        assert.match(list.innerHTML, /Stale · no market entry/);
        assert.equal(payload.summary.stale_price_signal_count, 1);
    } finally {
        harness.restore();
    }
});

test('local price expiry increments but preserves the server-wide stale count across loaded pages', () => {
    const harness = createScreenerHarness({ exchange: 'pintu' });
    try {
        const payload = screenerPayload('pintu', Array.from({ length: 50 }, (_, index) => ({
            price_fresh_until_ms: index === 0 ? 1_001_000 : 2_000_000,
        })), {
            matching_rows: 120,
            offset: 0,
            next_offset: 50,
            limit: 50,
            has_more: true,
            stale_price_signal_count: 7,
        });
        globalThis.DATA = payload;
        getDashboardBinding('applySignalSnapshot')(payload);
        const expiryTimer = harness.timers.find(timer => !timer.cancelled && timer.delay === 1_001);
        assert.ok(expiryTimer);
        harness.setNow(1_001_001);
        harness.fire(expiryTimer);

        assert.equal(payload.summary.matching_rows, 120);
        assert.equal(payload.summary.stale_price_signal_count, 8);
    } finally {
        harness.restore();
    }
});

test('Clear cancels a pending search debounce before issuing its immediate fetch', () => {
    const harness = createScreenerHarness();
    try {
        const requests = [];
        publishDashboardBindings('signal-refresh-regression-test', {
            fetchFreshSignals: { get: () => () => requests.push(getDashboardBinding('getSignalScreenerQuery')()) },
        });
        const search = harness.elements.get('searchInput');
        const clear = harness.elements.get('clearSearchBtn');
        search.value = 'BT';
        search.listeners.get('input')({ currentTarget: search });
        const debounce = harness.timers.find(timer => !timer.cancelled && timer.delay === 250);
        assert.ok(debounce);

        clear.listeners.get('click')();
        assert.equal(search.value, '');
        assert.equal(debounce.cancelled, true);
        harness.fireDelay(0);
        assert.equal(requests.length, 1);
        assert.equal(requests[0].search, '');

        harness.setNow(1_000_250);
        harness.fireDelay(250);
        assert.equal(requests.length, 1);
    } finally {
        harness.restore();
    }
});
