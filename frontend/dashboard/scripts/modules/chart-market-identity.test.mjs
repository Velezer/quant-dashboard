import test from 'node:test';
import assert from 'node:assert/strict';
import { marketSymbolForSignal, marketVenueForSignal } from './chart-market-identity.js';

test('market venue resolution honors exchange metadata before asset class', () => {
    assert.equal(marketVenueForSignal({ exchange: 'Tokocrypto', class: 'tokenized_stock' }), 'tokocrypto');
    assert.equal(marketVenueForSignal({ Exchange: 'Stockbit', class: 'crypto' }), 'idx');
    assert.equal(marketVenueForSignal({ venue: 'Pintu' }), 'pintu');
    assert.equal(marketVenueForSignal({ class: 'tokenized asset' }), 'tokocrypto');
    assert.equal(marketVenueForSignal(undefined), 'app');
});

test('market symbols preserve venue-specific symbol forms', () => {
    assert.equal(marketSymbolForSignal({ exchange: 'Stockbit', asset: 'bbca' }), 'BBCA.JK');
    assert.equal(marketSymbolForSignal({ exchange: 'Tokocrypto', asset: 'BTC', class: 'crypto' }), 'BTC-USD');
    assert.equal(marketSymbolForSignal({ exchange: 'Tokocrypto', asset: 'BBCA-USD', class: 'tokenized_stock' }), 'BBCA');
    assert.equal(marketSymbolForSignal({ exchange: 'Pluang', asset: 'AAPL' }), 'AAPL-USD');
    assert.equal(marketSymbolForSignal({ exchange: 'Pluang', asset: 'BBCA', class: 'IDX stock' }), 'BBCA.JK');
    assert.equal(marketSymbolForSignal({ exchange: 'app', asset: 'ETH_SPOT' }), 'ETH-USD');
});
