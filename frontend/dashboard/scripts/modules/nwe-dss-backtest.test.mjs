import test from 'node:test';
import assert from 'node:assert/strict';
import { displayProfitFactor, nonEligibleResearchRow } from './nwe-dss-backtest.js';

test('NWE/DSS backtest displays unbounded profit factor as infinity', () => {
    assert.equal(displayProfitFactor(99, true), '∞');
});

test('NWE/DSS backtest keeps finite and unavailable profit factors explicit', () => {
    assert.equal(displayProfitFactor(1.256), '1.26');
    assert.equal(displayProfitFactor(null), '—');
});

test('unverified registered tickers remain pending and are not presented as unavailable market data', () => {
    const row = nonEligibleResearchRow({
        symbol: 'AAVE',
        storage_symbol: 'AAVE-USD',
        research_eligibility: 'pending_verification',
    }, 'pintu', 'crypto_perpetual');

    assert.equal(row.symbol, 'AAVE-USD');
    assert.equal(row.freshness, 'PENDING');
    assert.equal(row.backtest.status, 'PENDING_VERIFICATION');
    assert.match(row.error, /no market-history request was made/);
});

test('eligible research tickers are sent to the research endpoint', () => {
    assert.equal(nonEligibleResearchRow({
        symbol: 'BTC', storage_symbol: 'BTC-USD', research_eligibility: 'eligible',
    }, 'pintu', 'crypto_perpetual'), null);
});

