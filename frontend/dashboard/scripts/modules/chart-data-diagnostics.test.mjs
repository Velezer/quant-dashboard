import test from 'node:test';
import assert from 'node:assert/strict';
import { chartDiagnosticsSeries } from './chart-data-diagnostics.js';

const pintuFutureFailure = {
    symbol: 'ZRX-USD',
    interval: '1h',
    source: 'pintu_futures_binance_usdm_futures_proxy',
    exchange_id: 'pintu',
    last_error: 'market-data request failed',
};
const pintuRetailSpot = {
    symbol: 'ZRX-USD',
    interval: '1h',
    source: 'pintu_retail_spot_binance_spot_proxy',
    exchange_id: 'pintu',
    last_error: null,
};

test('chart diagnostics select the exact chart source for a repeated ticker', () => {
    const selected = chartDiagnosticsSeries(
        [pintuFutureFailure, pintuRetailSpot],
        'zrx-usd',
        '1H',
        'pintu_retail_spot_binance_spot_proxy',
        'pintu',
    );

    assert.equal(selected, pintuRetailSpot);
    assert.equal(selected.last_error, null);
});

test('chart diagnostics do not borrow another source error when the chart source is absent', () => {
    const selected = chartDiagnosticsSeries(
        [pintuFutureFailure],
        'ZRX-USD',
        '1h',
        'pintu_retail_spot_binance_spot_proxy',
        'pintu',
    );

    assert.equal(selected, null);
});
