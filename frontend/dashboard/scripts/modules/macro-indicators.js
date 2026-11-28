import { requestJson } from './api.js';
import { formatObservationChange, formatValue, observationsForRange, renderMacroChart } from './macro-indicator-view.js';
import { loadTreasurySnapshot } from './macro-indicator-treasury.js';

const TIPS_ID = 'ten_year_tips_real_yield';
const ASSET_FILTERS = Object.freeze([
    { id: 'all', label: 'All indicators' },
    { id: 'precious_metals', label: 'Precious metals', assets: ['xauusd', 'xagusd'] },
    { id: 'xauusd', label: 'Gold · XAUUSD', assets: ['xauusd'] },
    { id: 'xagusd', label: 'Silver · XAGUSD', assets: ['xagusd'] },
    { id: 'ukousd', label: 'Energy · UKOUSD', assets: ['ukousd'] },
    { id: 'bitcoin', label: 'Crypto · Bitcoin', assets: ['bitcoin'] },
    { id: 'usdidr', label: 'FX · USD/IDR', assets: ['usdidr'] },
]);
const INDICATORS = Object.freeze([
    // biasByAsset polarity: 1 means rising values favor that asset; -1 means falling values do.
    { id: TIPS_ID, assets: ['xauusd', 'xagusd', 'bitcoin'], biasByAsset: { xauusd: -1, xagusd: -1, bitcoin: -1 }, category: 'GOLD & SILVER · REAL RATE', fallbackTitle: '10-Year TIPS real yield', summary: 'Daily inflation-adjusted 10-year U.S. yield.', chartHeading: 'Yield history' },
    { id: 'two_year_treasury_nominal_yield', assets: ['xauusd', 'xagusd', 'bitcoin'], biasByAsset: { xauusd: -1, xagusd: -1, bitcoin: -1 }, category: 'GOLD & SILVER · FED PATH', fallbackTitle: '2-Year U.S. Treasury yield', summary: 'Daily market rate for the expected Fed path.', chartHeading: 'Yield history' },
    { id: 'usd_idr_daily_reference', assets: ['usdidr'], biasByAsset: { usdidr: 1 }, category: 'FOREIGN EXCHANGE · INDONESIA', fallbackTitle: 'USD/IDR exchange rate', summary: 'ECB reference USD/IDR cross-rate.', chartHeading: 'Exchange-rate history' },
    { id: 'bitcoin_spot_etf_net_flows', assets: ['bitcoin'], biasByAsset: { bitcoin: 1 }, biasMode: 'level', biasReference: 0, category: 'CRYPTO FUND FLOWS · UNITED STATES', fallbackTitle: 'U.S. spot Bitcoin ETF net flows', summary: 'Net ETF creations and redemptions.', chartHeading: 'Daily net flow history', source: 'The Commoner Bitcoin ETF Tracker by TFTC', sourceUrl: 'https://www.tftc.io/bitcoin-etf-flows' },
    { id: 'bitcoin_fear_greed_index', assets: ['bitcoin'], biasByAsset: { bitcoin: 1 }, biasMode: 'level', biasReference: 50, category: 'CRYPTO MARKET SENTIMENT', fallbackTitle: 'Bitcoin Fear & Greed Index', summary: 'Daily crypto sentiment · 0–100.', chartHeading: 'Sentiment history', source: 'Alternative.me', sourceUrl: 'https://alternative.me/crypto/fear-and-greed-index/' },
    { id: 'bitcoin_options_max_pain', assets: ['bitcoin'], biasMode: 'context', category: 'BITCOIN OPTIONS · DERIBIT', fallbackTitle: 'Bitcoin options Max Pain', summary: 'Calculated strike by options expiry · 8h open-interest delay.', chartHeading: 'Max Pain by expiry', source: 'Deribit', sourceUrl: 'https://docs.deribit.com/api-reference/market-data/public-get_book_summary_by_currency', chartUsesAllObservations: true, hideChange: true, latestLabel: 'Nearest expiry Max Pain', observationLabel: 'Nearest expiry' },
    { id: 'broad_us_dollar_index', assets: ['xauusd', 'xagusd', 'ukousd', 'bitcoin', 'usdidr'], biasByAsset: { xauusd: -1, xagusd: -1, ukousd: -1, bitcoin: -1, usdidr: 1 }, category: 'GOLD · SILVER · OIL · USD', fallbackTitle: 'Broad U.S. dollar index', summary: 'Fed trade-weighted dollar index.', chartHeading: 'Dollar history' },
    { id: 'ten_year_treasury_nominal_yield', assets: ['xauusd', 'xagusd', 'bitcoin'], biasByAsset: { xauusd: -1, xagusd: -1, bitcoin: -1 }, category: 'GOLD & SILVER · NOMINAL RATE', fallbackTitle: '10-Year U.S. Treasury yield', summary: 'Daily nominal 10-year U.S. yield.', chartHeading: 'Yield history' },
    { id: 'ten_year_breakeven_inflation', assets: ['xauusd', 'xagusd'], biasByAsset: { xauusd: 1, xagusd: 1 }, category: 'GOLD · INFLATION PRICING', fallbackTitle: '10-Year breakeven inflation rate', summary: '10-year nominal yield minus TIPS yield.', chartHeading: 'Inflation-compensation history' },
    { id: 'us_equity_volatility_index', assets: ['xauusd', 'xagusd', 'ukousd', 'bitcoin'], biasByAsset: { xauusd: 1, xagusd: -1, ukousd: -1, bitcoin: -1 }, category: 'GOLD · SILVER · RISK PROXY', fallbackTitle: 'Cboe VIX volatility index', summary: 'U.S. equity-volatility proxy.', chartHeading: 'Volatility history' },
    { id: 'us_manufacturing_industrial_production', assets: ['xagusd', 'ukousd'], biasByAsset: { xagusd: 1, ukousd: 1 }, category: 'SILVER · MONTHLY ACTIVITY PROXY', fallbackTitle: 'U.S. manufacturing industrial production', summary: 'Monthly U.S. manufacturing output.', chartHeading: 'Industrial production history' },
    {
        id: 'china_manufacturing_pmi', assets: ['xagusd', 'ukousd'], biasByAsset: { xagusd: 1, ukousd: 1 }, biasMode: 'level', biasReference: 50, category: 'SILVER & OIL · CHINA ACTIVITY',
        fallbackTitle: 'China official manufacturing PMI', summary: 'Monthly factory activity · 50 marks expansion.', chartHeading: 'Manufacturing PMI history', referenceLine: 50,
    },
    { id: 'brent_crude_spot', assets: ['ukousd'], biasByAsset: { ukousd: 1 }, category: 'UKOUSD · BRENT BENCHMARK', fallbackTitle: 'Brent crude spot price', summary: 'Daily EIA Brent spot benchmark.', chartHeading: 'Brent spot history' },
    { id: 'wti_crude_spot', assets: ['ukousd'], biasByAsset: { ukousd: 1 }, category: 'UKOUSD · REGIONAL BENCHMARK', fallbackTitle: 'WTI crude spot price', summary: 'Daily EIA WTI spot benchmark.', chartHeading: 'WTI spot history' },
    { id: 'crude_oil_volatility_index', assets: ['ukousd'], biasMode: 'context', category: 'UKOUSD · OIL RISK PRICING', fallbackTitle: 'Cboe crude oil volatility index', summary: 'Options volatility for crude oil ETFs.', chartHeading: 'Oil volatility history' },
    { id: 'us_commercial_crude_stocks', assets: ['ukousd'], biasByAsset: { ukousd: -1 }, category: 'UKOUSD · WEEKLY SUPPLY BALANCE', fallbackTitle: 'U.S. commercial crude oil stocks', summary: 'Weekly U.S. commercial crude inventory.', chartHeading: 'Inventory history' },
]);

