import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { TREASURY_XML_URL } from './macro-indicator-treasury.js';

const dashboardTemplate = readFileSync(new URL('../../templates/dashboard.html', import.meta.url), 'utf8');
const macroTemplate = readFileSync(new URL('../../templates/macro-indicators.html', import.meta.url), 'utf8');
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
    description: 'A daily US real yield.', limitations: 'Not an inflation forecast.',
    unit: '% per year', value_format: 'percent', frequency: 'Daily, business days',
    source: 'U.S. Department of the Treasury', source_series_id: 'TC_10YEAR',
    source_url: 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_real_yield_curve&field_tdr_date_value=2026',
    freshness: 'current', fetch_status: 'fresh', observation_date: today, latest_value: 2.88,
    observation_change: -0.01, change_period: 'from the prior business day', daily_change_basis_points: -1,
    retrieved_at_seconds: Math.floor(Date.now() / 1000),
    status_note: null, refresh_error: null, observations: observationRows,
};
const responseTwoYear = {
    ...responseData,
    id: 'two_year_treasury_nominal_yield', title: '2-Year U.S. Treasury yield',
    description: 'The daily nominal 2-year constant-maturity Treasury rate.',
    source: 'U.S. Department of the Treasury',
    source_series_id: 'BC_2YEAR', source_url: 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_yield_curve',
    observation_change: 0.02, daily_change_basis_points: 2,
};
const responseFx = {
    id: 'usd_idr_daily_reference', title: 'USD/IDR exchange rate',
    description: 'Daily USD/IDR cross-rate calculated from the ECB euro reference rates.',
    limitations: 'Calculated from IDR/EUR divided by USD/EUR; not an intraday quote or the daily JISDOR fixing.', unit: 'IDR per USD', value_format: 'idr_per_usd',
    frequency: 'Daily, business days', source: 'European Central Bank',
    source_series_id: 'D.IDR.EUR.SP00.A / D.USD.EUR.SP00.A', source_url: 'https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html',
    freshness: 'current', fetch_status: 'fresh', observation_date: today, latest_value: 17983.82,
    observation_change: 120.5, change_period: 'from the prior business day', daily_change_basis_points: null,
    retrieved_at_seconds: Math.floor(Date.now() / 1000), status_note: null, refresh_error: null,
    observations: [
        { date: previousDay, value: 17844 },
        { date: today, value: 17983.82 },
    ],
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
        this.href = '';
        this.roles = new Map();
        this.actionButtons = new Map();
        this.rangeButtons = [];
    }
    addEventListener(type, callback) { this.listeners.set(type, callback); }
    setAttribute(name, value) { this.attributes[name] = value; }
    append(...elements) { this.children.push(...elements); }
    replaceChildren(...elements) { this.children = elements; }
    querySelector(selector) {
        const roleMatch = selector.match(/^\[data-role="([^"]+)"\]$/);
        if (roleMatch) return this.roles.get(roleMatch[1]);
        const actionMatch = selector.match(/^\[data-action="([^"]+)"\]$/);
        if (actionMatch) return this.actionButtons.get(actionMatch[1]);
        return null;
    }
    querySelectorAll(selector) {
        return selector === 'button[data-range]' ? this.rangeButtons : this.children;
    }
    contains(element) { return this.children.includes(element) || this.rangeButtons.includes(element); }
    cloneNode() { return new FakeCard(); }
}

class FakeCard extends FakeElement {
    constructor() {
        super();
        for (const name of [
            'category', 'title', 'description', 'freshness', 'latest', 'observation-date', 'change',
            'change-label', 'change-unit', 'retrieved-at', 'status', 'stale-note', 'chart-heading',
            'chart-caption', 'range-controls', 'chart', 'source', 'source-series', 'frequency-unit',
            'source-definition', 'limitations', 'source-link', 'observations-table', 'data-limits',
        ]) this.roles.set(name, new FakeElement());
        const retry = new FakeElement();
        this.actionButtons.set('refresh', retry);
        this.roles.get('range-controls').rangeButtons = ['6m', '1y', '5y', '10y'].map(range => {
            const button = new FakeElement();
            button.dataset.range = range;
            return button;
        });
    }
}

function mockDocument() {
    const elements = new Map([
        ['macro-status', new FakeElement()],
        ['macro-refresh', new FakeElement()],
        ['macro-indicator-grid', new FakeElement()],
    ]);
    const template = new FakeElement();
    template.content = { firstElementChild: { cloneNode: () => new FakeCard() } };
    elements.set('macro-series-template', template);
    return {
        elements,
        document: {
            getElementById: id => elements.get(id),
            createElement: () => new FakeElement(),
        },
    };
}

const settle = () => new Promise(resolve => setImmediate(resolve));
const cardFor = (elements, id) => elements.get('macro-indicator-grid').children.find(card => card.dataset.macroId === id);
const roleFor = (card, name) => card.roles.get(name);

