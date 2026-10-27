import test from 'node:test';
import assert from 'node:assert/strict';
import { chartDiagnosticsSeries, installChartDataDiagnostics } from './chart-data-diagnostics.js';

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

test('shared reference diagnostics preserve freshness, native failure, and recovery through app switching', t => {
    const elements = new Map([
        'chart-data-diagnostics-summary', 'chart-data-diagnostics-state', 'chart-diagnostics-why',
        'chart-diagnostics-latest', 'chart-diagnostics-age', 'chart-diagnostics-coverage',
        'chart-diagnostics-source', 'chart-diagnostics-attempt', 'chart-diagnostics-success',
        'chart-diagnostics-error', 'chart-diagnostics-run-list', 'chart-diagnostics-retry',
        'chart-diagnostics-sync',
    ].map(id => [id, { textContent: '', disabled: true }]));
    const inputGlobals = {
        document: { getElementById: id => elements.get(id) || null },
        activeTimeframe: '15m',
        CHART_STORAGE_DIAGNOSTICS: { series: [], recent_runs: [] },
        marketSymbolForSignal: () => 'BTC-USD',
    };
    const previous = Object.fromEntries(Object.keys(inputGlobals).map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
    Object.assign(globalThis, inputGlobals);
    t.after(() => {
        for (const [name, descriptor] of Object.entries(previous)) {
            if (descriptor) Object.defineProperty(globalThis, name, descriptor);
            else delete globalThis[name];
        }
    });
    installChartDataDiagnostics();
    const metadata = {
        source: 'shared_crypto_spot_binance_spot_proxy',
        source_label: 'Binance Spot research proxy (USDT quote)',
        data_provider: 'binance', instrument_kind: 'crypto_spot',
        rows: 32, last_ts: 1_700_000_000_000_000_000,
        shared_reference: true, source_is_proxy: true, is_stale: true,
        native_fallback_reason: 'Native provider unreachable',
    };
    globalThis.updateChartDataDiagnostics({ exchange: 'tokocrypto', chart_freshness: { '15m': metadata } });

    assert.equal(elements.get('chart-data-diagnostics-state').textContent, 'STALE');
    assert.match(elements.get('chart-diagnostics-source').textContent, /App exchange: TOKOCRYPTO.*Binance/);
    assert.match(elements.get('chart-diagnostics-source').textContent, /same asset and category share/);
    assert.match(elements.get('chart-diagnostics-source').textContent, /Spot and perpetual candles stay separate/);
    assert.match(elements.get('chart-diagnostics-source').textContent, /do not validate app execution/);
    assert.match(elements.get('chart-diagnostics-why').textContent, /native feed failed: Native provider unreachable/);
    assert.equal(elements.get('chart-diagnostics-retry').disabled, false);
    assert.equal(elements.get('chart-diagnostics-sync').disabled, false);

    globalThis.updateChartDataDiagnostics({ exchange: 'tokocrypto', chart_freshness: { '15m': {
        ...metadata, source: 'tokocrypto_native', source_label: 'Tokocrypto native candles',
        shared_reference: false, source_is_proxy: false, is_stale: false,
        native_fallback_reason: null, data_provider: 'tokocrypto',
    } } });
    assert.equal(elements.get('chart-data-diagnostics-state').textContent, 'READY');
    assert.doesNotMatch(elements.get('chart-diagnostics-source').textContent, /share these provider candles/);
});