const statusElement = document.getElementById('macro-status');
const refreshButton = document.getElementById('macro-refresh');
const indicatorGrid = document.getElementById('macro-indicator-grid');
const cardTemplate = document.getElementById('macro-series-template');
const assetFilters = document.getElementById('macro-asset-filters');
const records = new Map();
let selectedAssetFilter = ASSET_FILTERS[0];

const ASSET_LABELS = Object.freeze({
    xauusd: 'Gold', xagusd: 'Silver', ukousd: 'Brent', bitcoin: 'Bitcoin', usdidr: 'USD/IDR',
});

function standardDeviation(values) {
    if (!values.length) return 0;
    const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
    return Math.sqrt(values.reduce((sum, value) => sum + ((value - mean) ** 2), 0) / values.length);
}

function directionalBias(record, assetId) {
    const { config, snapshot } = record;
    if (config.biasMode === 'context' || !config.biasByAsset?.[assetId]) return null;
    const observations = (snapshot.observations || []).filter(item => Number.isFinite(Number(item.value)));
    const values = observations.map(item => Number(item.value));
    let signedMove;
    let scale;

    if (config.biasMode === 'level') {
        const reference = Number(config.biasReference ?? 0);
        signedMove = (Number(snapshot.latest_value) - reference) * config.biasByAsset[assetId];
        scale = standardDeviation(values.map(value => value - reference));
    } else {
        if (values.length < 3) return null;
        const moves = values.slice(1).map((value, index) => value - values[index]);
        const latestChange = snapshot.observation_change != null && Number.isFinite(Number(snapshot.observation_change))
            ? Number(snapshot.observation_change)
            : moves.at(-1);
        signedMove = latestChange * config.biasByAsset[assetId];
        scale = standardDeviation(moves.slice(0, -1));
        if (!scale) scale = standardDeviation(moves);
    }

    if (!Number.isFinite(signedMove) || !Number.isFinite(scale) || scale <= 0) return null;
    const zScore = signedMove / scale;
    const strength = Math.min(35, 35 * Math.tanh(Math.abs(zScore) / 2));
    const bullish = Math.round(50 + Math.sign(zScore) * strength);
    return { bullish, bearish: 100 - bullish };
}

