import test from 'node:test';
import assert from 'node:assert/strict';
import {
    cryptoSignalModelsForSelection,
    stockbitSignalModelsForSelection,
    installSignalScreener,
    signalIdentity,
    signalDisplayScore,
    signalMeetsRenderMinimum,
    signalPriceBarAgeLabel,
    signalPriceCoverageMessage,
    signalRowsForDisplay,
    signalStrategyLabel,
    signalScorePresentation,
    signalScoreComponentWeights,
    signalScoreSummary,
    signalScoreWeightSummary,
    SIGNAL_LIST_PAGE_SIZE,
} from './signal-screener.js';
import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { normalizeSignalRecord, signalScoreState } from './signal-screener-model.js';

test('screener row normalization preserves app identity and product category', () => {
    const strategies = [{ id: 'nwe', short_label: 'NWE' }];
    const idx = normalizeSignalRecord({
        exchange_id: 'STOCKBIT', product_id: 'stockbit_regular', asset_category_id: 'IDX_STOCK',
        symbol: 'BBCA', storage_symbol: 'BBCA.JK', timeframe: '1D', direction: 'long',
        signal_strategy_id: 'nwe', current_price: '9500',
    }, strategies);
    assert.equal(idx.exchange, 'stockbit');
    assert.equal(idx.class, 'idx');
    assert.equal(idx.timeframe, '1d');
    assert.equal(idx.data_symbol, 'BBCA.JK');
    assert.equal(idx.direction, 'LONG');
    assert.equal(idx.signal_strategy_name, 'NWE');
    assert.equal(idx.chart_data_lazy, true);

    assert.equal(normalizeSignalRecord({
        exchange_id: 'tokocrypto', product_id: 'tokocrypto_tokenized_stock',
        asset_category_id: 'tokenized_stock', symbol: 'BBCA-USD',
    }, []).class, 'tokenized_stock');
    assert.equal(normalizeSignalRecord({
        exchange_id: 'pintu', product_id: 'pintu_pro_futures',
        asset_category_id: 'crypto_futures', symbol: 'BTC-USD',
    }, []).class, 'crypto_futures');
});

test('screener signal identity keeps each timeframe independent', () => {
    const base = {
        exchange_id: 'tokocrypto',
        product_id: 'tokocrypto_spot',
        asset_category_id: 'crypto_spot',
        symbol: 'BTC-USD',
        signal_strategy_id: 'tokocrypto_spot_day_trade',
    };

    const fifteenMinute = signalIdentity({ ...base, timeframe: '15m' });
    const hourly = signalIdentity({ ...base, timeframe: '1h' });
    const dssSameFrame = signalIdentity({
        ...base,
        signal_strategy_id: 'tokocrypto_dss_bressert_15m_v2',
        timeframe: '15m',
    });

    assert.notEqual(fifteenMinute, hourly);
    assert.notEqual(fifteenMinute, dssSameFrame);
    assert.equal(fifteenMinute, signalIdentity({ ...base, timeframe: '15m' }));
});

test('screener uses canonical short strategy labels while keeping indicator identities separate', () => {
    const strategies = [
        { id: 'nwe_15m', label: 'Stockbit 15m Nadaraya-Watson Band Proximity · Research', short_label: '15m NWE Band Proximity' },
        { id: 'dss_15m', label: 'Stockbit 15m DSS Bressert Near-Cross · Research', short_label: '15m DSS Near-Cross' },
    ];
    assert.equal(signalStrategyLabel({ signal_strategy_id: 'nwe_15m' }, strategies), '15m NWE Band Proximity');
    assert.equal(signalStrategyLabel({ signal_strategy_id: 'dss_15m' }, strategies), '15m DSS Near-Cross');
});

test('screener price age converts the persisted nanosecond bar timestamp', () => {
    const nowEpochMs = Date.UTC(2026, 9, 2, 10, 59);
    const priceBarTimeNs = (nowEpochMs - 29 * 60 * 1000) * 1_000_000;

    assert.equal(signalPriceBarAgeLabel(priceBarTimeNs, nowEpochMs), '29m ago');
    assert.equal(signalPriceBarAgeLabel(null, nowEpochMs), 'age unavailable');
});

