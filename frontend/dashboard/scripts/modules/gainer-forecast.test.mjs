import test from 'node:test';
import assert from 'node:assert/strict';
import { initializeGainerForecast, normalizeGainerForecast } from './gainer-forecast.js';

const SCOPES = {
    tokocrypto_spot: {
        protocol_id: 'tokocrypto_native_momentum_top_rank_next_24h_return_forecast_v2',
        target_kind: 'calendar_24h',
        forecast_target: 'Next 24h close-to-close return (%)',
        forecast_horizon_seconds: 86_400,
        source_family: 'tokocrypto_native',
        data_provider: 'tokocrypto',
        price_basis: 'venue_native',
        scan_interval_seconds: 14_400,
    },
    stockbit_idx: {
        scope_id: 'stockbit_idx',
        protocol_id: 'stockbit_yahoo_momentum_top_rank_next_idx_session_return_forecast_v2',
        target_kind: 'next_idx_session',
        forecast_target: 'Next available IDX session close-to-close return (%)',
        forecast_horizon_seconds: null,
        source_family: 'stockbit_regular_yfinance_fallback',
        data_provider: 'yfinance',
        price_basis: 'yahoo_unadjusted',
        scan_interval_seconds: 86_400,
    },
};

function schedule(scopeId = 'tokocrypto_spot', overrides = {}) {
    const scope = SCOPES[scopeId];
    return {
        status: 'scheduled',
        attempt_count: 0,
        max_attempts_per_window: 15,
        scan_interval_seconds: scope.scan_interval_seconds,
        last_attempt_at: null,
        next_scan_at: 1_800_000_300,
        last_error: null,
        forecast_cutoff: 1_800_000_000,
        scope_id: scopeId,
        target_window_passed: false,
        ...overrides,
    };
}

function forecast(overrides = {}, scopeId = 'tokocrypto_spot') {
    const scope = SCOPES[scopeId];
    return {
        scope_id: scopeId,
        status: 'INSUFFICIENT_COVERAGE',
        as_of: 1_800_000_000,
        forecast_until: scopeId === 'tokocrypto_spot' ? 1_800_086_400 : null,
        eligible_count: 20,
        forecasted_count: 10,
        minimum_forecasted_count: 16,
        missing_or_stale_count: 10,
        invalid_history_count: 0,
        read_error_count: 0,
        ...scope,
        model: scopeId === 'tokocrypto_spot'
            ? 'Naive 24h momentum extrapolation'
            : 'Naive next-session momentum extrapolation',
        model_description: 'Momentum baseline; unvalidated research.',
        validation_status: 'UNVALIDATED_RESEARCH',
        candidates: [],
        provisional_candidate: null,
        schedule: schedule(scopeId, {
            status: 'waiting_for_coverage',
            attempt_count: 2,
            next_scan_at: 1_800_001_000,
        }),
        observed_outcomes: {
            sample_count: 0,
            mean_realized_return_pct: null,
            positive_return_rate_pct: null,
            latest_settled_at: null,
            status: 'PROSPECTIVE_RESEARCH',
        },
        ...overrides,
    };
}

function candidate(scopeId, overrides = {}) {
    const scope = SCOPES[scopeId];
    return {
        symbol: scopeId === 'tokocrypto_spot' ? 'BTCIDR' : 'BBCA',
        venue_symbol: scopeId === 'tokocrypto_spot' ? 'BTC/IDR' : 'BBCA.JK',
        source_family: scope.source_family,
        data_provider: scope.data_provider,
        price_basis: scope.price_basis,
        forecast_return_pct: 8.25,
        quote_currency: 'IDR',
        instrument_id: scopeId === 'tokocrypto_spot' ? 'tokocrypto:spot:btcidr' : 'stockbit.stockbit_regular.bbca',
        reference_bar_time: 1_800_000_000,
        reference_close: 100,
        ...overrides,
    };
}

test('scheduled response gives each profile a scan time without fabricating a candidate', () => {
    const payload = {
        ...forecast({}, 'tokocrypto_spot'),
        status: 'SCHEDULED',
        candidates: [],
        schedule: schedule(),
    };
    const normalized = normalizeGainerForecast(payload);
    assert.equal(normalized.status, 'SCHEDULED');
    assert.deepEqual(normalized.candidates, []);
    assert.equal(normalized.provisional_candidate, null);
    assert.equal(normalized.schedule.next_scan_at, 1_800_000_300);

    const stockbit = normalizeGainerForecast({
        ...forecast({}, 'stockbit_idx'),
        status: 'SCHEDULED',
        candidates: [],
        schedule: schedule('stockbit_idx'),
    }, 'stockbit_idx');
    assert.equal(stockbit.scope, 'stockbit_idx');
    assert.equal(stockbit.status, 'SCHEDULED');
});