function renderDirectionalBias(record) {
    const target = role(record.card, 'bias');
    if (record.config.biasMode === 'context') {
        target.textContent = 'Context only · no directional score';
        return;
    }
    const activeAssets = selectedAssetFilter.assets || record.config.assets;
    const lines = activeAssets
        .filter(asset => record.config.assets.includes(asset))
        .map(asset => {
            const score = directionalBias(record, asset);
            if (!score) return `${ASSET_LABELS[asset]} · unavailable`;
            return `${ASSET_LABELS[asset]} · ${score.bullish}% bullish / ${score.bearish}% bearish`;
        });
    target.textContent = lines.length ? lines.join('  ·  ') : 'No directional reading for this asset';
}

function applyAssetFilter(assetId) {
    const filter = ASSET_FILTERS.find(item => item.id === assetId) || ASSET_FILTERS[0];
    selectedAssetFilter = filter;
    for (const record of records.values()) {
        record.card.hidden = Boolean(filter.assets && !filter.assets.some(asset => record.config.assets.includes(asset)));
        if (record.snapshot) renderDirectionalBias(record);
    }
    for (const button of assetFilters.querySelectorAll('button[data-asset-filter]')) {
        button.setAttribute('aria-pressed', String(button.dataset.assetFilter === filter.id));
    }
}

function role(card, name) {
    return card.querySelector(`[data-role="${name}"]`);
}