test('screener reports partial price coverage across the full scan universe', () => {
    assert.equal(
        signalPriceCoverageMessage('partial_stale', 571),
        'Fresh 15m price data was unavailable for 571 registered instruments in the full scan universe.',
    );
    assert.equal(
        signalPriceCoverageMessage('unavailable', 573),
        'No fresh 15m price data was available for any of 573 registered instruments in the full scan universe.',
    );
    assert.equal(signalPriceCoverageMessage('ready', 0), '');
    assert.equal(signalPriceCoverageMessage('partial_stale', NaN), '');
});

test('screener initially renders a bounded top-ranked page without mutating the full snapshot', () => {
    const signals = Array.from({ length: SIGNAL_LIST_PAGE_SIZE + 10 }, (_, index) => ({ rank: index + 1 }));
    const visible = signalRowsForDisplay(signals);

    assert.equal(visible.length, SIGNAL_LIST_PAGE_SIZE);
    assert.equal(visible[0].rank, 1);
    assert.equal(visible.at(-1).rank, SIGNAL_LIST_PAGE_SIZE);
    assert.equal(signals.length, SIGNAL_LIST_PAGE_SIZE + 10);
    assert.equal(signalRowsForDisplay(signals, SIGNAL_LIST_PAGE_SIZE + 10).length, signals.length);
});

test('crypto model empty state remains scoped when the user selects a crypto category', () => {
    const models = [
        {
            id: 'crypto_nwe',
            exchange_ids: ['tokocrypto'],
            product_ids: ['pintu_retail_spot'],
            asset_category_ids: ['crypto_spot', 'crypto_perpetual'],
            timeframe: '4h',
            score_scale_max: 10,
            minimum_score_out_of_10: 5,
            minimum_history_bars: 501,
        },
        {
            id: 'crypto_dss_20',
            exchange_ids: ['tokocrypto'],
            product_ids: ['pintu_retail_spot'],
            asset_category_ids: ['crypto_spot'],
            timeframe: '1h',
            score_scale_max: 20,
            minimum_score_out_of_20: 0.5,
            component_score_scale_max: 10,
            minimum_history_bars: 500,
        },
        {
            id: 'tokenized_stock',
            exchange_ids: ['tokocrypto'],
            product_ids: ['tokocrypto_tokenized_stock'],
            asset_category_ids: ['tokenized_stock'],
            timeframe: '4h',
            score_scale_max: 10,
            minimum_score_out_of_10: 5,
            minimum_history_bars: 50,
        },
    ];

    assert.deepEqual(
        cryptoSignalModelsForSelection(models, 'tokocrypto', 'crypto_spot').map(model => model.id),
        ['crypto_nwe', 'crypto_dss_20'],
    );
    assert.deepEqual(
        cryptoSignalModelsForSelection(models, 'tokocrypto', 'tokenized_stock'),
        [],
    );
});

test('Stockbit model selection respects app, product, category and timeframe filters', () => {
    const model = {
        id: 'stockbit_nwe_1d',
        exchange_ids: ['stockbit'],
        product_ids: ['stockbit_regular'],
        asset_category_ids: ['idx_stock'],
        timeframe: '1d',
        score_scale_max: 10,
        minimum_score_out_of_10: 5,
        minimum_history_bars: 50,
    };
    const models = [
        model,
        { ...model, id: 'wrong_exchange', exchange_ids: ['tokocrypto'] },
        { ...model, id: 'wrong_product', product_ids: ['stockbit_margin'] },
        { ...model, id: 'wrong_timeframe', timeframe: '4h' },
        { ...model, id: 'missing_minimum_history', minimum_history_bars: undefined },
    ];
    assert.deepEqual(
        stockbitSignalModelsForSelection(models, 'stockbit', 'all', '1d').map(item => item.id),
        ['stockbit_nwe_1d'],
    );
    assert.deepEqual(stockbitSignalModelsForSelection(models, 'tokocrypto', 'all'), []);
});

