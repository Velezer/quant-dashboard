import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { TREASURY_XML_URL } from './macro-indicator-treasury.js';

const dashboardTemplate = readFileSync(new URL('../../templates/dashboard.html', import.meta.url), 'utf8');
const macroTemplate = readFileSync(new URL('../../templates/macro-indicators.html', import.meta.url), 'utf8');
const macroScript = readFileSync(new URL('./macro-indicators.js', import.meta.url), 'utf8');
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
const responseBitcoinFlows = {
    id: 'bitcoin_spot_etf_net_flows', title: 'U.S. spot Bitcoin ETF net flows',
    description: 'Daily net creations and redemptions across U.S. spot Bitcoin ETFs.',
    limitations: 'The latest flow can lag the U.S. market close and may be revised.',
    unit: 'USD per reported date', value_format: 'usd_flow', frequency: 'Daily, as reported',
    source: 'The Commoner Bitcoin ETF Tracker by TFTC', source_series_id: 'netFlowUsd · all U.S. spot Bitcoin ETFs',
    source_url: 'https://www.tftc.io/bitcoin-etf-flows', freshness: 'current', fetch_status: 'fresh',
    observation_date: today, latest_value: -125_500_000, latest_classification: null,
    observation_change: -155_500_000, change_period: 'from the prior reported date', daily_change_basis_points: null,
    retrieved_at_seconds: Math.floor(Date.now() / 1000), status_note: null, refresh_error: null,
    observations: [
        { date: previousDay, value: 30_000_000 },
        { date: today, value: -125_500_000 },
    ],
};
const responseFearGreed = {
    id: 'bitcoin_fear_greed_index', title: 'Bitcoin Fear & Greed Index',
    description: 'A daily 0–100 reading of crypto-market sentiment.',
    limitations: 'Broad crypto sentiment, not a Bitcoin price forecast.',
    unit: 'Index points (0–100)', value_format: 'index_0_100', frequency: 'Daily',
    source: 'Alternative.me', source_series_id: 'Crypto Fear & Greed Index',
    source_url: 'https://alternative.me/crypto/fear-and-greed-index/', freshness: 'current', fetch_status: 'fresh',
    observation_date: today, latest_value: 11, latest_classification: 'Extreme Fear',
    observation_change: -17, change_period: 'from the prior daily reading', daily_change_basis_points: null,
    retrieved_at_seconds: Math.floor(Date.now() / 1000), status_note: null, refresh_error: null,
    observations: [
        { date: previousDay, value: 28, classification: 'Fear' },
        { date: today, value: 11, classification: 'Extreme Fear' },
    ],
};

