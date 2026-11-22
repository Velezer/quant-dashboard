import { requestJson } from './api.js';
import { formatObservationChange, formatValue, observationsForRange, renderMacroChart } from './macro-indicator-view.js';
import { loadTreasurySnapshot } from './macro-indicator-treasury.js';

const TIPS_ID = 'ten_year_tips_real_yield';
const INDICATORS = Object.freeze([
    { id: TIPS_ID, category: 'REAL INTEREST RATE · UNITED STATES', fallbackTitle: '10-Year TIPS real yield', chartHeading: 'Yield history' },
    { id: 'two_year_treasury_nominal_yield', category: 'NOMINAL INTEREST RATE · UNITED STATES', fallbackTitle: '2-Year U.S. Treasury yield', chartHeading: 'Yield history' },
    { id: 'usd_idr_daily_reference', category: 'FOREIGN EXCHANGE · INDONESIA', fallbackTitle: 'USD/IDR exchange rate', chartHeading: 'Exchange-rate history' },
]);

const statusElement = document.getElementById('macro-status');
const refreshButton = document.getElementById('macro-refresh');
const indicatorGrid = document.getElementById('macro-indicator-grid');
const cardTemplate = document.getElementById('macro-series-template');
const records = new Map();

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
    const recent = observations.slice(-20).reverse();
    table.replaceChildren(...recent.map(row => {
        const tableRow = document.createElement('tr');
        const dateCell = document.createElement('td');
        const valueCell = document.createElement('td');
        dateCell.textContent = formatObservationDate(row.date, record.snapshot.frequency);
        valueCell.textContent = formatValue(row.value, record.snapshot.value_format);
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
    role(card, 'description').textContent = data.description;
    role(card, 'latest').textContent = formatValue(data.latest_value, valueFormat);
    role(card, 'observation-date').textContent = `Observation period: ${observationDate}`;
    role(card, 'change').textContent = formatObservationChange(
        data.observation_change ?? (data.daily_change_basis_points == null ? null : data.daily_change_basis_points / 100),
        data.latest_value,
        valueFormat,
        data.change_period || 'from prior observation',
    );
    role(card, 'change-unit').textContent = valueFormat === 'idr_per_usd' ? 'IDR per USD · percentage shown' : 'Basis points';
    role(card, 'retrieved-at').textContent = formatRetrievedAt(data.retrieved_at_seconds);
    role(card, 'source').textContent = data.source;
    role(card, 'source-series').textContent = data.source_series_id;
    role(card, 'frequency-unit').textContent = `${data.frequency} · ${data.unit}`;
    role(card, 'source-definition').textContent = data.description;
    role(card, 'limitations').textContent = data.limitations || 'Observations may be revised by the source.';

    const sourceLink = role(card, 'source-link');
    sourceLink.href = data.source_url;
    sourceLink.textContent = `Open ${data.source} series`;

    const freshness = role(card, 'freshness');
    freshness.textContent = data.freshness === 'stale' ? 'Stale data' : 'Current data';
    freshness.className = `state-badge ${data.freshness === 'stale' ? 'error' : 'valid'}`;
    const staleNote = role(card, 'stale-note');
    staleNote.hidden = !data.status_note && !data.refresh_error;
    staleNote.textContent = [data.status_note, data.refresh_error].filter(Boolean).join(' ');
    role(card, 'chart-heading').textContent = record.config.chartHeading;
    role(card, 'chart-caption').textContent = `${data.frequency} · ${data.unit} · latest observation ${observationDate}`;
    role(card, 'data-limits').textContent = 'The observation period is shown separately from the source check time. Macro values are descriptive context and are not a prediction or an execution signal.';
    renderChart(record);
    renderObservationsTable(record, data.observations);

    const status = role(card, 'status');
    if (data.freshness === 'stale') {
        status.textContent = `Loaded ${data.title}; the latest observation is ${observationDate}. Review the stale-data note before relying on it.`;
        status.dataset.tone = 'error';
    } else {
        const retrievalText = data.fetch_status === 'cached'
            ? `Last retrieved at ${formatRetrievedAt(data.retrieved_at_seconds)}.`
            : data.fetch_status === 'stale'
                ? `Showing the last available observations from ${formatRetrievedAt(data.retrieved_at_seconds)}.`
                : `Retrieved at ${formatRetrievedAt(data.retrieved_at_seconds)}.`;
        status.textContent = `Loaded ${data.title}; latest observation ${observationDate}. ${retrievalText}`;
        status.dataset.tone = 'ready';
    }
}

function renderChart(record) {
    const data = record.snapshot;
    if (!data) return;
    const rangeControls = role(record.card, 'range-controls');
    const observations = observationsForRange(data.observations, record.selectedRange);
    const chart = role(record.card, 'chart');
    chart.innerHTML = renderMacroChart(observations, data.title, data.value_format || 'percent');
    chart.setAttribute('aria-label', `${data.title} history, ${record.selectedRange} range`);
    rangeControls.querySelectorAll('button[data-range]').forEach(button => {
        button.setAttribute('aria-pressed', String(button.dataset.range === record.selectedRange));
    });
}

function renderUnavailable(record, message) {
    role(record.card, 'latest').textContent = 'Unavailable';
    role(record.card, 'observation-date').textContent = 'No observation loaded';
    role(record.card, 'change').textContent = 'Unavailable';
    role(record.card, 'change-unit').textContent = 'No prior observation';
    role(record.card, 'retrieved-at').textContent = 'Unavailable';
    role(record.card, 'freshness').textContent = 'Unavailable';
    role(record.card, 'freshness').className = 'state-badge error';
    role(record.card, 'chart').innerHTML = '<p class="macro-chart-empty">No observations are available yet. Retry this indicator to try again.</p>';
    role(record.card, 'stale-note').hidden = true;
    role(record.card, 'status').textContent = `${message} Retry this indicator or use Refresh all.`;
    role(record.card, 'status').dataset.tone = 'error';
}

function updatePageStatus() {
    const loaded = [...records.values()].filter(record => record.snapshot).length;
    const stale = [...records.values()].filter(record => record.snapshot?.freshness === 'stale').length;
    const unavailable = [...records.values()].filter(record => record.failed && !record.snapshot).length;
    const refreshFailures = [...records.values()].filter(record => record.failed && record.snapshot).length;
    const total = records.size;
    if (loaded === total && stale === 0 && unavailable === 0 && refreshFailures === 0) {
        statusElement.textContent = `All ${total} macro indicators are current. Each card shows its source and observation period.`;
        statusElement.dataset.tone = 'ready';
    } else {
        const notes = [];
        if (loaded < total) notes.push(`${loaded} of ${total} loaded`);
        if (stale) notes.push(`${stale} stale`);
        if (unavailable) notes.push(`${unavailable} unavailable`);
        if (refreshFailures) notes.push(`${refreshFailures} refresh failed`);
        statusElement.textContent = `Macro indicators: ${notes.join(', ')}. Each available chart remains independent and can be refreshed separately.`;
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
    role(record.card, 'status').textContent = refresh ? 'Checking the latest published observation…' : 'Loading observations…';
    role(record.card, 'status').dataset.tone = 'loading';
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
            role(record.card, 'stale-note').textContent = `The update failed. Showing the last loaded observation (${formatObservationDate(record.snapshot.observation_date, record.snapshot.frequency)}). ${message}`;
            role(record.card, 'status').textContent = 'The latest macro data could not be refreshed. The previously loaded observation remains visible.';
            role(record.card, 'status').dataset.tone = 'error';
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
    card.dataset.macroId = config.id;
    role(card, 'category').textContent = config.category;
    role(card, 'title').textContent = config.fallbackTitle;
    role(card, 'chart-heading').textContent = config.chartHeading;
    const record = { config, card, snapshot: null, failed: false, selectedRange: '1y' };
    const refreshButton = card.querySelector('[data-action="refresh"]');
    const rangeControls = role(card, 'range-controls');

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

refreshButton.addEventListener('click', async () => {
    refreshButton.disabled = true;
    await Promise.all(initializedRecords.map(record => loadData(record, { refresh: true })));
    refreshButton.disabled = false;
});

updatePageStatus();
Promise.all(initializedRecords.map(record => loadData(record)));
