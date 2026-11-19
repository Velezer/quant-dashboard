import test from 'node:test';
import assert from 'node:assert/strict';
import { initializeGainerForecast, normalizeGainerForecast } from './gainer-forecast.js';

function schedule(overrides = {}) {
    return {
        status: 'scheduled',
        attempt_count: 0,
        max_attempts_per_window: 5,
        last_attempt_at: null,
        next_scan_at: 1_800_000_300,
        last_error: null,
        forecast_cutoff: 1_800_000_000,
        ...overrides,
    };
}

function forecast(overrides = {}) {
    return {
        status: 'INSUFFICIENT_COVERAGE',
        as_of: 1_800_000_000,
        forecast_until: 1_800_014_400,
        eligible_count: 20,
        scored_count: 10,
        minimum_scored_count: 16,
        missing_or_stale_count: 10,
        invalid_history_count: 0,
        read_error_count: 0,
        model: 'Combined six-indicator score',
        candidates: [],
        schedule: schedule({ status: 'waiting_for_coverage', attempt_count: 2, next_scan_at: 1_800_001_000 }),
        ...overrides,
    };
}

test('scheduled response gives the page a scan time without fabricating a candidate', () => {
    const normalized = normalizeGainerForecast({
        status: 'SCHEDULED',
        candidates: [],
        schedule: schedule(),
    });
    assert.equal(normalized.status, 'SCHEDULED');
    assert.deepEqual(normalized.candidates, []);
    assert.equal(normalized.schedule.next_scan_at, 1_800_000_300);
});

test('insufficient coverage includes retry schedule and never exposes sync targets', () => {
    const normalized = normalizeGainerForecast(forecast());
    assert.equal(normalized.status, 'INSUFFICIENT_COVERAGE');
    assert.equal(normalized.schedule.status, 'waiting_for_coverage');
    assert.equal(normalized.schedule.attempt_count, 2);
    assert.equal(normalized.schedule.max_attempts_per_window, 5);
    assert.equal(normalized.schedule.next_scan_at, 1_800_001_000);
    assert.equal('sync_targets' in normalized, false);
});

test('ready result accepts only positive Tokocrypto native candidates', () => {
    const normalized = normalizeGainerForecast(forecast({
        status: 'READY',
        scored_count: 18,
        candidates: [{
            symbol: 'BTCIDR',
            venue_symbol: 'BTC/IDR',
            source_family: 'tokocrypto_native',
            score: 34.5,
            quote_currency: 'IDR',
            instrument_id: 'tokocrypto:spot:btcidr',
        }],
    }));
    assert.equal(normalized.candidates[0].symbol, 'BTCIDR');
    assert.equal(normalized.candidates[0].score, 34.5);
    assert.throws(() => normalizeGainerForecast(forecast({
        status: 'READY',
        scored_count: 18,
        candidates: [{ symbol: 'BTCIDR', source_family: 'proxy', score: 34.5 }],
    })));
});

test('scheduled result rejects an invalid state or timestamp', () => {
    assert.throws(() => normalizeGainerForecast({
        status: 'SCHEDULED',
        schedule: schedule({ status: 'manual' }),
    }));
    assert.throws(() => normalizeGainerForecast({
        status: 'SCHEDULED',
        schedule: schedule({ next_scan_at: -1 }),
    }));
});

test('opening the page loads a scheduled result without a refresh button', async () => {
    const section = fakeElement();
    const status = fakeElement();
    const results = fakeElement();
    const elements = new Map([
        ['gainer-forecast', section],
        ['gainer-forecast-status', status],
        ['gainer-forecast-results', results],
    ]);
    let timeoutId = 0;
    const win = {
        setTimeout: () => ++timeoutId,
        clearTimeout: () => {},
        addEventListener: () => {},
    };
    const doc = {
        hidden: false,
        defaultView: win,
        getElementById: id => elements.get(id) || null,
        createElement: () => fakeElement(),
        addEventListener: () => {},
    };
    const originalFetch = globalThis.fetch;
    let requests = 0;
    globalThis.fetch = async () => {
        requests += 1;
        return {
            ok: true,
            json: async () => ({
                status: 'SCHEDULED',
                candidates: [],
                schedule: schedule(),
            }),
        };
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

function fakeElement() {
    return {
        children: [],
        dataset: {},
        textContent: '',
        setAttribute: () => {},
        append(...children) { this.children.push(...children); },
        replaceChildren(...children) { this.children = [...children]; },
    };
}
