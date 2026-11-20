import { requestJson } from './api.js';
import { formatBasisPoints, formatYield, observationsForRange, renderYieldChart } from './macro-indicator-view.js';
import { FRED_SNAPSHOT_URL, parseFredSnapshotCsv } from './macro-indicator-snapshot.js';

const statusElement = document.getElementById('macro-status');
const refreshButton = document.getElementById('macro-refresh');
const retryButton = document.getElementById('macro-retry');
const emptyState = document.getElementById('macro-empty');
const emptyMessage = document.getElementById('macro-empty-message');
const card = document.getElementById('macro-series-card');
const freshnessBadge = document.getElementById('macro-freshness');
const chartElement = document.getElementById('macro-chart');
const chartControls = document.getElementById('macro-range-controls');
let snapshot = null;
let selectedRange = '1y';

function setStatus(message, tone = 'loading') {
    statusElement.textContent = message;
    statusElement.dataset.tone = tone;
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

function renderObservationsTable(observations) {
    const table = document.getElementById('macro-observations-table');
    const recent = observations.slice(-20).reverse();
    table.replaceChildren(...recent.map(row => {
        const tableRow = document.createElement('tr');
        const dateCell = document.createElement('td');
        const valueCell = document.createElement('td');
        dateCell.textContent = row.date;
        valueCell.textContent = formatYield(row.value);
        tableRow.append(dateCell, valueCell);
        return tableRow;
    }));
}

function renderSnapshot(data) {
    snapshot = data;
    emptyState.hidden = true;
    card.hidden = false;
    document.getElementById('macro-series-title').textContent = data.title;
    document.getElementById('macro-description').textContent = data.description;
    document.getElementById('macro-latest-value').textContent = formatYield(data.latest_value);
    document.getElementById('macro-observation-date').textContent = `Observation date: ${data.observation_date}`;
    document.getElementById('macro-daily-change').textContent = formatBasisPoints(data.daily_change_basis_points);
    document.getElementById('macro-retrieved-at').textContent = formatRetrievedAt(data.retrieved_at_seconds);
    document.getElementById('macro-source').textContent = data.source;
    document.getElementById('macro-source-series').textContent = data.source_series_id;
    document.getElementById('macro-frequency-unit').textContent = `${data.frequency} · ${data.unit}`;
    document.getElementById('macro-source-definition').textContent = data.description;
    const sourceLink = document.getElementById('macro-source-link');
    sourceLink.href = data.source_url;
    freshnessBadge.textContent = data.freshness === 'stale' ? 'Stale data' : 'Current data';
    freshnessBadge.className = `state-badge ${data.freshness === 'stale' ? 'error' : 'valid'}`;
    const staleNote = document.getElementById('macro-stale-note');
    staleNote.hidden = !data.status_note && !data.refresh_error;
    staleNote.textContent = [data.status_note, data.refresh_error].filter(Boolean).join(' ');
    document.getElementById('macro-chart-caption').textContent = `${data.frequency} · ${data.unit} · last observation ${data.observation_date}`;
    renderChart();
    renderObservationsTable(data.observations);
}

function renderChart() {
    if (!snapshot) return;
    const observations = observationsForRange(snapshot.observations, selectedRange);
    chartElement.innerHTML = renderYieldChart(observations, snapshot.title);
    chartElement.setAttribute('aria-label', `${snapshot.title} history, ${selectedRange} range`);
}

async function loadLatestSnapshot() {
    let snapshotError;
    try {
        const response = await fetch(`${FRED_SNAPSHOT_URL}?v=${Date.now()}`, {
            cache: 'no-store',
            headers: { Accept: 'text/plain' },
        });
        if (!response.ok) throw new Error(`FRED snapshot returned HTTP ${response.status}.`);
        return parseFredSnapshotCsv(await response.text());
    } catch (error) {
        snapshotError = error;
    }

    const deployedSnapshot = await requestJson('/api/macro/indicators');
    const fallbackNote = `The latest FRED snapshot could not be loaded; showing the last snapshot bundled with this dashboard. ${snapshotError?.message || ''}`.trim();
    return {
        ...deployedSnapshot,
        freshness: 'stale',
        fetch_status: 'stale',
        status_note: [deployedSnapshot.status_note, fallbackNote].filter(Boolean).join(' '),
    };
}

async function loadData({ refresh = false } = {}) {
    refreshButton.disabled = true;
    retryButton.disabled = true;
    emptyState.hidden = true;
    setStatus(refresh ? 'Checking for the latest published FRED snapshot…' : 'Loading the latest available 10-year TIPS real yield…');
    try {
        const data = await loadLatestSnapshot();
        renderSnapshot(data);
        const freshnessText = data.freshness === 'stale'
            ? 'FRED data is stale. Review the status note; the dashboard updates when FRED publishes a newer snapshot.'
            : `Loaded ${data.title}; the latest observation is ${data.observation_date}.`;
        const retrievalText = data.fetch_status === 'cached'
            ? `Showing the FRED snapshot last retrieved at ${formatRetrievedAt(data.retrieved_at_seconds)}.`
            : data.fetch_status === 'stale'
                ? `Showing the last available FRED snapshot from ${formatRetrievedAt(data.retrieved_at_seconds)}.`
                : `FRED snapshot retrieved at ${formatRetrievedAt(data.retrieved_at_seconds)}.`;
        setStatus(`${freshnessText} ${retrievalText}`, data.freshness === 'stale' ? 'error' : 'ready');
    } catch (error) {
        const message = error?.message || 'FRED could not be reached.';
        if (snapshot) {
            freshnessBadge.textContent = 'Refresh failed';
            freshnessBadge.className = 'state-badge error';
            const staleNote = document.getElementById('macro-stale-note');
            staleNote.hidden = false;
            staleNote.textContent = `The update failed. Showing the last loaded data (${snapshot.observation_date}). ${message}`;
            setStatus('The latest macro data could not be refreshed. The previously loaded observation remains visible.', 'error');
        } else {
            emptyState.hidden = false;
            emptyMessage.textContent = `Macroeconomic data is unavailable right now. ${message}`;
            setStatus('FRED data could not be loaded. Retry to try again.', 'error');
        }
    } finally {
        refreshButton.disabled = false;
        retryButton.disabled = false;
    }
}

refreshButton.addEventListener('click', () => loadData({ refresh: true }));
retryButton.addEventListener('click', () => loadData({ refresh: true }));
chartControls.addEventListener('click', event => {
    const button = event.target.closest('button[data-range]');
    if (!button || !chartControls.contains(button)) return;
    selectedRange = button.dataset.range;
    chartControls.querySelectorAll('button[data-range]').forEach(control => {
        control.setAttribute('aria-pressed', String(control === button));
    });
    renderChart();
});

loadData();