test('insufficient coverage preserves retry status and does not expose sync targets', () => {
    const normalized = normalizeGainerForecast(forecast());
    assert.equal(normalized.status, 'INSUFFICIENT_COVERAGE');
    assert.equal(normalized.schedule.status, 'waiting_for_coverage');
    assert.equal(normalized.schedule.attempt_count, 2);
    assert.equal(normalized.schedule.max_attempts_per_window, 15);
    assert.equal(normalized.schedule.next_scan_at, 1_800_001_000);
    assert.equal(normalized.provisional_candidate, null);
    assert.equal('sync_targets' in normalized, false);
});

test('partial coverage exposes only a validated provisional candidate', () => {
    const normalized = normalizeGainerForecast(forecast({
        provisional_candidate: candidate('tokocrypto_spot', { forecast_return_pct: -2.5 }),
    }));
    assert.equal(normalized.status, 'INSUFFICIENT_COVERAGE');
    assert.equal(normalized.candidates.length, 0);
    assert.equal(normalized.provisional_candidate.symbol, 'BTCIDR');
    assert.equal(normalized.provisional_candidate.forecast_return_pct, -2.5);
    assert.throws(() => normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('tokocrypto_spot')],
        provisional_candidate: candidate('tokocrypto_spot'),
    })));
});

test('Tokocrypto ready result accepts signed native Spot projections and rejects proxy candidates', () => {
    const normalized = normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('tokocrypto_spot')],
    }));
    assert.equal(normalized.candidates[0].symbol, 'BTCIDR');
    assert.equal(normalized.candidates[0].forecast_return_pct, 8.25);
    const negative = normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('tokocrypto_spot', { forecast_return_pct: -2 })],
    }));
    assert.equal(negative.candidates[0].forecast_return_pct, -2);
    const flat = normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('tokocrypto_spot', { forecast_return_pct: 0 })],
    }));
    assert.equal(flat.candidates[0].forecast_return_pct, 0);
    assert.throws(() => normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('tokocrypto_spot', { source_family: 'proxy' })],
    })));
});

test('Stockbit accepts only next-session forecasts from the unadjusted Yahoo research proxy', () => {
    const normalized = normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('stockbit_idx')],
    }, 'stockbit_idx'), 'stockbit_idx');
    assert.equal(normalized.scope, 'stockbit_idx');
    assert.equal(normalized.forecast_until, null);
    assert.equal(normalized.forecast_target, 'Next available IDX session close-to-close return (%)');
    assert.throws(() => normalizeGainerForecast(forecast({
        status: 'READY',
        forecasted_count: 18,
        candidates: [candidate('stockbit_idx', { price_basis: 'venue_native' })],
    }, 'stockbit_idx'), 'stockbit_idx'));
});