test('Stockbit empty state keeps scan-wide partial price coverage visible and retryable', () => {
    const previousDocument = globalThis.document;
    const elements = new Map();
    const classTokens = () => {
        const values = new Set();
        return {
            add: value => values.add(value),
            remove: value => values.delete(value),
            toggle: (value, force) => {
                if (force === undefined ? !values.has(value) : force) values.add(value);
                else values.delete(value);
            },
            contains: value => values.has(value),
        };
    };
    const makeElement = id => ({
        id,
        classList: classTokens(),
        addEventListener() {},
        setAttribute() {},
        textContent: '',
        innerHTML: '',
        hidden: id === 'screener-retry',
    });
    for (const id of [
        'screener-status-text', 'screener-freshness', 'screener-retry',
        'screener-status', 'kpi-grid', 'signal-count-badge',
        'clearSearchBtn', 'signalsList',
    ]) elements.set(id, makeElement(id));
    globalThis.document = {
        getElementById: id => elements.get(id) || null,
        querySelectorAll: () => [],
    };
    publishDashboardBindings('signal-screener-test', {
        activeExchangeFilter: { get: () => 'stockbit' },
        activeProductFilter: { get: () => 'all' },
    });
    try {
        installSignalScreener();
        getDashboardBinding('applySignalSnapshot')({
            available_timeframes: ['15m'],
            master_data: {
                signal_score_models: [{
                    id: 'stockbit_nwe_15m',
                    exchange_ids: ['stockbit'],
                    product_ids: ['stockbit_regular'],
                    asset_category_ids: ['idx_stock'],
                    timeframe: '15m',
                    score_scale_max: 20,
                    minimum_score_out_of_20: 1,
                    minimum_history_bars: 50,
                }],
            },
            signals: [],
            summary: {
                generated_at: 1_791_893_169,
                stale: false,
                status: {
                    last_attempt_status: 'ready',
                    last_success_at: 1_791_893_169,
                    price_data_status: 'partial_stale',
                    price_data_unavailable_count: 571,
                },
            },
        });

        assert.match(elements.get('screener-status-text').textContent, /571 registered instruments in the full scan universe/);
        assert.equal(elements.get('screener-retry').classList.contains('hidden'), false);
        assert.match(elements.get('signalsList').innerHTML, /571 registered instruments in the full scan universe/);
    } finally {
        globalThis.document = previousDocument;
    }
});


test('score presentation keeps retained legacy rows on /10 and current models on /40', () => {
    const currentModel = {
        score_scale_max: 40,
        minimum_score_out_of_40: 1,
        component_score_scale_max: 20,
        indicator_component_score_scale_max: 10,
    };
    assert.deepEqual(
        signalScorePresentation({
            score: 7,
            score_basis: {
                model_id: 'nwe_endpoint_band_proximity_dss_rank_v3',
                score_scale_max: 10,
                minimum_score_out_of_10: 5,
                component_score_scale_max: 10,
                score_model_status: 'legacy',
            },
        }, currentModel),
        {
            scoreScaleMax: 10,
            minimumScore: 5,
            componentScoreScaleMax: 10,
            indicatorComponentScoreScaleMax: 10,
            legacy: true,
        },
    );
    assert.deepEqual(
        signalScorePresentation({
            score: 14,
            signal_strategy_id: 'stockbit_nwe_band_proximity_15m_v1',
            score_basis: {
                model_id: 'nwe_endpoint_band_proximity_dss_rank_v4',
                score_scale_max: 20,
                minimum_score_out_of_20: 5,
                component_score_scale_max: 10,
                score_model_status: 'legacy',
            },
        }, currentModel),
        {
            scoreScaleMax: 20,
            minimumScore: 5,
            componentScoreScaleMax: 10,
            indicatorComponentScoreScaleMax: 10,
            legacy: true,
        },
    );
});

test('score component weights are shown from the active model or legacy row basis', () => {
    const currentModel = {
        score_scale_max: 20,
        component_score_scale_max: 10,
    };
    assert.deepEqual(
        signalScoreComponentWeights({ score_basis: { score_scale_max: 20 } }, currentModel),
        { strategy: 0.5, confluence: 0.5 },
    );
    assert.deepEqual(
        signalScoreComponentWeights({
            score_basis: {
                score_scale_max: 10,
                strategy_weight: 0.7,
                confluence_weight: 0.3,
            },
        }, currentModel),
        { strategy: 0.7, confluence: 0.3 },
    );
    assert.deepEqual(
        signalScoreComponentWeights({ score_basis: { score_scale_max: 10 } }, currentModel),
        { strategy: null, confluence: null },
    );
    assert.equal(
        signalScoreWeightSummary({ score_basis: { score_scale_max: 20 } }, {
            ...currentModel,
            strategy_score_label: 'NWE strategy',
            confluence_score_label: 'DSS confluence',
        }),
        'NWE strategy 50%, DSS confluence 50%',
    );
});