test('macro page renders separate charts, source metadata and independent refresh controls', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    let failTreasury = false;
    globalThis.fetch = async url => {
        const address = String(url);
        if (address.startsWith(TREASURY_XML_URL)) {
            if (failTreasury) return new Response('Treasury unavailable.', { status: 503 });
            const year = Number(new URL(address).searchParams.get('field_tdr_date_value'));
            return new Response(xmlForYear(year), { status: 200 });
        }
        const parsedUrl = new URL(address, 'https://dashboard.example');
        const id = parsedUrl.searchParams.get('id');
        if (id === 'two_year_treasury_nominal_yield') return new Response(JSON.stringify(responseTwoYear), { status: 200 });
        if (id === 'usd_idr_daily_reference') return new Response(JSON.stringify(responseFx), { status: 200 });
        return new Response(JSON.stringify({ error: 'Treasury request timed out.' }), { status: 502 });
    };
    await import(`./macro-indicators.js?success=${Date.now()}`);
    await settle();

    const tips = cardFor(elements, 'ten_year_tips_real_yield');
    const twoYear = cardFor(elements, 'two_year_treasury_nominal_yield');
    const fx = cardFor(elements, 'usd_idr_daily_reference');
    assert.equal(elements.get('macro-indicator-grid').children.length, 3);
    assert.equal(roleFor(tips, 'latest').textContent, '2.88%');
    assert.equal(roleFor(twoYear, 'latest').textContent, '2.88%');
    assert.match(roleFor(fx, 'latest').textContent, /IDR 17,983\.82 \/ USD/);
    assert.match(roleFor(fx, 'chart').innerHTML, /<svg/);
    assert.match(roleFor(fx, 'chart').innerHTML, /17,983/);
    assert.equal(roleFor(fx, 'source-series').textContent, 'D.IDR.EUR.SP00.A / D.USD.EUR.SP00.A');
    assert.match(roleFor(fx, 'observation-date').textContent, new RegExp(today));

    const rangeControls = roleFor(tips, 'range-controls');
    const fiveYearButton = rangeControls.rangeButtons.find(button => button.dataset.range === '5y');
    rangeControls.listeners.get('click')({ target: { closest: () => fiveYearButton } });
    assert.equal(fiveYearButton.attributes['aria-pressed'], 'true');

    failTreasury = true;
    await tips.actionButtons.get('refresh').listeners.get('click')();
    assert.equal(roleFor(tips, 'latest').textContent, '2.88%');
    assert.equal(roleFor(tips, 'freshness').textContent, 'Refresh failed');
    assert.match(roleFor(tips, 'status').textContent, /previously loaded observation remains visible/);
    assert.match(roleFor(tips, 'stale-note').textContent, /Treasury request timed out/);
    assert.equal(tips.actionButtons.get('refresh').disabled, false);
});

test('macro cards retain retry states when a provider is unavailable', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    globalThis.fetch = async url => String(url).startsWith(TREASURY_XML_URL)
        ? new Response('Treasury unavailable.', { status: 503 })
        : new Response(JSON.stringify({ error: 'Data source unavailable.' }), { status: 502 });
    await import(`./macro-indicators.js?error=${Date.now()}`);
    await settle();

    const fx = cardFor(elements, 'usd_idr_daily_reference');
    assert.equal(roleFor(fx, 'latest').textContent, 'Unavailable');
    assert.match(roleFor(fx, 'status').textContent, /Data source unavailable/);
    assert.equal(fx.actionButtons.get('refresh').disabled, false);
    assert.match(elements.get('macro-status').textContent, /unavailable/);
});

test('macro page uses its API response when the browser cannot reach Treasury', async () => {
    const { document, elements } = mockDocument();
    globalThis.document = document;
    globalThis.fetch = async url => String(url).startsWith(TREASURY_XML_URL)
        ? new Response('Treasury unavailable.', { status: 503 })
        : new Response(JSON.stringify(responseData), { status: 200 });
    await import(`./macro-indicators.js?fallback=${Date.now()}`);
    await settle();

    const tips = cardFor(elements, 'ten_year_tips_real_yield');
    assert.equal(roleFor(tips, 'latest').textContent, '2.88%');
    assert.match(roleFor(tips, 'chart').innerHTML, /<svg/);
    assert.equal(roleFor(tips, 'freshness').textContent, 'Current data');
    assert.match(roleFor(tips, 'stale-note').textContent, /direct Treasury feed could not be loaded/);
});

test('macro indicators remain discoverable from the screener and their page includes per-series status and provenance', () => {
    assert.match(dashboardTemplate, /<a href="\/macro" aria-label="Open macroeconomic indicators" title="Macroeconomic indicators">\s*<span aria-hidden="true">◒<\/span> Macro indicators<\/a>/);
    assert.match(dashboardTemplate, /<nav id="macroQuickNavigation"[\s\S]*?<a href="\/macro">[\s\S]*?Macroeconomic indicators<\/a>/);
    assert.match(macroNavigationStyles, /\.tv-terminal \.macro-quick-nav\s*\{\s*display: flex;/);
    assert.match(macroNavigationStyles, /@media \(min-width: 1537px\)[\s\S]*?signals-workspace-active \.macro-quick-nav[\s\S]*?display: none;/);
    assert.match(macroNavigationStyles, /@media \(min-width: 1181px\) and \(max-width: 1536px\)[\s\S]*?desktop-primary-nav a\[href="\/macro"\][\s\S]*?display: none;/);
    assert.match(macroTemplate, /<template id="macro-series-template">/);
    assert.match(macroTemplate, /data-role="status" role="status"/);
    assert.match(macroTemplate, /data-role="source"/);
    assert.match(macroTemplate, /data-role="limitations"/);
    assert.match(macroTemplate, /data-role="chart"/);
    assert.match(macroTemplate, /NWE\/DSS calculations, the combined ticker score, or signal direction/);
});