test('Stockbit candidates remain visible with a stale label after their target session closes', async () => {
    const { doc, results, stockbitTab } = fakeDocument();
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => ({
        ok: true,
        json: async () => forecast({
            status: 'READY',
            forecasted_count: 18,
            candidates: [candidate('stockbit_idx')],
            schedule: schedule('stockbit_idx', { target_window_passed: true }),
        }, 'stockbit_idx'),
    });
    try {
        initializeGainerForecast(doc);
        await new Promise(resolve => setImmediate(resolve));
        stockbitTab.fire('click');
        await new Promise(resolve => setImmediate(resolve));
        const renderedText = collectText(results);
        assert.match(renderedText, /targeted IDX session has closed/);
        assert.match(renderedText, /BBCA/);
        assert.match(renderedText, /Last forecast · stale/);
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('completed response validates each profile target and horizon separately', () => {
    assert.throws(() => normalizeGainerForecast(forecast({ forecast_until: 1_800_014_400 })));
    assert.throws(() => normalizeGainerForecast(forecast({ forecast_horizon_seconds: 14_400 })));
    assert.throws(() => normalizeGainerForecast(forecast({
        forecast_until: 1_800_086_400,
    }, 'stockbit_idx'), 'stockbit_idx'));
});

test('page loads the scheduled result automatically and has no scan trigger', async () => {
    const { doc, status, results } = fakeDocument();
    const originalFetch = globalThis.fetch;
    let requests = 0;
    globalThis.fetch = async () => {
        requests += 1;
        return { ok: true, json: async () => ({ ...forecast(), status: 'SCHEDULED', schedule: schedule() }) };
    };
    try {
        initializeGainerForecast(doc);
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(requests, 1);
        assert.match(status.textContent, /Next scan/);
        assert.ok(results.children.length > 0);
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('ready page shows the projected return and prospective outcomes', async () => {
    const { doc, results } = fakeDocument();
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => ({
        ok: true,
        json: async () => forecast({
            status: 'READY',
            forecasted_count: 18,
            candidates: [candidate('tokocrypto_spot')],
            observed_outcomes: {
                sample_count: 4,
                mean_realized_return_pct: 1.25,
                positive_return_rate_pct: 75,
                latest_settled_at: 1_800_000_500,
            },
        }),
    });
    try {
        initializeGainerForecast(doc);
        await new Promise(resolve => setImmediate(resolve));
        const renderedText = collectText(results);
        assert.match(renderedText, /Latest scheduled Tokocrypto Spot forecast/);
        assert.match(renderedText, /\+8\.25%/);
        assert.match(renderedText, /Mean realized gross return/);
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('partial coverage still shows the best currently covered ticker as provisional', async () => {
    const { doc, status, results } = fakeDocument();
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => ({
        ok: true,
        json: async () => forecast({
            provisional_candidate: candidate('tokocrypto_spot', { forecast_return_pct: -2.5 }),
        }),
    });
    try {
        initializeGainerForecast(doc);
        await new Promise(resolve => setImmediate(resolve));
        const renderedText = collectText(results);
        assert.match(renderedText, /BTC\/IDR/);
        assert.match(renderedText, /-2\.5%/);
        assert.match(renderedText, /Partial coverage · provisional/);
        assert.match(renderedText, /may not be the full-market leader/);
        assert.match(status.textContent, /highest projection among 10 instruments/);
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('ready page shows the top-ranked ticker when every projection is non-positive', async () => {
    const { doc, status, results } = fakeDocument();
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => ({
        ok: true,
        json: async () => forecast({
            status: 'READY',
            forecasted_count: 18,
            candidates: [candidate('tokocrypto_spot', { forecast_return_pct: -2.5 })],
        }),
    });
    try {
        initializeGainerForecast(doc);
        await new Promise(resolve => setImmediate(resolve));
        const renderedText = collectText(results);
        assert.match(renderedText, /BTC\/IDR/);
        assert.match(renderedText, /-2\.5%/);
        assert.match(renderedText, /Top-ranked ticker/);
        assert.match(status.textContent, /No positive return is projected/);
    } finally {
        globalThis.fetch = originalFetch;
    }
});

test('selecting Stockbit automatically loads that separate forecast profile', async () => {
    const { doc, stockbitTab } = fakeDocument();
    const originalFetch = globalThis.fetch;
    const urls = [];
    globalThis.fetch = async url => {
        urls.push(String(url));
        const scopeId = String(url).includes('stockbit_idx') ? 'stockbit_idx' : 'tokocrypto_spot';
        return {
            ok: true,
            json: async () => ({
                ...forecast({}, scopeId),
                status: 'SCHEDULED',
                schedule: schedule(scopeId),
            }),
        };
    };
    try {
        initializeGainerForecast(doc);
        await new Promise(resolve => setImmediate(resolve));
        stockbitTab.fire('click');
        await new Promise(resolve => setImmediate(resolve));
        assert.equal(urls.length, 2);
        assert.match(urls[0], /scope=tokocrypto_spot/);
        assert.match(urls[1], /scope=stockbit_idx/);
        assert.equal(stockbitTab.attributes['aria-pressed'], 'true');
    } finally {
        globalThis.fetch = originalFetch;
    }
});

function collectText(node) {
    return node.textContent + node.children.map(collectText).join(' ');
}

function fakeDocument() {
    const elements = new Map();
    for (const id of [
        'gainer-forecast',
        'gainer-forecast-status',
        'gainer-forecast-results',
        'gainer-forecast-tab-tokocrypto_spot',
        'gainer-forecast-tab-stockbit_idx',
    ]) elements.set(id, fakeElement());
    return {
        doc: {
            hidden: false,
            defaultView: {
                setTimeout: () => 1,
                clearTimeout: () => {},
                addEventListener: () => {},
            },
            getElementById: id => elements.get(id) || null,
            createElement: () => fakeElement(),
            addEventListener: () => {},
        },
        status: elements.get('gainer-forecast-status'),
        results: elements.get('gainer-forecast-results'),
        stockbitTab: elements.get('gainer-forecast-tab-stockbit_idx'),
    };
}

function fakeElement() {
    const handlers = new Map();
    return {
        children: [],
        attributes: {},
        dataset: {},
        textContent: '',
        setAttribute(name, value) { this.attributes[name] = value; },
        addEventListener(name, handler) { handlers.set(name, handler); },
        fire(name) { handlers.get(name)?.(); },
        append(...children) { this.children.push(...children); },
        replaceChildren(...children) { this.children = [...children]; },
    };
}
