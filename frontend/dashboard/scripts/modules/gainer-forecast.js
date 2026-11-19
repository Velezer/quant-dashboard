import { requestJson } from './api.js';

const FORECAST_URL = '/api/entry-validation/gainer-forecast';
const MAX_SCORE = 60;
const SHORTLIST_LIMIT = 5;
const REQUEST_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 60_000;
const FORECAST_STATUSES = new Set([
    'SCHEDULED',
    'READY',
    'INSUFFICIENT_COVERAGE',
    'NO_POSITIVE_CANDIDATES',
]);
const SCHEDULE_STATUSES = new Set([
    'scheduled',
    'running',
    'waiting_for_coverage',
    'complete',
    'error',
]);
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

function optionalEpoch(value, field) {
    if (value === undefined || value === null) return null;
    if (!Number.isSafeInteger(value) || value <= 0) {
        throw new Error('The forecast response has an invalid ' + field + '.');
    }
    return value;
}

function normalizeSchedule(schedule) {
    if (!schedule || typeof schedule !== 'object' || Array.isArray(schedule)
        || !SCHEDULE_STATUSES.has(schedule.status)) {
        throw new Error('The service returned an invalid forecast schedule.');
    }
    const attemptCount = countValue(schedule.attempt_count, 'scheduled scan attempt');
    const maxAttempts = countValue(schedule.max_attempts_per_window, 'maximum scheduled scan attempt');
    if (maxAttempts < 1 || attemptCount > maxAttempts) {
        throw new Error('The forecast response has inconsistent scheduled scan attempts.');
    }
    const forecastCutoff = optionalEpoch(schedule.forecast_cutoff, 'scheduled cutoff');
    if (forecastCutoff === null) throw new Error('The service omitted the scheduled cutoff.');
    return {
        status: schedule.status,
        attempt_count: attemptCount,
        max_attempts_per_window: maxAttempts,
        last_attempt_at: optionalEpoch(schedule.last_attempt_at, 'last scan time'),
        next_scan_at: optionalEpoch(schedule.next_scan_at, 'next scan time'),
        last_error: optionalText(schedule.last_error, 'scheduled scan error'),
        forecast_cutoff: forecastCutoff,
    };
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
    if (typeof candidate.score !== 'number' || !Number.isFinite(candidate.score)
        || candidate.score <= 0 || candidate.score > MAX_SCORE) {
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
    if (!payload || typeof payload !== 'object' || Array.isArray(payload)
        || !FORECAST_STATUSES.has(payload.status)) {
        throw new Error('The service returned an unknown forecast state.');
    }
    const schedule = normalizeSchedule(payload.schedule);
    if (payload.status === 'SCHEDULED') {
        return {
            status: 'SCHEDULED',
            candidates: [],
            schedule,
        };
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

    const candidates = payload.status === 'READY'
        ? payload.candidates.slice(0, SHORTLIST_LIMIT).map(normalizeCandidate)
        : [];
    if (payload.status === 'READY' && candidates.length === 0) {
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
        candidate_count: candidates.length,
        model: payload.model.trim(),
        validation_status: payload.validation_status || null,
        excluded_counts: excludedCounts,
        schedule,
    };
}

export function initializeGainerForecast(doc = document) {
    const $ = id => doc.getElementById(id);
    const section = $('gainer-forecast');
    const status = $('gainer-forecast-status');
    const results = $('gainer-forecast-results');
    if (!section || !status || !results) return;

    const win = doc.defaultView || globalThis;
    const dateFormatter = new Intl.DateTimeFormat('en-GB', {
        timeZone: 'Asia/Jakarta',
        dateStyle: 'medium',
        timeStyle: 'short',
        hourCycle: 'h23',
    });
    let requestController = null;
    let latestPayload = null;
    let pollTimer = null;
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

    function setBusy(busy) {
        results.setAttribute('aria-busy', String(busy));
        section.setAttribute('aria-busy', String(busy));
    }

    function formatDate(epochSeconds) {
        if (!Number.isSafeInteger(epochSeconds) || epochSeconds <= 0) return 'Unavailable';
        return dateFormatter.format(new Date(epochSeconds * 1000)) + ' WIB';
    }

    function formatAge(epochSeconds) {
        const seconds = Math.max(0, Math.floor(Date.now() / 1000) - epochSeconds);
        if (seconds < 60) return 'just now';
        if (seconds < 3600) return Math.floor(seconds / 60) + 'm ago';
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor((seconds % 3600) / 60);
        return hours + 'h' + (minutes ? ' ' + minutes + 'm' : '') + ' ago';
    }

    function clearTimer(name) {
        if (name === 'poll' && pollTimer !== null) win.clearTimeout(pollTimer);
        if (name === 'expiry' && expiryTimer !== null) win.clearTimeout(expiryTimer);
        if (name === 'poll') pollTimer = null;
        else expiryTimer = null;
    }

    function appendMeta(meta, label, value) {
        const item = element('div');
        item.append(element('dt', '', label), element('dd', '', value));
        meta.append(item);
    }

    function scheduleLabel(schedule) {
        return {
            scheduled: 'Scheduled',
            running: 'Running',
            waiting_for_coverage: 'Waiting for more history',
            complete: 'Complete',
            error: 'Retry planned',
        }[schedule.status];
    }

    function scheduleMessage(schedule) {
        if (schedule.status === 'running') return 'The scheduled forecast scan is running now.';
        if (schedule.status === 'waiting_for_coverage') return 'Coverage is short. The service will retry the scheduled scan.';
        if (schedule.status === 'error') return schedule.last_error
            ? 'The scheduled scan failed: ' + schedule.last_error
            : 'The scheduled scan failed and a retry is planned.';
        if (schedule.status === 'complete') return 'No retry is pending; the next scan follows the next 4h close.';
        return 'The next scheduled scan is queued.';
    }

    function nextScanText(schedule) {
        if (schedule.status === 'running') return 'In progress';
        return schedule.next_scan_at ? formatDate(schedule.next_scan_at) : 'No retry pending';
    }

    function nextScanMessage(schedule, label = 'Next scan') {
        if (schedule.status === 'running') return ' A scheduled scan is in progress.';
        return ' ' + label + ': ' + nextScanText(schedule) + '.';
    }

    function statusFor(payload, expired) {
        if (payload.status === 'SCHEDULED') {
            const next = payload.schedule.status === 'running' ? '' : nextScanMessage(payload.schedule);
            return {
                label: scheduleLabel(payload.schedule),
                tone: 'pending',
                message: scheduleMessage(payload.schedule) + next,
            };
        }
        if (expired) {
            return {
                label: 'Forecast window ended',
                tone: 'pending',
                message: 'The forecast window ended at ' + formatDate(payload.forecast_until)
                    + '. The candidate list is hidden.' + nextScanMessage(payload.schedule, 'Next scheduled scan'),
            };
        }
        if (payload.status === 'READY') {
            return {
                label: 'Coverage ready',
                tone: 'valid',
                message: 'The scheduled scan met the coverage minimum.' + nextScanMessage(payload.schedule),
            };
        }
        if (payload.status === 'INSUFFICIENT_COVERAGE') {
            const more = Math.max(0, payload.minimum_scored_count - payload.scored_count);
            const need = more ? ' ' + more.toLocaleString('en-US') + ' more pairs must score.' : '';
            const retry = nextScanMessage(
                payload.schedule,
                payload.schedule.status === 'waiting_for_coverage' ? 'Coverage retry' : 'Next scheduled scan',
            );
            return {
                label: 'Insufficient coverage',
                tone: 'pending',
                message: 'Coverage is below the service minimum, so no candidate ranking is shown.' + need + retry,
            };
        }
        return {
            label: 'No positive candidates',
            tone: 'pending',
            message: 'Coverage completed, but no positive-scoring candidates qualified.' + nextScanMessage(payload.schedule),
        };
    }

    function appendSchedule(payload, host) {
        const { schedule } = payload;
        const meta = element('dl', 'gainer-forecast-meta');
        appendMeta(meta, 'Scheduled scan', scheduleLabel(schedule));
        appendMeta(meta, 'Scheduled cutoff', formatDate(schedule.forecast_cutoff));
        if (schedule.attempt_count > 0) {
            appendMeta(meta, 'Attempts this window', schedule.attempt_count + ' of ' + schedule.max_attempts_per_window);
        }
        if (schedule.last_attempt_at) appendMeta(meta, 'Last scan attempt', formatDate(schedule.last_attempt_at));
        appendMeta(meta, schedule.status === 'running' ? 'Current scan' : 'Next scheduled scan', nextScanText(schedule));
        host.append(meta);
        if (payload.status === 'SCHEDULED') {
            const message = schedule.status === 'error'
                ? 'No saved forecast is available yet. The background worker will retry this scan.'
                : schedule.status === 'complete'
                    ? 'No saved forecast is available yet. The next scan is scheduled after the next completed 4h window.'
                    : 'No saved forecast is available yet. The background scan will publish its result here.';
            host.append(element('p', 'gainer-forecast-empty', message));
        }
        if (schedule.last_error) {
            host.append(element('p', 'gainer-forecast-exclusions', 'Latest scan error: ' + schedule.last_error));
        }
    }

    function appendCoverage(payload, host) {
        if (payload.status === 'SCHEDULED') return;
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
        if (payload.status === 'INSUFFICIENT_COVERAGE') {
            const additional = Math.max(0, payload.minimum_scored_count - payload.scored_count);
            host.append(element(
                'p',
                'gainer-forecast-exclusions',
                additional.toLocaleString('en-US') + ' more instruments must have current native history before the ranking is shown.',
            ));
        }
        const exclusions = OPTIONAL_COVERAGE_COUNTS
            .filter(([field]) => Object.hasOwn(payload.excluded_counts, field))
            .map(([field, label]) => label + ' ' + payload.excluded_counts[field].toLocaleString('en-US'));
        if (exclusions.length) host.append(element('p', 'gainer-forecast-exclusions', 'Excluded from scoring: ' + exclusions.join(' · ') + '.'));
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
        if (payload.status === 'SCHEDULED') return;
        if (expired) {
            host.append(element('p', 'gainer-forecast-empty', 'This completed 4h forecast window has ended. Its candidate list is hidden until a new scheduled result is ready.'));
            return;
        }
        if (payload.status !== 'READY' || !payload.candidates.length) {
            const empty = payload.status === 'INSUFFICIENT_COVERAGE'
                ? 'The minimum coverage was not met. No ranked candidates are displayed.'
                : 'No positive-scoring candidate qualified for this completed 4h window.';
            host.append(element('p', 'gainer-forecast-empty', empty));
            return;
        }

        const grid = element('div', 'gainer-forecast-candidate-grid');
        const top = payload.candidates[0];
        const topCard = element('article', 'gainer-forecast-top');
        topCard.append(element('span', 'gainer-forecast-top-label', 'Top candidate'));
        topCard.append(element('h3', '', candidateName(top)));
        topCard.append(element('strong', 'gainer-forecast-score', formatScore(top.score)));
        appendCandidateMeta(topCard, top);
        grid.append(topCard);

        const shortlist = element('section', 'gainer-forecast-shortlist');
        shortlist.append(element('h4', '', 'Shortlist · next candidates'));
        const remaining = payload.candidates.slice(1, SHORTLIST_LIMIT);
        if (!remaining.length) {
            shortlist.append(element('p', 'gainer-forecast-empty', 'No additional positive-scoring candidates qualified in this window.'));
        } else {
            const list = element('ol');
            for (const candidate of remaining) {
                const row = element('li');
                const line = element('div');
                line.append(element('strong', '', candidateName(candidate)), element('span', '', formatScore(candidate.score)));
                row.append(line);
                const detail = [];
                if (candidate.quote_currency) detail.push('Quote ' + candidate.quote_currency);
                detail.push(sourceName(candidate));
                if (candidate.instrument_id) detail.push(candidate.instrument_id);
                row.append(element('small', '', detail.join(' · ')));
                list.append(row);
            }
            shortlist.append(list);
        }
        grid.append(shortlist);
        host.append(grid);
    }

    function render(payload) {
        const expired = payload.status !== 'SCHEDULED' && Date.now() >= payload.forecast_until * 1000;
        const resultState = statusFor(payload, expired);
        results.replaceChildren();
        const header = element('div', 'gainer-forecast-result-header');
        header.append(element('h3', '', 'Latest scheduled 4h snapshot'));
        header.append(element('span', 'state-badge ' + resultState.tone, resultState.label));
        results.append(header);
        appendCandidates(payload, results, expired);
        appendCoverage(payload, results);
        appendSchedule(payload, results);
        setStatus(resultState.message, payload.status === 'READY' && !expired ? 'ready' : 'pending');
        clearTimer('expiry');
        if (payload.status !== 'SCHEDULED' && !expired) {
            const remaining = payload.forecast_until * 1000 - Date.now();
            expiryTimer = win.setTimeout(() => {
                expiryTimer = null;
                if (latestPayload === payload) render(payload);
            }, Math.max(0, remaining + 25));
        }
    }

    function schedulePoll() {
        clearTimer('poll');
        pollTimer = win.setTimeout(() => {
            pollTimer = null;
            if (doc.hidden) schedulePoll();
            else void refreshForecast({ quiet: true });
        }, POLL_INTERVAL_MS);
    }

    async function refreshForecast({ quiet = false } = {}) {
        if (requestController) return;
        const controller = new AbortController();
        requestController = controller;
        const timeout = win.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
        if (!quiet) {
            setBusy(true);
            setStatus('Loading the latest saved scheduled forecast…', 'loading');
        }
        try {
            const payload = normalizeGainerForecast(await requestJson(FORECAST_URL, {
                signal: controller.signal,
                cache: 'no-store',
            }));
            if (requestController !== controller) return;
            latestPayload = payload;
            render(payload);
        } catch (error) {
            if (requestController !== controller) return;
            const detail = error?.name === 'AbortError'
                ? 'The scheduled result request timed out.'
                : String(error?.message || error).slice(0, 240);
            setStatus('Could not load the scheduled forecast: ' + detail + ' It will retry automatically.', 'error');
        } finally {
            win.clearTimeout(timeout);
            if (requestController === controller) requestController = null;
            setBusy(false);
            schedulePoll();
        }
    }

    doc.addEventListener('visibilitychange', () => {
        if (!doc.hidden) void refreshForecast({ quiet: true });
    });
    win.addEventListener?.('pagehide', () => {
        clearTimer('poll');
        clearTimer('expiry');
        requestController?.abort();
    });
    setBusy(false);
    void refreshForecast();
}

if (typeof document !== 'undefined') initializeGainerForecast(document);