test('four-factor model uses /40 total, /20 groups, and /10 individual factors', () => {
    assert.deepEqual(
        signalScorePresentation({
            score_basis: {
                score_scale_max: 40,
                component_score_scale_max: 20,
                indicator_component_score_scale_max: 10,
                score_model_status: 'current',
            },
        }, {
            score_scale_max: 40,
            component_score_scale_max: 20,
            indicator_component_score_scale_max: 10,
            minimum_score_out_of_40: 1,
        }),
        {
            scoreScaleMax: 40,
            minimumScore: 1,
            componentScoreScaleMax: 20,
            indicatorComponentScoreScaleMax: 10,
            legacy: false,
        },
    );
});

test('clearing a filtered-out selection resets signal-specific chart context', () => {
    const previousDocument = globalThis.document;
    const previousWindow = globalThis.window;
    const previousSelectedSignal = globalThis.selectedSignal;
    const elements = new Map();
    const makeElement = id => ({
        id,
        className: '',
        classList: {
            add() {},
            remove() {},
            toggle() {},
        },
        dataset: {},
        addEventListener() {},
        setAttribute(name, value) { this[name] = value; },
        removeAttribute(name) { delete this[name]; },
        textContent: 'old selection',
        innerText: 'old selection',
        innerHTML: '',
    });
    const documentStub = {
        body: { classList: { add() {}, remove() {}, toggle() {} } },
        addEventListener() {},
        getElementById(id) {
            if (!elements.has(id)) elements.set(id, makeElement(id));
            return elements.get(id);
        },
        querySelectorAll() { return []; },
    };
    globalThis.document = documentStub;
    globalThis.window = { dispatchEvent() {} };
    globalThis.selectedSignal = { signal_id: 'stockbit:BEKS:1w', symbol: 'BEKS' };

    let resetOHLCSignal = 'not-called';
    let warningSignal = 'not-called';
    let regimeSignal = 'not-called';
    let chartStatus = null;
    publishDashboardBindings('signal-screener-clear-test', {
        resetOHLCBarForSignal: { get: () => signal => { resetOHLCSignal = signal; } },
        updateChartDataWarning: { get: () => signal => { warningSignal = signal; } },
        updateMarkovRegimeStatus: { get: () => signal => { regimeSignal = signal; } },
        setChartStatus: { get: () => (state, detail) => { chartStatus = { state, detail }; } },
    });

    try {
        installSignalScreener();
        getDashboardBinding('clearSelectedSignalChart')('The selected signal is outside the current screener filters.');

        assert.equal(globalThis.selectedSignal, null);
        assert.equal(elements.get('chart-timeframe-context').textContent, '');
        assert.equal(Object.hasOwn(elements.get('chart-timeframe-context'), 'title'), false);
        assert.equal(resetOHLCSignal, null);
        assert.equal(warningSignal, null);
        assert.equal(regimeSignal, null);
        assert.equal(chartStatus.state, 'no_selection');
    } finally {
        globalThis.document = previousDocument;
        if (previousWindow === undefined) delete globalThis.window;
        else globalThis.window = previousWindow;
        if (previousSelectedSignal === undefined) delete globalThis.selectedSignal;
        else globalThis.selectedSignal = previousSelectedSignal;
    }
});

test('selected signal summary stays concise and preserves the score scale', () => {
    const currentModel = {
        score_scale_max: 20,
        minimum_score_out_of_20: 1,
        component_score_scale_max: 10,
    };
    const currentSignal = {
        score: 9.6,
        score_basis: {
            score_scale_max: 20,
            minimum_score_out_of_20: 1,
        },
    };
    assert.equal(
        signalScoreSummary(currentSignal, currentModel, { label: 'CLOSED · 1H' }),
        'Market-entry setup score 9.6/20 · closed · Strategy 50%, Confluence 50% · setup strength, not win probability or execution availability',
    );
    assert.equal(
        signalScoreSummary(currentSignal, currentModel, { label: 'PROVISIONAL · 1H' }),
        'Market-entry setup score 9.6/20 · forming · Strategy 50%, Confluence 50% · setup strength, not win probability or execution availability',
    );

    const legacySignal = {
        score: 7.4,
        score_basis: {
            score_scale_max: 10,
            minimum_score_out_of_10: 5,
            score_model_status: 'legacy',
            strategy_weight: 0.7,
            confluence_weight: 0.3,
        },
    };
    assert.equal(
        signalScoreSummary(legacySignal, currentModel),
        'Market-entry setup score 7.4/10 · Strategy 70%, Confluence 30% · setup strength, not win probability or execution availability',
    );
});

