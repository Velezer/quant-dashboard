import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { TREASURY_XML_URL } from './macro-indicator-treasury.js';

const dashboardTemplate = readFileSync(new URL('../../templates/dashboard.html', import.meta.url), 'utf8');
const macroNavigationStyles = readFileSync(new URL('../../styles/macro-navigation.css', import.meta.url), 'utf8');

const today = new Date().toISOString().slice(0, 10);
const currentYear = Number(today.slice(0, 4));
const previousDay = new Date(Date.parse(`${today}T00:00:00Z`) - 86_400_000).toISOString().slice(0, 10);
const observationRows = [
    { date: `${currentYear - 1}-01-02`, value: 2.75 },
    { date: previousDay, value: 2.89 },
    { date: today, value: 2.88 },
];
const responseData = {
    id: 'ten_year_tips_real_yield', title: '10-Year TIPS real yield',
    description: 'A daily US real yield.', unit: '% per year', frequency: 'Daily, business days',
    source: 'U.S. Department of the Treasury', source_series_id: 'TC_10YEAR',
    source_url: 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_real_yield_curve&field_tdr_date_value=2026',
    freshness: 'current', fetch_status: 'fresh', observation_date: today, latest_value: 2.88,
    daily_change_basis_points: -1, retrieved_at_seconds: Math.floor(Date.now() / 1000),
    status_note: null, refresh_error: null, observations: observationRows,
};

function xmlForYear(year) {
    const rows = year === currentYear
        ? observationRows.slice(-2)
        : year === currentYear - 1
            ? observationRows.slice(0, 1)
            : [{ date: `${year}-01-04`, value: 1.25 }];
    const entries = rows.map(row => `<entry><d:NEW_DATE m:type="Edm.DateTime">${row.date}T00:00:00</d:NEW_DATE><d:TC_10YEAR m:type="Edm.Double">${row.value}</d:TC_10YEAR></entry>`).join('');
    return `<feed>${entries}</feed>`;
}

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

test('macro page loads Treasury observations, changes chart range and retains values after refresh failure', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    let failTreasury = false;
    globalThis.fetch = async url => {
        if (String(url).startsWith(TREASURY_XML_URL)) {
            if (failTreasury) return new Response('Treasury unavailable.', { status: 503 });
            const year = Number(new URL(url).searchParams.get('field_tdr_date_value'));
            return new Response(xmlForYear(year), { status: 200 });
        }
        return new Response(JSON.stringify({ error: 'Treasury request timed out.' }), { status: 502 });
    };
    await import(`./macro-indicators.js?success=${Date.now()}`);
    await settle();

    assert.equal(elements.get('macro-series-card').hidden, false);
    assert.equal(elements.get('macro-latest-value').textContent, '2.88%');
    assert.match(elements.get('macro-status').textContent, /Treasury series retrieved at/);
    assert.match(elements.get('macro-chart').innerHTML, /<svg/);
    assert.equal(elements.get('macro-freshness').textContent, 'Current data');
    assert.equal(elements.get('macro-source-series').textContent, 'TC_10YEAR');

    const rangeControls = elements.get('macro-range-controls');
    const fiveYearButton = rangeControls.children.find(button => button.dataset.range === '5y');
    rangeControls.listeners.get('click')({ target: { closest: () => fiveYearButton } });
    assert.equal(fiveYearButton.attributes['aria-pressed'], 'true');

    failTreasury = true;
    await elements.get('macro-refresh').listeners.get('click')();
    assert.equal(elements.get('macro-latest-value').textContent, '2.88%');
    assert.equal(elements.get('macro-freshness').textContent, 'Refresh failed');
    assert.match(elements.get('macro-status').textContent, /previously loaded observation remains visible/);
    assert.match(elements.get('macro-stale-note').textContent, /Treasury request timed out/);
    assert.equal(elements.get('macro-refresh').disabled, false);
});

test('macro page exposes retry after the Treasury feed and API are unavailable', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    globalThis.fetch = async url => String(url).startsWith(TREASURY_XML_URL)
        ? new Response('Treasury unavailable.', { status: 503 })
        : new Response(JSON.stringify({ error: 'Treasury is unavailable.' }), { status: 502 });
    await import(`./macro-indicators.js?error=${Date.now()}`);
    await settle();

    assert.equal(elements.get('macro-empty').hidden, false);
    assert.match(elements.get('macro-empty-message').textContent, /Treasury is unavailable/);
    assert.match(elements.get('macro-status').textContent, /Retry to try again/);
    assert.equal(elements.get('macro-retry').disabled, false);
});

test('macro page uses the API response when the browser cannot reach Treasury', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    globalThis.fetch = async url => String(url).startsWith(TREASURY_XML_URL)
        ? new Response('Treasury unavailable.', { status: 503 })
        : new Response(JSON.stringify(responseData), { status: 200 });
    await import(`./macro-indicators.js?fallback=${Date.now()}`);
    await settle();

    assert.equal(elements.get('macro-series-card').hidden, false);
    assert.equal(elements.get('macro-latest-value').textContent, '2.88%');
    assert.match(elements.get('macro-chart').innerHTML, /<svg/);
    assert.equal(elements.get('macro-freshness').textContent, 'Current data');
    assert.match(elements.get('macro-stale-note').textContent, /direct Treasury feed could not be loaded/);
});

test('macro indicators stay discoverable when the desktop header is crowded', () => {
    assert.match(dashboardTemplate, /<a href="\/macro" aria-label="Open macroeconomic indicators" title="Macroeconomic indicators">\s*<span aria-hidden="true">◒<\/span> Macro indicators<\/a>/);
    assert.match(dashboardTemplate, /<nav id="macroQuickNavigation"[\s\S]*?<a href="\/macro">[\s\S]*?Macroeconomic indicators<\/a>/);
    assert.match(macroNavigationStyles, /@media \(max-width: 1536px\)/);
    assert.match(macroNavigationStyles, /@media \(min-width: 1181px\) and \(max-width: 1536px\)[\s\S]*?desktop-primary-nav a\[href="\/macro"\][\s\S]*?display: none;/);
});