function formatRetrievedAt(seconds) {
    if (!Number.isFinite(Number(seconds))) return 'Unavailable';
    const date = new Date(Number(seconds) * 1_000);
    return Number.isNaN(date.getTime())
        ? 'Unavailable'
        : date.toLocaleString(undefined, {
            year: 'numeric', month: 'short', day: 'numeric',
            hour: 'numeric', minute: '2-digit', timeZoneName: 'short',
        });
}

function formatObservationDate(dateString, frequency) {
    if (frequency?.startsWith('Monthly') && /^\d{4}-\d{2}-\d{2}$/.test(dateString || '')) {
        const date = new Date(`${dateString}T00:00:00Z`);
        return date.toLocaleDateString(undefined, { month: 'long', year: 'numeric', timeZone: 'UTC' });
    }
    return dateString || 'Unavailable';
}

function renderObservationsTable(record, observations) {
    const table = role(record.card, 'observations-table');
    const recent = record.config.chartUsesAllObservations
        ? observations.slice(0, 20)
        : observations.slice(-20).reverse();
    table.replaceChildren(...recent.map(row => {
        const tableRow = document.createElement('tr');
        const dateCell = document.createElement('td');
        const valueCell = document.createElement('td');
        dateCell.textContent = formatObservationDate(row.date, record.snapshot.frequency);
        const classification = row.classification ? ` · ${row.classification}` : '';
        valueCell.textContent = `${formatValue(row.value, record.snapshot.value_format)}${classification}`;
        tableRow.append(dateCell, valueCell);
        return tableRow;
    }));
}

function renderSnapshot(record, data) {
    record.snapshot = data;
    const card = record.card;
    const valueFormat = data.value_format || 'percent';
    const observationDate = formatObservationDate(data.observation_date, data.frequency);

    card.setAttribute('aria-label', data.title);
    role(card, 'title').textContent = data.title;
    role(card, 'description').textContent = record.config.summary || data.description;
    role(card, 'latest').textContent = formatValue(data.latest_value, valueFormat);
    const latestClassification = data.latest_classification || data.observations?.at(-1)?.classification;
    const classificationElement = role(card, 'latest-classification');
    classificationElement.textContent = latestClassification || '';
    classificationElement.hidden = !latestClassification;
    role(card, 'observation-date').textContent = `${record.config.observationLabel || 'Observation period'}: ${observationDate}`;
    role(card, 'change').textContent = formatObservationChange(
        data.observation_change ?? (data.daily_change_basis_points == null ? null : data.daily_change_basis_points / 100),
        data.latest_value,
        valueFormat,
        data.change_period || 'from prior observation',
    );
    role(card, 'change-unit').textContent = changeUnit(valueFormat);
    role(card, 'retrieved-at').textContent = formatRetrievedAt(data.retrieved_at_seconds);
    role(card, 'source').textContent = data.source;
    role(card, 'source-series').textContent = data.source_series_id;
    role(card, 'frequency-unit').textContent = `${data.frequency} · ${data.unit}`;
    role(card, 'source-definition').textContent = data.description;
    role(card, 'limitations').textContent = data.limitations || 'Observations may be revised by the source.';
    renderDirectionalBias(record);

    const sourceLink = role(card, 'source-link');
    sourceLink.href = data.source_url;
    sourceLink.textContent = `Open ${data.source} series`;
    const attributionLink = role(card, 'attribution-link');
    attributionLink.href = data.source_url;
    attributionLink.textContent = data.source;

    const freshness = role(card, 'freshness');
    freshness.textContent = data.freshness === 'stale' ? 'Stale data' : 'Current data';
    freshness.className = `state-badge ${data.freshness === 'stale' ? 'error' : 'valid'}`;
    const staleNote = role(card, 'stale-note');
    staleNote.hidden = !data.status_note && !data.refresh_error;
    staleNote.textContent = [data.status_note, data.refresh_error].filter(Boolean).join(' ');
    role(card, 'chart-heading').textContent = record.config.chartHeading;
    role(card, 'chart-caption').textContent = `${data.frequency} · ${data.unit}`;
    renderChart(record);
    renderObservationsTable(record, data.observations);

    const status = role(card, 'status');
    status.hidden = data.freshness !== 'stale';
    if (data.freshness === 'stale') {
        status.textContent = `Stale · latest observation ${observationDate}.`;
        status.dataset.tone = 'error';
    } else {
        status.textContent = '';
        status.dataset.tone = 'ready';
    }
}