test('retained scores remain visible while stale and unavailable values never become zero', () => {
    const model = {
        score_scale_max: 20,
        component_score_scale_max: 10,
        strategy_weight: 0.5,
        confluence_weight: 0.5,
    };
    const staleSignal = { score: 12, persistent_score: true, score_status: 'stale' };
    assert.equal(signalDisplayScore(staleSignal), 12);
    assert.equal(
        signalScoreSummary(staleSignal, model, signalScoreState(staleSignal)),
        'Last recorded setup score 12.0/20 · stale · no market entry · held · Strategy 50%, Confluence 50% · setup strength, not win probability or execution availability',
    );
    const nonQualifyingSignal = { ...staleSignal, score: 0, score_status: 'not_qualifying' };
    assert.equal(signalDisplayScore(nonQualifyingSignal), 0);
    assert.match(
        signalScoreSummary(nonQualifyingSignal, model, signalScoreState(nonQualifyingSignal)),
        /^Market-entry setup score 0\.0\/20 · no market entry · score 0/,
    );
    assert.equal(signalDisplayScore({ score: 7.4, persistent_score: true, score_status: 'current' }), 7.4);
    for (const score of [null, undefined, '', ' ', NaN, 'invalid', true, {}, []]) {
        const unavailable = { ...staleSignal, score };
        assert.equal(signalDisplayScore(unavailable), null);
        assert.match(
            signalScoreSummary(unavailable, model, signalScoreState(unavailable)),
            /^Last recorded setup score unavailable/,
        );
    }
});

test('screener hides retained scores below the model render minimum', () => {
    const combinedModel = {
        score_scale_max: 20,
        minimum_score_out_of_20: 1,
        component_score_scale_max: 10,
    };
    const basis = {
        score_scale_max: 20,
        minimum_score_out_of_20: 1,
        score_model_status: 'current',
    };

    assert.equal(signalMeetsRenderMinimum({ score: 0, score_basis: basis }, combinedModel), false);
    assert.equal(signalMeetsRenderMinimum({ score: 1, score_basis: basis }, combinedModel), true);
    assert.equal(signalMeetsRenderMinimum({ score: 0.99, score_basis: basis }, combinedModel), false);
});

test('Retry queues a scan for stale, failed and waiting snapshots, and rereads fresh snapshots', async () => {
    const previousDocument = globalThis.document;
    let retryClick;
    let scans = 0;
    let reads = 0;
    globalThis.document = {
        getElementById: id => id === 'screener-retry'
            ? { addEventListener: (_, callback) => { retryClick = callback; } }
            : null,
        querySelectorAll: () => [],
    };
    publishDashboardBindings('signal-refresh', {
        triggerManualRefresh: { get: () => async () => { scans++; } },
        fetchFreshSignals: { get: () => async () => { reads++; } },
    });
    try {
        installSignalScreener();
        for (const summary of [
            { stale: true, status: { last_attempt_status: 'ready' } },
            { stale: false, status: { last_attempt_status: 'error' } },
            { stale: false, status: { last_attempt_status: 'waiting' } },
        ]) {
            getDashboardBinding('applySignalSnapshot')({ signals: [], summary });
            retryClick();
        }
        assert.equal(scans, 3);
        assert.equal(reads, 0);
        getDashboardBinding('applySignalSnapshot')({
            signals: [], summary: { stale: false, status: { last_attempt_status: 'ready' } },
        });
        getDashboardBinding('setScreenerFetchError')('Connection interrupted');
        retryClick();
        assert.equal(scans, 3);
        assert.equal(reads, 1);
    } finally {
        globalThis.document = previousDocument;
    }
});
