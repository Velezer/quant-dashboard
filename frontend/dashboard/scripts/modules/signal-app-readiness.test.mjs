import test from 'node:test';
import assert from 'node:assert/strict';
import { appSignalReadiness } from './signal-app-readiness.js';

test('every app reports its own listing, verification, and history coverage', () => {
    const apps = ['stockbit', 'pintu', 'pluang', 'tokocrypto'];
    const products = apps.map((exchange_id, i) => ({ exchange_id, registered: 10 + i, confirmed: 8 + i, with_15m_candles: 5 + i }));
    for (const [i, app] of apps.entries()) {
        const result = appSignalReadiness(app, { products });
        assert.match(result.message, new RegExp(`${8 + i} verified of ${10 + i} registered.*${5 + i} with stored`));
        assert.match(result.message, /Stored candles alone do not confirm fresh prices/);
        assert.equal(result.hasScanDetails, false);
        assert.equal(result.retryListings, false);
    }
});

test('listing failures, unverified listings, missing history and stale saved signals have distinct recovery', () => {
    const blocked = appSignalReadiness('tokocrypto', { products: [], refresh_errors: { tokocrypto: 'Public provider HTTP 451' } });
    assert.match(blocked.message, /listing refresh failed.*451/);
    assert.equal(blocked.retryListings, true);
    const pending = appSignalReadiness('pintu', { products: [{ exchange_id: 'pintu', registered: 10, confirmed: 0 }] });
    assert.match(pending.message, /pending verification/);
    assert.equal(pending.retryListings, true);
    const products = [{ exchange_id: 'stockbit', registered: 10, confirmed: 10, with_15m_candles: 0 }];
    assert.match(appSignalReadiness('stockbit', { products }).message, /history is being gathered/);
    products[0].with_15m_candles = 10;
    assert.match(appSignalReadiness('stockbit', { products }, { stale_price_signal_count: 7 }).message, /7 saved signals.*stale/);
    assert.equal(appSignalReadiness('pluang', { products: [] }).retryListings, false);
});

test('one app never inherits another app’s provider error or coverage', () => {
    const result = appSignalReadiness('pintu', { products: [{ exchange_id: 'pluang', registered: 999 }], refresh_errors: { tokocrypto: 'HTTP 451' } });
    assert.match(result.message, /Pintu has no detailed scan diagnostics/);
    assert.doesNotMatch(result.message, /451|999/);
});

const frame = (timeframe, extra = {}) => ({ timeframe, evaluated_count: 0, qualifying_signal_count: 0, ...extra });
const product = (exchange_id, product_id, extra = {}) => ({ exchange_id, product_id,
    instrument_count: 10, fresh_price_count: 0, missing_price_count: 0, stale_price_count: 0,
    data_error_count: 0, examples: [], timeframes: [frame('15m')], ...extra });
const snapshot = products => ({ stale: false, status: { last_attempt_status: 'ready' },
    readiness: { version: 1, checked_at: 1792737300, products } });

test('each app receives its observed blocker rather than scan-wide coverage from another app', () => {
    const summary = snapshot([
        product('pintu', 'pintu_pro_futures', { stale_price_count: 10 }),
        product('pluang', 'pluang_spot', { instrument_count: 7, missing_price_count: 7 }),
        product('tokocrypto', 'tokocrypto_spot', { fresh_price_count: 10, timeframes: [frame('15m', { evaluated_count: 10 })] }),
    ]);
    assert.match(appSignalReadiness('pintu', null, summary).message, /10 stale 15m prices/);
    assert.doesNotMatch(appSignalReadiness('pintu', null, summary).message, /7 missing/);
    assert.match(appSignalReadiness('pluang', null, summary).message, /7 missing 15m prices/);
    const healthy = appSignalReadiness('tokocrypto', null, summary);
    assert.equal(healthy.title, 'No qualifying setup in the last scan');
    assert.equal(healthy.tone, 'neutral');
    assert.match(healthy.checkedAt, /WIB.*when each ticker was scanned/);
});

test('product and timeframe scope preserve insufficient history, source mismatch, and unreadable checks', () => {
    const summary = snapshot([
        product('tokocrypto', 'tokocrypto_spot', { fresh_price_count: 10, timeframes: [
            frame('15m', { evaluated_count: 10 }), frame('4h', { insufficient_history_count: 10 }),
        ] }),
        product('tokocrypto', 'tokocrypto_tokenized_stock', { fresh_price_count: 10,
            timeframes: [frame('15m', { source_mismatch_count: 10 })] }),
    ]);
    const scope = { product: 'tokocrypto_spot', timeframe: '4h' };
    assert.equal(appSignalReadiness('tokocrypto', null, summary, scope).title, 'Not enough usable model history');
    assert.match(appSignalReadiness('tokocrypto', null, summary, { product: 'tokocrypto_tokenized_stock' }).message, /10 price\/history source mismatches/);
    summary.readiness.products[0].timeframes[0].data_error_count = 2;
    const partial = appSignalReadiness('tokocrypto', null, summary, { product: 'tokocrypto_spot', timeframe: '15m' });
    assert.equal(partial.title, 'Some tickers still need usable data');
    assert.match(partial.message, /2 unreadable timeframe histories/);
});

test('stale, failed, missing diagnostics and hidden stale-price rows remain distinct', () => {
    const summary = snapshot([product('pintu', 'pintu_pro_spot', { fresh_price_count: 10, timeframes: [frame('15m', { evaluated_count: 10 })] })]);
    assert.match(appSignalReadiness('pintu', null, { ...summary, stale: true }).title, /outdated/);
    assert.match(appSignalReadiness('pintu', null, { ...summary, status: { last_attempt_status: 'error', last_error: 'database is locked' } }).message, /database is locked/);
    assert.match(appSignalReadiness('pintu', null, { ...summary, stale_price_signal_count: 3 }).title, /hidden by stale prices/);
    assert.match(appSignalReadiness('pintu', null, { status: { last_attempt_status: 'waiting' } }).title, /first signal scan/);
    const absent = appSignalReadiness('pintu', null, {});
    assert.equal(absent.hasScanDetails, false);
    assert.doesNotMatch(absent.title, /No qualifying setup/);
});

test('NWE warm-up gaps prevent a healthy all-models claim and search does not change scan counts', () => {
    const summary = snapshot([product('pluang', 'pluang_perpetual', { fresh_price_count: 10,
        timeframes: [frame('15m', { evaluated_count: 10, nwe_insufficient_history_count: 8 })] })]);
    assert.equal(appSignalReadiness('pluang', null, summary).title, 'Some tickers still need usable data');
    const search = appSignalReadiness('pluang', null, summary, { search: 'BTC', visibleCount: 0 });
    assert.equal(search.title, 'No current signals match your search');
    assert.match(search.message, /search filters the returned signal rows only/);
    assert.equal(search.metrics[0].value, '10');
});
