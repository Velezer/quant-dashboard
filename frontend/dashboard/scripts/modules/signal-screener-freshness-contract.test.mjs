import test from 'node:test';
import assert from 'node:assert/strict';
import {
    normalizeSignalRecord,
    signalPriceCoverageMessage,
    signalPriceIsFresh,
    signalScoreState,
} from './signal-screener-model.js';

const row = overrides => ({
    direction: 'LONG',
    timeframe: '4h',
    price_is_fresh: true,
    price_fresh_until_ms: 2_000,
    ...overrides,
});

test('missing freshness evidence is unavailable, never a current price', () => {
    assert.equal(signalPriceIsFresh({ price_is_fresh: true }, 1_000), false);
    assert.equal(signalPriceIsFresh({ price_fresh_until_ms: null }, 1_000), false);
    assert.equal(signalPriceIsFresh(row({}), 1_000), true);
});

test('persistent score rows expire locally when their price freshness window ends', () => {
    const signal = row({ persistent_score: true, score_status: 'qualifying' });
    assert.equal(signalScoreState(signal, 1_500).current, true);
    assert.deepEqual(signalScoreState(signal, 2_001), {
        label: 'STALE · NO MARKET ENTRY · HELD', tone: 'stale', current: false,
    });
});

test('explicit no-market-entry status is honored for nonpersistent scores', () => {
    assert.deepEqual(signalScoreState(row({ market_entry_status: 'NO_MARKET_ENTRY' }), 1_000), {
        label: 'LATEST 4H · NO MARKET ENTRY', tone: 'inactive', current: false,
    });
});

test('a closed-market persistent reference remains a no-entry reference after its price expires', () => {
    const state = signalScoreState(row({
        persistent_score: true,
        score_status: 'current',
        market_entry_status: 'NO_MARKET_ENTRY',
        price_is_fresh: false,
    }), 2_001);
    assert.deepEqual(state, {
        label: 'LATEST CLOSED 4H · NO MARKET ENTRY', tone: 'inactive', current: false,
    });
});

test('missing, blank, and nonpositive prices stay unavailable instead of becoming zero', () => {
    const raw = { exchange_id: 'pintu', symbol: 'BTC-USD', timeframe: '4h' };
    assert.equal(normalizeSignalRecord({ ...raw, current_price: null }).current_price, null);
    assert.equal(normalizeSignalRecord({ ...raw, current_price: '' }).current_price, null);
    assert.equal(normalizeSignalRecord({ ...raw, current_price: 0 }).current_price, null);
    assert.equal(normalizeSignalRecord({ ...raw, current_price: '123.45' }).current_price, 123.45);
});

test('missing coverage status and count stay unknown instead of reporting zero', () => {
    assert.match(signalPriceCoverageMessage(undefined, 0), /coverage is not reported/);
    assert.match(signalPriceCoverageMessage('partial_stale', null), /number of affected instruments was not reported/);
});
