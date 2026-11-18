import { requestJson } from './api.js';

const FORECAST_URL = '/api/entry-validation/gainer-forecast';
const MAX_SCORE = 60;
const SHORTLIST_LIMIT = 5;
const REQUEST_TIMEOUT_MS = 30_000;
const FORECAST_STATUSES = new Set(['READY', 'INSUFFICIENT_COVERAGE', 'NO_POSITIVE_CANDIDATES']);
const OPTIONAL_COVERAGE_COUNTS = [
    ['missing_or_stale_count', 'Missing or stale'],
    ['invalid_history_count', 'Invalid history'],
    ['read_error_count', 'Read errors'],
];

function countValue(value, field) {
    if (!Number.isSafeInteger(value) || value < 0) {
        throw new Error('The forecast response has an invalid ' + field + ' count.');
    }
    return value;
}

function optionalText(value, field) {
    if (value === undefined || value === null) return null;
    if (typeof value !== 'string' || !value.trim()) {
        throw new Error('The forecast response has an invalid ' + field + '.');
    }
    return value.trim();
}

function normalizeCandidate(candidate) {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        throw new Error('The forecast response contains an invalid candidate.');
    }
    if (typeof candidate.symbol !== 'string' || !candidate.symbol.trim()) {
        throw new Error('A forecast candidate is missing its symbol.');
    }
    if (typeof candidate.source_family !== 'string' || !candidate.source_family.trim()) {
        throw new Error('A forecast candidate is missing its source family.');
    }
    if (typeof candidate.score !== 'number' || !Number.isFinite(candidate.score) || candidate.score <= 0 || candidate.score > MAX_SCORE) {
        throw new Error('A forecast candidate must have a positive score within the model’s /60 scale.');
    }
    if (candidate.source_family !== 'tokocrypto_native') {
        throw new Error('A forecast candidate does not use native Tokocrypto Spot history.');
    }
    if (candidate.validation_status !== undefined && candidate.validation_status !== 'UNVALIDATED_RESEARCH') {
        throw new Error('A forecast candidate did not identify itself as unvalidated research.');
    }

    return {
        symbol: candidate.symbol.trim(),
        score: candidate.score,
        source_family: candidate.source_family.trim(),
        venue_symbol: optionalText(candidate.venue_symbol, 'venue symbol'),
        quote_currency: optionalText(candidate.quote_currency, 'quote currency'),
        instrument_id: optionalText(candidate.instrument_id, 'instrument identity'),
    };
}

export function normalizeGainerForecast(payload) {
    if (!payload || typeof payload !== 'object' || Array.isArray(payload) || !FORECAST_STATUSES.has(payload.status)) {
        throw new Error('The service returned an unknown forecast state.');
    }
    if (!Number.isSafeInteger(payload.as_of) || payload.as_of <= 0
        || !Number.isSafeInteger(payload.forecast_until) || payload.forecast_until <= payload.as_of) {
        throw new Error('The service returned an invalid completed-window time.');
    }
    if (typeof payload.model !== 'string' || !payload.model.trim()) {
        throw new Error('The service did not identify the forecast model.');
    }
    if (payload.validation_status !== undefined && payload.validation_status !== 'UNVALIDATED_RESEARCH') {
        throw new Error('The service did not identify the forecast as unvalidated research.');
    }
    if (!Array.isArray(payload.candidates)) {
        throw new Error('The forecast response is missing its candidate list.');
    }

    const eligibleCount = countValue(payload.eligible_count, 'eligible instrument');
    const scoredCount = countValue(payload.scored_count, 'scored instrument');
    const minimumScoredCount = countValue(payload.minimum_scored_count, 'minimum scored instrument');
    if (scoredCount > eligibleCount) {
        throw new Error('The forecast coverage counts are inconsistent.');
    }

    const coverageReady = scoredCount >= minimumScoredCount;
    if ((payload.status === 'INSUFFICIENT_COVERAGE') === coverageReady) {
        throw new Error('The forecast state does not match its scored coverage.');
    }

    const candidateCount = payload.candidates.length;
    const candidates = payload.status === 'READY'
        ? payload.candidates.slice(0, SHORTLIST_LIMIT).map(normalizeCandidate)
        : [];
    if (payload.status === 'READY' && candidateCount === 0) {
        throw new Error('The service marked the forecast ready without a top candidate.');
    }

    const excludedCounts = {};
    for (const [field] of OPTIONAL_COVERAGE_COUNTS) {
        if (payload[field] !== undefined && payload[field] !== null) {
            excludedCounts[field] = countValue(payload[field], field);
        }
    }

    return {
        status: payload.status,
        as_of: payload.as_of,
        forecast_until: payload.forecast_until,
        eligible_count: eligibleCount,
        scored_count: scoredCount,
        minimum_scored_count: minimumScoredCount,
        candidates,
        candidate_count: candidateCount,
        model: payload.model.trim(),
        validation_status: payload.validation_status || null,
        excluded_counts: excludedCounts,
    };
}