function changeUnit(valueFormat) {
    if (valueFormat === 'idr_per_usd') return 'IDR per USD · percentage shown';
    if (valueFormat === 'usd_flow') return 'USD net flow';
    if (valueFormat === 'index_0_100') return 'Index points';
    if (valueFormat === 'index_points') return 'Index points';
    if (valueFormat === 'usd_per_barrel') return 'USD per barrel';
    if (valueFormat === 'million_barrels') return 'Million barrels';
    return 'Basis points';
}

function renderChart(record) {
    const data = record.snapshot;
    if (!data) return;
    const rangeControls = role(record.card, 'range-controls');
    const observations = record.config.chartUsesAllObservations
        ? data.observations
        : observationsForRange(data.observations, record.selectedRange);
    const chart = role(record.card, 'chart');
    chart.innerHTML = renderMacroChart(
        observations,
        data.title,
        data.value_format || 'percent',
        record.config.referenceLine,
    );
    chart.setAttribute('aria-label', record.config.chartUsesAllObservations
        ? `${data.title} levels by options expiration date`
        : `${data.title} history, ${record.selectedRange} range`);
    rangeControls.querySelectorAll('button[data-range]').forEach(button => {
        button.setAttribute('aria-pressed', String(button.dataset.range === record.selectedRange));
    });
}

function renderUnavailable(record, message) {
    role(record.card, 'latest').textContent = 'Unavailable';
    role(record.card, 'latest-classification').textContent = '';
    role(record.card, 'latest-classification').hidden = true;
    role(record.card, 'observation-date').textContent = 'No observation loaded';
    role(record.card, 'change').textContent = 'Unavailable';
    role(record.card, 'change-unit').textContent = 'No prior observation';
    role(record.card, 'bias').textContent = 'Directional bias unavailable';
    role(record.card, 'retrieved-at').textContent = 'Unavailable';
    role(record.card, 'freshness').textContent = 'Unavailable';
    role(record.card, 'freshness').className = 'state-badge error';
    role(record.card, 'chart').innerHTML = '<p class="macro-chart-empty">No observations are available yet. Retry this indicator to try again.</p>';
    role(record.card, 'stale-note').hidden = true;
    role(record.card, 'status').textContent = `${message} Retry this indicator or use Refresh all.`;
    role(record.card, 'status').dataset.tone = 'error';
    role(record.card, 'status').hidden = false;
}

function updatePageStatus() {
    const loaded = [...records.values()].filter(record => record.snapshot).length;
    const stale = [...records.values()].filter(record => record.snapshot?.freshness === 'stale').length;
    const unavailable = [...records.values()].filter(record => record.failed && !record.snapshot).length;
    const refreshFailures = [...records.values()].filter(record => record.failed && record.snapshot).length;
    const total = records.size;
    if (loaded === total && stale === 0 && unavailable === 0 && refreshFailures === 0) {
        statusElement.textContent = `All ${total} indicators current.`;
        statusElement.dataset.tone = 'ready';
    } else {
        const notes = [];
        if (loaded < total) notes.push(`${loaded} of ${total} loaded`);
        if (stale) notes.push(`${stale} stale`);
        if (unavailable) notes.push(`${unavailable} unavailable`);
        if (refreshFailures) notes.push(`${refreshFailures} refresh failed`);
        statusElement.textContent = `Macro data: ${notes.join(' · ')}.`;
        statusElement.dataset.tone = 'error';
    }
}