function apiDataFor(id) {
    if (id === 'two_year_treasury_nominal_yield') return responseTwoYear;
    if (id === 'usd_idr_daily_reference') return responseFx;
    if (id === 'bitcoin_spot_etf_net_flows') return responseBitcoinFlows;
    if (id === 'bitcoin_fear_greed_index') return responseFearGreed;
    if (id === 'brent_crude_spot') return {
        ...responseData, id, title: 'Brent crude spot price', value_format: 'usd_per_barrel',
        unit: 'USD per barrel', latest_value: 92.94, observation_change: 0.5,
        source_series_id: 'RBRTE', source_url: 'https://www.eia.gov/dnav/pet/hist/RBRTEd.htm',
        observations: [{ date: previousDay, value: 92.44 }, { date: today, value: 92.94 }],
    };
    if (id === 'us_commercial_crude_stocks') return {
        ...responseData, id, title: 'U.S. commercial crude oil stocks', value_format: 'million_barrels',
        unit: 'Million barrels', latest_value: 417.797, observation_change: 3.38,
        source_series_id: 'WCESTUS1', source_url: 'https://www.eia.gov/dnav/pet/hist/LeafHandler.ashx?f=W&n=PET&s=WCESTUS1',
        observations: [{ date: previousDay, value: 414.417 }, { date: today, value: 417.797 }],
    };
    if (id === 'us_equity_volatility_index') return {
        ...responseData, id, title: 'Cboe VIX volatility index', value_format: 'index_points',
        unit: 'Index points', latest_value: 28.44, observation_change: 1.25,
        source_series_id: 'VIX', source_url: 'https://cdn.cboe.com/api/global/us_indices/daily_prices/VIX_History.csv',
        observations: [{ date: previousDay, value: 27.19 }, { date: today, value: 28.44 }],
    };
    if (id === 'broad_us_dollar_index') return {
        ...responseData, id, title: 'Broad U.S. dollar index', value_format: 'index_points',
        unit: 'Index points', latest_value: 122.6503, observation_change: -0.1,
        source_series_id: 'JRXWTFB_N.B', source_url: 'https://www.federalreserve.gov/releases/h10/current/',
        observations: [{ date: previousDay, value: 122.7503 }, { date: today, value: 122.6503 }],
    };
    if (id === 'ten_year_treasury_nominal_yield') return {
        ...responseData, id, title: '10-Year U.S. Treasury yield', source_series_id: 'BC_10YEAR',
        source_url: 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_yield_curve',
    };
    if (id === 'ten_year_breakeven_inflation') return {
        ...responseData, id, title: '10-Year breakeven inflation rate', source_series_id: 'BC_10YEAR − TC_10YEAR, matched by date',
        source_url: 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_yield_curve',
    };
    if (id === 'us_manufacturing_industrial_production') return {
        ...responseData, id, title: 'U.S. manufacturing industrial production', value_format: 'index_points',
        unit: 'Index points', latest_value: 99.7246, source_series_id: 'GMF',
        source_url: 'https://www.federalreserve.gov/releases/g17/download.htm',
    };
    if (id === 'china_manufacturing_pmi') return {
        ...responseData, id, title: 'China official manufacturing PMI', value_format: 'index_points',
        unit: 'Index points', latest_value: 49.3, source: 'National Bureau of Statistics of China',
        source_series_id: 'NBS nationwide manufacturing PMI',
        source_url: 'https://data.stats.gov.cn/dg/website/publicrelease/web/external/query?search=PMI&pagenum=1&pageSize=10',
    };
    if (id === 'wti_crude_spot') return {
        ...responseData, id, title: 'WTI crude spot price', value_format: 'usd_per_barrel',
        unit: 'USD per barrel', latest_value: 89.53, source_series_id: 'RWTC',
        source_url: 'https://www.eia.gov/dnav/pet/hist/RWTCd.htm',
    };
    if (id === 'crude_oil_volatility_index') return {
        ...responseData, id, title: 'Cboe crude oil volatility index', value_format: 'index_points',
        unit: 'Index points', latest_value: 50.67, source_series_id: 'OVX',
        source_url: 'https://cdn.cboe.com/api/global/us_indices/daily_prices/OVX_History.csv',
    };
    return responseData;
}

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
            'change-label', 'change-unit', 'retrieved-at', 'latest-classification', 'attribution-link',
            'status', 'stale-note', 'chart-heading',
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
        if (id === 'ten_year_tips_real_yield') return new Response(JSON.stringify({ error: 'Treasury request timed out.' }), { status: 502 });
        if (id) return new Response(JSON.stringify(apiDataFor(id)), { status: 200 });
        return new Response(JSON.stringify({ error: 'Treasury request timed out.' }), { status: 502 });
    };
    await import(`./macro-indicators.js?success=${Date.now()}`);
    await settle();

    const tips = cardFor(elements, 'ten_year_tips_real_yield');
    const twoYear = cardFor(elements, 'two_year_treasury_nominal_yield');
    const fx = cardFor(elements, 'usd_idr_daily_reference');
    const etfFlows = cardFor(elements, 'bitcoin_spot_etf_net_flows');
    const fearGreed = cardFor(elements, 'bitcoin_fear_greed_index');
    const chinaPmi = cardFor(elements, 'china_manufacturing_pmi');
    assert.equal(elements.get('macro-indicator-grid').children.length, 15);
    assert.equal(roleFor(chinaPmi, 'latest').textContent, '49.3 points');
    assert.match(roleFor(chinaPmi, 'chart').innerHTML, /<svg/);
    assert.match(roleFor(chinaPmi, 'chart').innerHTML, /macro-chart-reference/);
    assert.equal(roleFor(chinaPmi, 'attribution-link').href, apiDataFor('china_manufacturing_pmi').source_url);
    assert.equal(roleFor(tips, 'latest').textContent, '2.88%');
    assert.equal(roleFor(twoYear, 'latest').textContent, '2.88%');
    assert.match(roleFor(fx, 'latest').textContent, /IDR 17,983\.82 \/ USD/);
    assert.match(roleFor(fx, 'chart').innerHTML, /<svg/);
    assert.match(roleFor(fx, 'chart').innerHTML, /17,983/);
    assert.equal(roleFor(fx, 'source-series').textContent, 'D.IDR.EUR.SP00.A / D.USD.EUR.SP00.A');
    assert.match(roleFor(fx, 'observation-date').textContent, new RegExp(today));
    assert.equal(roleFor(etfFlows, 'latest').textContent, '−$125.5M');
    assert.match(roleFor(etfFlows, 'chart').innerHTML, /macro-chart-zero/);
    assert.equal(roleFor(etfFlows, 'attribution-link').textContent, responseBitcoinFlows.source);
    assert.equal(roleFor(fearGreed, 'latest').textContent, '11 / 100');
    assert.equal(roleFor(fearGreed, 'latest-classification').textContent, 'Extreme Fear');
    assert.equal(roleFor(fearGreed, 'change-unit').textContent, 'Index points');
    assert.match(roleFor(fearGreed, 'chart').innerHTML, />100<\/text>/);
    assert.equal(roleFor(fearGreed, 'attribution-link').href, responseFearGreed.source_url);
    assert.equal(roleFor(cardFor(elements, 'brent_crude_spot'), 'latest').textContent, '$92.94 / bbl');
    assert.match(roleFor(cardFor(elements, 'us_commercial_crude_stocks'), 'latest').textContent, /417\.8 million bbl/);
    assert.equal(roleFor(cardFor(elements, 'us_equity_volatility_index'), 'latest').textContent, '28.44 points');

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
    globalThis.fetch = async url => {
        if (String(url).startsWith(TREASURY_XML_URL)) return new Response('Treasury unavailable.', { status: 503 });
        const id = new URL(String(url), 'https://dashboard.example').searchParams.get('id');
        return new Response(JSON.stringify(apiDataFor(id)), { status: 200 });
    };
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
    assert.match(macroTemplate, /XAUUSD · Gold \/ U\.S\. dollar/);
    assert.match(macroTemplate, /XAGUSD · Silver \/ U\.S\. dollar/);
    assert.match(macroTemplate, /UKOUSD · Brent crude/);
    assert.match(macroTemplate, /ISM Manufacturing PMI/);
    assert.match(macroTemplate, /China official manufacturing PMI/);
    assert.match(macroTemplate, /#macro-indicator-china_manufacturing_pmi/);
    assert.match(macroTemplate, /macro-pmi-note/);
    assert.match(macroTemplate, /href="#macro-indicator-us_manufacturing_industrial_production">Federal Reserve production chart/);
    assert.match(macroScript, /card\.id = `macro-indicator-\$\{config\.id\}`/);
});
