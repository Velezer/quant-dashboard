import test from 'node:test';
import assert from 'node:assert/strict';
import { FRED_SNAPSHOT_URL } from './macro-indicator-snapshot.js';

const observationRows = [
    { date: '2026-01-02', value: 2.75 },
    { date: '2026-11-18', value: 2.88 },
];
const responseData = {
    id: 'ten_year_tips_real_yield', title: '10-Year TIPS real yield',
    description: 'A daily US real yield.', unit: '% per year', frequency: 'Daily, business days',
    source: 'Federal Reserve Bank of St. Louis · FRED', source_series_id: 'DFII10',
    source_url: 'https://fred.stlouisfed.org/series/DFII10', freshness: 'current', fetch_status: 'fresh',
    observation_date: '2026-11-18', latest_value: 2.88, daily_change_basis_points: -1,
    retrieved_at_seconds: 1_794_960_000, status_note: null, refresh_error: null,
    observations: observationRows,
};
const snapshotCsv = '# retrieved_at_seconds=1795164809\nobservation_date,DFII10\n2026-11-17,2.89\n2026-11-18,2.88\n';

class FakeElement {
    constructor() {
        this.textContent = '';
        this.dataset = {};
        this.attributes = {};
        this.listeners = new Map();
        this.hidden = false;
        this.disabled = false;
        this.children = [];
        this.className = '';
        this.innerHTML = '';
    }
    addEventListener(type, callback) { this.listeners.set(type, callback); }
    setAttribute(name, value) { this.attributes[name] = value; }
    append(...elements) { this.children.push(...elements); }
    replaceChildren(...elements) { this.children = elements; }
    querySelectorAll() { return this.children; }
    contains(element) { return this.children.includes(element); }
}

function mockDocument() {
    const ids = [
        'macro-status', 'macro-refresh', 'macro-retry', 'macro-empty', 'macro-empty-message',
        'macro-series-card', 'macro-freshness', 'macro-chart', 'macro-range-controls',
        'macro-series-title', 'macro-description', 'macro-latest-value', 'macro-observation-date',
        'macro-daily-change', 'macro-retrieved-at', 'macro-source', 'macro-source-series',
        'macro-frequency-unit', 'macro-source-definition', 'macro-source-link', 'macro-stale-note',
        'macro-chart-caption', 'macro-observations-table',
    ];
    const elements = new Map(ids.map(id => [id, new FakeElement()]));
    const rangeControls = ['6m', '1y', '5y', '10y'].map(range => {
        const button = new FakeElement();
        button.dataset.range = range;
        return button;
    });
    elements.get('macro-range-controls').children = rangeControls;
    return {
        elements,
        document: {
            getElementById: id => elements.get(id),
            createElement: () => new FakeElement(),
        },
    };
}

const settle = () => new Promise(resolve => setImmediate(resolve));

test('macro page loads the series, changes chart range and retains values after refresh failure', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    let snapshotFetchCount = 0;
    globalThis.fetch = async url => {
        if (String(url).startsWith(FRED_SNAPSHOT_URL)) {
            snapshotFetchCount += 1;
            return snapshotFetchCount === 1
                ? new Response(snapshotCsv, { status: 200 })
                : new Response('FRED request timed out.', { status: 503 });
        }
        return new Response(JSON.stringify({ error: 'FRED request timed out.' }), { status: 502 });
    };
    await import(`./macro-indicators.js?success=${Date.now()}`);
    await settle();

    assert.equal(elements.get('macro-series-card').hidden, false);
    assert.equal(elements.get('macro-latest-value').textContent, '2.88%');
    assert.match(elements.get('macro-status').textContent, /FRED snapshot retrieved at/);
    assert.match(elements.get('macro-chart').innerHTML, /<svg/);
    assert.equal(elements.get('macro-freshness').textContent, 'Current data');

    const rangeControls = elements.get('macro-range-controls');
    const fiveYearButton = rangeControls.children.find(button => button.dataset.range === '5y');
    rangeControls.listeners.get('click')({ target: { closest: () => fiveYearButton } });
    assert.equal(fiveYearButton.attributes['aria-pressed'], 'true');

    await elements.get('macro-refresh').listeners.get('click')();
    assert.equal(elements.get('macro-latest-value').textContent, '2.88%');
    assert.equal(elements.get('macro-freshness').textContent, 'Refresh failed');
    assert.match(elements.get('macro-status').textContent, /previously loaded observation remains visible/);
    assert.match(elements.get('macro-stale-note').textContent, /FRED request timed out/);
    assert.equal(elements.get('macro-refresh').disabled, false);
});

test('macro page exposes retry after its first data request is unavailable', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    globalThis.fetch = async () => new Response(JSON.stringify({ error: 'FRED is unavailable.' }), { status: 502 });
    await import(`./macro-indicators.js?error=${Date.now()}`);
    await settle();

    assert.equal(elements.get('macro-empty').hidden, false);
    assert.match(elements.get('macro-empty-message').textContent, /FRED is unavailable/);
    assert.match(elements.get('macro-status').textContent, /Retry to try again/);
    assert.equal(elements.get('macro-retry').disabled, false);
});

test('macro page keeps a chart from the deployed snapshot when the latest source is unavailable', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    globalThis.fetch = async url => String(url).startsWith(FRED_SNAPSHOT_URL)
        ? new Response('FRED snapshot is unavailable.', { status: 503 })
        : new Response(JSON.stringify(responseData), { status: 200 });
    await import(`./macro-indicators.js?fallback=${Date.now()}`);
    await settle();

    assert.equal(elements.get('macro-series-card').hidden, false);
    assert.equal(elements.get('macro-latest-value').textContent, '2.88%');
    assert.match(elements.get('macro-chart').innerHTML, /<svg/);
    assert.equal(elements.get('macro-freshness').textContent, 'Stale data');
    assert.match(elements.get('macro-stale-note').textContent, /last snapshot bundled with this dashboard/);
});