async function loadLatestSnapshot(id, refresh) {
    if (id === TIPS_ID) {
        try {
            return await loadTreasurySnapshot();
        } catch (treasuryError) {
            const fallback = await requestJson(apiUrl(id, refresh));
            const fallbackNote = `The direct Treasury feed could not be loaded; using the dashboard API response. ${treasuryError?.message || ''}`.trim();
            return { ...fallback, status_note: [fallback.status_note, fallbackNote].filter(Boolean).join(' ') };
        }
    }
    return requestJson(apiUrl(id, refresh));
}

function apiUrl(id, refresh) {
    const query = new URLSearchParams({ id });
    if (refresh) query.set('refresh', '1');
    return `/api/macro/indicators?${query.toString()}`;
}

async function loadData(record, { refresh = false } = {}) {
    const refreshButton = record.card.querySelector('[data-action="refresh"]');
    refreshButton.disabled = true;
    role(record.card, 'status').textContent = refresh ? 'Checking latest data…' : 'Loading…';
    role(record.card, 'status').dataset.tone = 'loading';
    role(record.card, 'status').hidden = !refresh;
    record.failed = false;
    try {
        const data = await loadLatestSnapshot(record.config.id, refresh);
        renderSnapshot(record, data);
    } catch (error) {
        record.failed = true;
        const message = error?.message || 'The data source could not be reached.';
        if (record.snapshot) {
            const freshness = role(record.card, 'freshness');
            freshness.textContent = 'Refresh failed';
            freshness.className = 'state-badge error';
            role(record.card, 'stale-note').hidden = false;
            role(record.card, 'stale-note').textContent = `Refresh failed. Showing prior data from ${formatObservationDate(record.snapshot.observation_date, record.snapshot.frequency)}. ${message}`;
            role(record.card, 'status').textContent = 'Refresh failed · showing prior data.';
            role(record.card, 'status').dataset.tone = 'error';
            role(record.card, 'status').hidden = false;
        } else {
            renderUnavailable(record, message);
        }
    } finally {
        refreshButton.disabled = false;
        updatePageStatus();
    }
}

function initializeCard(config) {
    const card = cardTemplate.content.firstElementChild.cloneNode(true);
    card.id = `macro-indicator-${config.id}`;
    card.dataset.macroId = config.id;
    role(card, 'category').textContent = config.category;
    role(card, 'title').textContent = config.fallbackTitle;
    role(card, 'chart-heading').textContent = config.chartHeading;
    if (config.source) {
        const attributionLink = role(card, 'attribution-link');
        attributionLink.href = config.sourceUrl;
        attributionLink.textContent = config.source;
    }
    const record = { config, card, snapshot: null, failed: false, selectedRange: '1y' };
    const refreshButton = card.querySelector('[data-action="refresh"]');
    const rangeControls = role(card, 'range-controls');
    if (config.chartUsesAllObservations) rangeControls.hidden = true;
    if (config.hideChange) {
        role(card, 'change-metric').hidden = true;
        role(card, 'latest-label').textContent = config.latestLabel;
    }

    refreshButton.addEventListener('click', () => loadData(record, { refresh: true }));
    rangeControls.addEventListener('click', event => {
        const button = event.target.closest('button[data-range]');
        if (!button || !rangeControls.contains(button)) return;
        record.selectedRange = button.dataset.range;
        renderChart(record);
    });

    indicatorGrid.append(card);
    records.set(config.id, record);
    return record;
}

const initializedRecords = INDICATORS.map(initializeCard);

assetFilters.addEventListener('click', event => {
    const button = event.target.closest('button[data-asset-filter]');
    if (!button || !assetFilters.contains(button)) return;
    applyAssetFilter(button.dataset.assetFilter);
});

refreshButton.addEventListener('click', async () => {
    refreshButton.disabled = true;
    await Promise.all(initializedRecords.map(record => loadData(record, { refresh: true })));
    refreshButton.disabled = false;
});

updatePageStatus();
Promise.all(initializedRecords.map(record => loadData(record)));
