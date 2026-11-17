import test from 'node:test';
import assert from 'node:assert/strict';
import { installSignalRefresh, validateSignalScreenerPayload } from './signal-refresh.js';
import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';

const validSignal = {
    exchange_id: 'tokocrypto',
    product_id: 'tokocrypto_spot',
    asset_category_id: 'crypto_spot',
    symbol: 'BTC-USD',
    signal_strategy_id: 'tokocrypto_nwe_4h',
    timeframe: '4h',
    direction: 'LONG',
};

const payload = overrides => ({
    signals: [],
    summary: {
        matching_rows: 0,
        offset: 0,
        next_offset: 0,
        limit: 100,
        has_more: false,
        stale: false,
        stale_price_signal_count: 0,
        stale_score_signal_count: 0,
        status: {
            last_attempt_status: 'ready',
            price_data_status: 'unknown',
            price_data_unavailable_count: 0,
        },
    },
    available_timeframes: ['15m', '1h', '4h'],
    ...overrides,
});

test('complete empty screener snapshots are accepted without inventing healthy coverage', () => {
    assert.equal(validateSignalScreenerPayload(payload()).signals.length, 0);
});

test('incomplete summary and price coverage metadata are rejected', () => {
    const missingPageSummary = payload();
    delete missingPageSummary.summary.matching_rows;
    assert.throws(() => validateSignalScreenerPayload(missingPageSummary), /incomplete screener snapshot/);

    const missingPriceStatus = payload();
    delete missingPriceStatus.summary.status.price_data_unavailable_count;
    assert.throws(() => validateSignalScreenerPayload(missingPriceStatus), /incomplete screener snapshot/);
});

test('nonadvancing or inconsistent pagination is rejected instead of looking like the final page', () => {
    assert.throws(() => validateSignalScreenerPayload(payload({
        signals: [validSignal],
        summary: {
            ...payload().summary,
            matching_rows: 2,
            limit: 1,
            has_more: true,
        },
    })), /incomplete screener snapshot/);

    assert.throws(() => validateSignalScreenerPayload(payload({
        signals: [validSignal],
        summary: {
            ...payload().summary,
            matching_rows: 1,
            next_offset: 0,
        },
    })), /incomplete screener snapshot/);
});

test('rows with incomplete stable identity are rejected before replacing the current snapshot', () => {
    const response = payload({
        signals: [{ ...validSignal, product_id: '' }],
        summary: {
            ...payload().summary,
            matching_rows: 1,
            next_offset: 1,
        },
    });
    assert.throws(() => validateSignalScreenerPayload(response), /incomplete screener snapshot/);
});

test('an incomplete response keeps the last good snapshot and exposes a retryable error', async () => {
    const previous = Object.fromEntries(['document', 'fetch', 'DATA'].map(key => [
        key, Object.getOwnPropertyDescriptor(globalThis, key),
    ]));
    const lastGoodSnapshot = { signals: [validSignal], summary: { matching_rows: 1 } };
    let requestError = '';
    globalThis.document = { getElementById: () => null };
    globalThis.DATA = lastGoodSnapshot;
    globalThis.fetch = async () => ({
        ok: true,
        text: async () => JSON.stringify({ signals: [], summary: {} }),
    });
    publishDashboardBindings('response-contract-test', {
        setScreenerFetchError: { get: () => message => { requestError = message; } },
    });
    try {
        installSignalRefresh();
        await getDashboardBinding('fetchFreshSignals')();
        assert.equal(globalThis.DATA, lastGoodSnapshot);
        assert.match(requestError, /incomplete screener snapshot/);
    } finally {
        for (const [key, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, key, descriptor);
            else delete globalThis[key];
        }
    }
});
