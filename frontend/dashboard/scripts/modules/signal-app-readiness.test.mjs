import test from 'node:test';
import assert from 'node:assert/strict';
import { appSignalReadiness } from './signal-app-readiness.js';

test('every app reports its own listing, verification, and history coverage', () => {
    const apps = ['stockbit', 'pintu', 'pluang', 'tokocrypto'];
    const products = apps.map((exchange_id, i) => ({ exchange_id, registered: 10 + i, confirmed: 8 + i, with_15m_candles: 5 + i }));
    for (const [i, app] of apps.entries()) {
        const result = appSignalReadiness(app, { products });
        assert.match(result.message, new RegExp(`${8 + i} verified of ${10 + i} registered.*${5 + i} with stored`));
        assert.match(result.message, /fresh, qualifying setup/);
        assert.equal(result.retryListings, false);
    }
});

test('listing failures, unverified listings, missing history and stale saved signals have distinct recovery', () => {
    const blocked = appSignalReadiness('tokocrypto', { products: [], refresh_errors: { tokocrypto: 'Public provider HTTP 451' } });
    assert.match(blocked.message, /provider failed.*451.*No verified listings/);
    assert.equal(blocked.retryListings, true);
    const pending = appSignalReadiness('pintu', { products: [{ exchange_id: 'pintu', registered: 10, confirmed: 0 }] });
    assert.match(pending.message, /pending verification/);
    assert.equal(pending.retryListings, true);
    const products = [{ exchange_id: 'stockbit', registered: 10, confirmed: 10, with_15m_candles: 0 }];
    assert.match(appSignalReadiness('stockbit', { products }).message, /history is being gathered/);
    products[0].with_15m_candles = 10;
    assert.match(appSignalReadiness('stockbit', { products }, { stale_price_signal_count: 7 }).message, /7 saved signals.*stale.*market closure/);
    assert.equal(appSignalReadiness('pluang', { products: [] }).retryListings, false);
});

test('one app never inherits another app’s provider error or coverage', () => {
    const result = appSignalReadiness('pintu', { products: [{ exchange_id: 'pluang', registered: 999 }], refresh_errors: { tokocrypto: 'HTTP 451' } });
    assert.match(result.message, /0 verified of 0/);
    assert.doesNotMatch(result.message, /451|999/);
});