export function initializeGainerForecast(doc = document) {
    const $ = id => doc.getElementById(id);
    const section = $('gainer-forecast');
    const runButton = $('gainer-forecast-run');
    const cancelButton = $('gainer-forecast-cancel');
    const retryButton = $('gainer-forecast-retry');
    const status = $('gainer-forecast-status');
    const results = $('gainer-forecast-results');
    if (!section || !runButton || !cancelButton || !retryButton || !status || !results) return;

    const win = doc.defaultView || globalThis;
    const dateFormatter = new Intl.DateTimeFormat('en-GB', {
        timeZone: 'Asia/Jakarta',
        dateStyle: 'medium',
        timeStyle: 'short',
        hourCycle: 'h23',
    });
    let requestController = null;
    let retryAvailable = false;
    let abortRequested = false;
    let latestPayload = null;
    let expiryTimer = null;

    function element(tagName, className = '', value = '') {
        const node = doc.createElement(tagName);
        if (className) node.className = className;
        if (value !== '') node.textContent = String(value);
        return node;
    }

    function setStatus(message, tone = '') {
        status.textContent = message;
        if (tone) status.dataset.tone = tone;
        else delete status.dataset.tone;
    }

    function setControls() {
        const busy = requestController !== null;
        runButton.disabled = busy;
        cancelButton.hidden = !busy;
        cancelButton.disabled = !busy || abortRequested;
        retryButton.hidden = busy || !retryAvailable;
        retryButton.disabled = busy;
        results.setAttribute('aria-busy', String(busy));
        section.setAttribute('aria-busy', String(busy));
    }

    function formatDate(epochSeconds) {
        const date = new Date(epochSeconds * 1000);
        return Number.isFinite(date.getTime()) ? dateFormatter.format(date) + ' WIB' : 'Unavailable';
    }

    function formatAge(epochSeconds) {
        const seconds = Math.max(0, Math.floor(Date.now() / 1000) - epochSeconds);
        if (seconds < 60) return 'just now';
        if (seconds < 3600) return Math.floor(seconds / 60) + 'm ago';
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor((seconds % 3600) / 60);
        return hours + 'h' + (minutes ? ' ' + minutes + 'm' : '') + ' ago';
    }

    function clearExpiryTimer() {
        if (expiryTimer !== null) win.clearTimeout(expiryTimer);
        expiryTimer = null;
    }

    function statusFor(payload, expired) {
        if (expired) {
            return {
                label: 'Forecast window ended',
                tone: 'pending',
                message: 'The completed 4h window ended at ' + formatDate(payload.forecast_until) + '. The candidate is hidden. Run the forecast again to check the latest completed window.',
            };
        }
        if (payload.status === 'READY') {
            return {
                label: 'Coverage ready',
                tone: 'valid',
                message: 'Coverage met the service minimum. The top candidate and shortlist are shown for this completed window.',
            };
        }
        if (payload.status === 'INSUFFICIENT_COVERAGE') {
            return {
                label: 'Insufficient coverage',
                tone: 'pending',
                message: 'Coverage is below the service minimum, so no candidate ranking is shown. Sync native Tokocrypto history on the screener, then retry.',
            };
        }
        return {
            label: 'No positive candidates',
            tone: 'pending',
            message: 'Coverage completed, but no positive-scoring candidates qualified in this window. Run again after a newer 4h window is available.',
        };
    }

    function appendMeta(meta, label, value) {
        const item = element('div');
        const term = element('dt', '', label);
        const description = element('dd', '', value);
        item.append(term, description);
        meta.append(item);
    }

    function appendCoverage(payload, host) {
        const meta = element('dl', 'gainer-forecast-meta');
        appendMeta(meta, 'Completed cutoff', formatDate(payload.as_of) + ' · ' + formatAge(payload.as_of));
        appendMeta(meta, 'Forecast window ends', formatDate(payload.forecast_until));
        appendMeta(meta, 'Model', payload.model);
        appendMeta(meta, 'Scored / eligible instruments', payload.scored_count.toLocaleString('en-US') + ' / ' + payload.eligible_count.toLocaleString('en-US'));
        appendMeta(meta, 'Minimum scored count', payload.minimum_scored_count.toLocaleString('en-US'));
        if (payload.eligible_count > 0) {
            const percentage = payload.scored_count / payload.eligible_count * 100;
            appendMeta(meta, 'Scored coverage', percentage.toLocaleString('en-US', { maximumFractionDigits: 1 }) + '%');
        }
        host.append(meta);

        const exclusions = OPTIONAL_COVERAGE_COUNTS
            .filter(([field]) => Object.hasOwn(payload.excluded_counts, field))
            .map(([field, label]) => label + ' ' + payload.excluded_counts[field].toLocaleString('en-US'));
        if (exclusions.length) {
            const line = element('p', 'gainer-forecast-exclusions', 'Excluded from scoring: ' + exclusions.join(' · ') + '.');
            host.append(line);
        }
    }

    function candidateName(candidate) {
        return candidate.venue_symbol || candidate.symbol;
    }

    function sourceName(candidate) {
        return candidate.source_family === 'tokocrypto_native'
            ? 'Tokocrypto native Spot'
            : candidate.source_family;
    }

    function formatScore(score) {
        return score.toLocaleString('en-US', { maximumFractionDigits: 2 }) + ' /60';
    }

    function appendCandidateMeta(host, candidate) {
        const parts = ['Source: ' + sourceName(candidate)];
        if (candidate.quote_currency) parts.push('Quote: ' + candidate.quote_currency);
        if (candidate.instrument_id) parts.push('Instrument: ' + candidate.instrument_id);
        const meta = element('p', 'gainer-forecast-candidate-meta');
        for (const part of parts) meta.append(element('span', '', part));
        host.append(meta);
    }

    function appendCandidates(payload, host, expired) {
        const candidates = payload.candidates;
        if (expired) {
            host.append(element('p', 'gainer-forecast-empty', payload.status === 'READY'
                ? 'This completed 4h forecast window has ended. The candidate list is hidden; run again for the latest completed window.'
                : 'This completed 4h forecast window has ended. Run again to check the latest completed window.'));
            return;
        }
        if (payload.status !== 'READY' || !candidates.length) {
            const empty = payload.status === 'INSUFFICIENT_COVERAGE'
                ? 'The minimum coverage was not met. No ranked candidates are displayed.'
                : 'No positive-scoring candidate qualified for this completed 4h window.';
            host.append(element('p', 'gainer-forecast-empty', empty));
            return;
        }

        const grid = element('div', 'gainer-forecast-candidate-grid');
        const top = candidates[0];
        const topCard = element('article', 'gainer-forecast-top');
        topCard.append(element('span', 'gainer-forecast-top-label', 'Top candidate'));
        topCard.append(element('h3', '', candidateName(top)));
        topCard.append(element('strong', 'gainer-forecast-score', formatScore(top.score)));
        appendCandidateMeta(topCard, top);
        grid.append(topCard);

        const shortlist = element('section', 'gainer-forecast-shortlist');
        shortlist.append(element('h4', '', 'Shortlist · next candidates'));
        const remaining = candidates.slice(1, SHORTLIST_LIMIT);
        if (!remaining.length) {
            shortlist.append(element('p', 'gainer-forecast-empty', 'No additional positive-scoring candidates qualified in this window.'));
        } else {
            const list = element('ol');
            for (const candidate of remaining) {
                const row = element('li');
                const line = element('div');
                line.append(element('strong', '', candidateName(candidate)));
                line.append(element('span', '', formatScore(candidate.score)));
                row.append(line);
                const detail = [];
                if (candidate.quote_currency) detail.push('Quote ' + candidate.quote_currency);
                detail.push(sourceName(candidate));
                if (candidate.instrument_id) detail.push(candidate.instrument_id);
                row.append(element('small', '', detail.join(' · ')));
                list.append(row);
            }
            shortlist.append(list);
            if (payload.candidate_count > SHORTLIST_LIMIT) {
                shortlist.append(element('p', 'gainer-forecast-exclusions', 'Showing the next ' + remaining.length + ' of ' + (payload.candidate_count - 1).toLocaleString('en-US') + ' candidates.'));
            }
        }
        grid.append(shortlist);
        host.append(grid);
    }

    function render(payload) {
        const expired = Date.now() >= payload.forecast_until * 1000;
        const resultState = statusFor(payload, expired);
        results.replaceChildren();
        const header = element('div', 'gainer-forecast-result-header');
        header.append(element('h3', '', 'Latest aligned 4h snapshot'));
        const badge = element('span', 'state-badge ' + resultState.tone, resultState.label);
        header.append(badge);
        results.append(header);

        appendCandidates(payload, results, expired);
        appendCoverage(payload, results);
        setStatus(resultState.message, expired || payload.status !== 'READY' ? 'pending' : 'ready');
        retryAvailable = payload.status === 'INSUFFICIENT_COVERAGE' && !expired;
        setControls();
    }

    function scheduleExpiry(payload) {
        clearExpiryTimer();
        const remaining = payload.forecast_until * 1000 - Date.now();
        expiryTimer = win.setTimeout(() => {
            if (latestPayload !== payload) return;
            if (Date.now() < payload.forecast_until * 1000) {
                scheduleExpiry(payload);
                return;
            }
            expiryTimer = null;
            render(payload);
        }, Math.max(0, remaining + 20));
    }

    async function runForecast() {
        if (requestController) return;
        clearExpiryTimer();
        latestPayload = null;
        retryAvailable = false;
        abortRequested = false;
        results.replaceChildren();
        setStatus('Requesting the latest completed, aligned Tokocrypto Spot 4h window…', 'loading');

        const controller = new AbortController();
        requestController = controller;
        setControls();
        let timedOut = false;
        const timeout = win.setTimeout(() => {
            timedOut = true;
            controller.abort();
        }, REQUEST_TIMEOUT_MS);

        try {
            const response = normalizeGainerForecast(await requestJson(FORECAST_URL, {
                signal: controller.signal,
                cache: 'no-store',
            }));
            if (requestController !== controller) return;
            latestPayload = response;
            render(response);
            scheduleExpiry(response);
        } catch (error) {
            if (requestController !== controller) return;
            if (error?.name === 'AbortError') {
                setStatus(
                    timedOut
                        ? 'The forecast request timed out. Check service or market-data recovery, then retry.'
                        : 'The request was canceled before a result returned. Run the forecast again when ready.',
                    timedOut ? 'error' : 'pending',
                );
                retryAvailable = timedOut;
            } else {
                const detail = String(error?.message || error).slice(0, 240);
                setStatus('Forecast unavailable: ' + detail + ' Retry after the service or history recovers.', 'error');
                retryAvailable = true;
            }
        } finally {
            win.clearTimeout(timeout);
            if (requestController === controller) {
                requestController = null;
                abortRequested = false;
                setControls();
            }
        }
    }

    function cancelForecast() {
        if (!requestController || abortRequested) return;
        abortRequested = true;
        setStatus('Canceling the forecast request…', 'loading');
        setControls();
        requestController.abort();
    }

    runButton.addEventListener('click', () => { void runForecast(); });
    cancelButton.addEventListener('click', cancelForecast);
    retryButton.addEventListener('click', () => { void runForecast(); });
    doc.addEventListener('visibilitychange', () => {
        if (!doc.hidden && latestPayload) render(latestPayload);
    });
    win.addEventListener?.('pagehide', () => {
        clearExpiryTimer();
        requestController?.abort();
    });
    setControls();

    if (new URLSearchParams(win.location?.search || '').get('runForecast') === '1') {
        void runForecast();
    }
}

if (typeof document !== 'undefined') initializeGainerForecast(document);
