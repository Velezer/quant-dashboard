import { requestJson } from './api.js';

const FORECAST_URL = '/api/entry-validation/gainer-forecast';
const SHORTLIST_LIMIT = 5;
const REQUEST_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 60_000;
const DAY_SECONDS = 24 * 60 * 60;
const MAX_STOCKBIT_FORECAST_AGE_SECONDS = 5 * DAY_SECONDS;
const FORECAST_SCOPES = {
    tokocrypto_spot: {
        id: 'tokocrypto_spot',
        label: 'Tokocrypto Spot',
        protocol: 'tokocrypto_native_momentum_top_rank_next_24h_return_forecast_v2',
        targetKind: 'calendar_24h',
        target: 'Next 24h close-to-close return (%)',
        horizonSeconds: DAY_SECONDS,
        source: 'tokocrypto_native',
        provider: 'tokocrypto',
        priceBasis: 'venue_native',
        scanIntervalSeconds: 4 * 60 * 60,
        scanLabel: 'after each completed 4h close',
    },
    stockbit_idx: {
        id: 'stockbit_idx',
        label: 'Stockbit IDX',
        protocol: 'stockbit_yahoo_momentum_top_rank_next_idx_session_return_forecast_v2',
        targetKind: 'next_idx_session',
        target: 'Next available IDX session close-to-close return (%)',
        horizonSeconds: null,
        source: 'stockbit_regular_yfinance_fallback',
        provider: 'yfinance',
        priceBasis: 'yahoo_unadjusted',
        scanIntervalSeconds: DAY_SECONDS,
        scanLabel: 'at 14:00 WIB for the next IDX session',
    },
};
const FORECAST_STATUSES = new Set([
    'SCHEDULED',
    'READY',
    'INSUFFICIENT_COVERAGE',
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

function normalizeSchedule(schedule, scope) {
    if (!schedule || typeof schedule !== 'object' || Array.isArray(schedule)
        || !SCHEDULE_STATUSES.has(schedule.status)) {
        throw new Error('The service returned an invalid forecast schedule.');
    }
    const attemptCount = countValue(schedule.attempt_count, 'scheduled scan attempt');
    const maxAttempts = countValue(schedule.max_attempts_per_window, 'maximum scheduled scan attempt');
    const scanInterval = countValue(schedule.scan_interval_seconds, 'scheduled scan interval');
    if (maxAttempts < 1 || attemptCount > maxAttempts || scanInterval !== scope.scanIntervalSeconds) {
        throw new Error('The forecast response has inconsistent scheduled scan settings.');
    }
    if (schedule.scope_id !== scope.id) throw new Error('The schedule belongs to another forecast scope.');
    const forecastCutoff = optionalEpoch(schedule.forecast_cutoff, 'scheduled cutoff');
    if (forecastCutoff === null) throw new Error('The service omitted the scheduled cutoff.');
    if (typeof schedule.target_window_passed !== 'boolean') {
        throw new Error('The service omitted the target session freshness state.');
    }
    return {
        status: schedule.status,
        attempt_count: attemptCount,
        max_attempts_per_window: maxAttempts,
        scan_interval_seconds: scanInterval,
        last_attempt_at: optionalEpoch(schedule.last_attempt_at, 'last scan time'),
        next_scan_at: optionalEpoch(schedule.next_scan_at, 'next scan time'),
        last_error: optionalText(schedule.last_error, 'scheduled scan error'),
        forecast_cutoff: forecastCutoff,
        target_window_passed: schedule.target_window_passed,
    };
}

function normalizeCandidate(candidate, scope) {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        throw new Error('The forecast response contains an invalid candidate.');
    }
    if (typeof candidate.symbol !== 'string' || !candidate.symbol.trim()) {
        throw new Error('A forecast candidate is missing its symbol.');
    }
    if (candidate.source_family !== scope.source || candidate.data_provider !== scope.provider
        || candidate.price_basis !== scope.priceBasis) {
        throw new Error('A forecast candidate does not match its market data source.');
    }
    if (typeof candidate.forecast_return_pct !== 'number' || !Number.isFinite(candidate.forecast_return_pct)) {
        throw new Error('A forecast candidate must have a finite projected return.');
    }
    if (candidate.validation_status !== undefined && candidate.validation_status !== 'UNVALIDATED_RESEARCH') {
        throw new Error('A forecast candidate did not identify itself as unvalidated research.');
    }
    return {
        symbol: candidate.symbol.trim(),
        forecast_return_pct: candidate.forecast_return_pct,
        source_family: candidate.source_family,
        data_provider: candidate.data_provider,
        price_basis: candidate.price_basis,
        venue_symbol: optionalText(candidate.venue_symbol, 'venue symbol'),
        quote_currency: optionalText(candidate.quote_currency, 'quote currency'),
        instrument_id: optionalText(candidate.instrument_id, 'instrument identity'),
        reference_bar_time: optionalEpoch(candidate.reference_bar_time, 'reference candle time'),
    };
}

function normalizeOutcomes(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
        return { sample_count: 0, mean_realized_return_pct: null, positive_return_rate_pct: null, latest_settled_at: null };
    }
    const sampleCount = countValue(value.sample_count, 'settled outcome');
    const mean = value.mean_realized_return_pct;
    const positiveRate = value.positive_return_rate_pct;
    if (mean !== null && mean !== undefined && (typeof mean !== 'number' || !Number.isFinite(mean))) {
        throw new Error('The forecast response has an invalid mean realized return.');
    }
    if (positiveRate !== null && positiveRate !== undefined
        && (typeof positiveRate !== 'number' || !Number.isFinite(positiveRate) || positiveRate < 0 || positiveRate > 100)) {
        throw new Error('The forecast response has an invalid positive outcome rate.');
    }
    return {
        sample_count: sampleCount,
        mean_realized_return_pct: mean ?? null,
        positive_return_rate_pct: positiveRate ?? null,
        latest_settled_at: optionalEpoch(value.latest_settled_at, 'latest settled outcome'),
    };
}

export function normalizeGainerForecast(payload, scopeId = 'tokocrypto_spot') {
    const scope = FORECAST_SCOPES[scopeId];
    if (!scope) throw new Error('Choose a supported forecast scope.');
    if (!payload || typeof payload !== 'object' || Array.isArray(payload)
        || !FORECAST_STATUSES.has(payload.status)) {
        throw new Error('The service returned an unknown forecast state.');
    }
    if (payload.scope_id !== scope.id || payload.protocol_id !== scope.protocol
        || payload.target_kind !== scope.targetKind || payload.forecast_target !== scope.target
        || payload.source_family !== scope.source || payload.data_provider !== scope.provider
        || payload.price_basis !== scope.priceBasis) {
        throw new Error('The service returned data for a different forecast profile.');
    }
    const schedule = normalizeSchedule(payload.schedule, scope);
    const outcomes = normalizeOutcomes(payload.observed_outcomes);
    if (payload.status === 'SCHEDULED') {
        return {
            scope: scope.id,
            status: 'SCHEDULED',
            candidates: [],
            provisional_candidate: null,
            model: payload.model,
            model_description: payload.model_description,
            schedule,
            outcomes,
        };
    }
    if (!Number.isSafeInteger(payload.as_of) || payload.as_of <= 0) {
        throw new Error('The service returned an invalid forecast cutoff.');
    }
    const forecastUntil = optionalEpoch(payload.forecast_until, 'forecast end');
    if (scope.horizonSeconds === null) {
        if (forecastUntil !== null || payload.forecast_horizon_seconds !== null) {
            throw new Error('The Stockbit forecast must use a next-session target.');
        }
    } else if (forecastUntil !== payload.as_of + scope.horizonSeconds
        || payload.forecast_horizon_seconds !== scope.horizonSeconds) {
        throw new Error('The service returned an invalid 24h forecast window.');
    }
    if (typeof payload.model !== 'string' || !payload.model.trim()
        || typeof payload.model_description !== 'string' || !payload.model_description.trim()) {
        throw new Error('The service did not identify the forecast model.');
    }
    if (payload.validation_status !== 'UNVALIDATED_RESEARCH') {
        throw new Error('The service did not identify the forecast as unvalidated research.');
    }
    if (!Array.isArray(payload.candidates)) throw new Error('The forecast response is missing its candidate list.');

    const eligibleCount = countValue(payload.eligible_count, 'eligible instrument');
    const forecastedCount = countValue(payload.forecasted_count, 'forecasted instrument');
    const minimumForecastedCount = countValue(payload.minimum_forecasted_count, 'minimum forecasted instrument');
    if (forecastedCount > eligibleCount) throw new Error('The forecast coverage counts are inconsistent.');
    const coverageReady = forecastedCount >= minimumForecastedCount;
    if ((payload.status === 'INSUFFICIENT_COVERAGE') === coverageReady) {
        throw new Error('The forecast state does not match its history coverage.');
    }
    const candidates = payload.status === 'READY'
        ? payload.candidates.slice(0, SHORTLIST_LIMIT).map(candidate => normalizeCandidate(candidate, scope))
        : [];
    if (payload.status === 'READY' && candidates.length === 0) {
        throw new Error('The service marked the forecast ready without a top candidate.');
    }
    const provisionalCandidate = payload.provisional_candidate === undefined || payload.provisional_candidate === null
        ? null
        : normalizeCandidate(payload.provisional_candidate, scope);
    if (payload.status === 'READY' && provisionalCandidate !== null) {
        throw new Error('A full-coverage forecast cannot include a provisional candidate.');
    }
    const excludedCounts = {};
    for (const [field] of OPTIONAL_COVERAGE_COUNTS) {
        if (payload[field] !== undefined && payload[field] !== null) {
            excludedCounts[field] = countValue(payload[field], field);
        }
    }
    return {
        scope: scope.id,
        status: payload.status,
        as_of: payload.as_of,
        forecast_until: forecastUntil,
        eligible_count: eligibleCount,
        forecasted_count: forecastedCount,
        minimum_forecasted_count: minimumForecastedCount,
        candidates,
        candidate_count: candidates.length,
        provisional_candidate: provisionalCandidate,
        model: payload.model.trim(),
        model_description: payload.model_description.trim(),
        forecast_target: payload.forecast_target,
        forecast_horizon_seconds: payload.forecast_horizon_seconds,
        source_family: payload.source_family,
        data_provider: payload.data_provider,
        price_basis: payload.price_basis,
        validation_status: payload.validation_status,
        excluded_counts: excludedCounts,
        schedule,
        outcomes,
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
    let currentScope = 'tokocrypto_spot';
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

    function scopeConfig(payload) {
        return FORECAST_SCOPES[payload.scope];
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

    function scheduleMessage(schedule, scope) {
        if (schedule.status === 'running') return 'The scheduled forecast scan is running now.';
        if (schedule.status === 'waiting_for_coverage') return 'Coverage is short. The service will retry this forecast.';
        if (schedule.status === 'error') return schedule.last_error
            ? 'The scheduled scan failed: ' + schedule.last_error
            : 'The scheduled scan failed and a retry is planned.';
        if (schedule.status === 'complete') return 'The next scan runs ' + scope.scanLabel + '.';
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

    function isExpired(payload) {
        if (payload.status === 'SCHEDULED') return false;
        if (payload.scope === 'tokocrypto_spot') {
            return Date.now() >= payload.forecast_until * 1000;
        }
        return payload.schedule.target_window_passed
            || payload.as_of < payload.schedule.forecast_cutoff
            || Date.now() / 1000 - payload.as_of > MAX_STOCKBIT_FORECAST_AGE_SECONDS;
    }

    function statusFor(payload, expired) {
        const scope = scopeConfig(payload);
        if (payload.status === 'SCHEDULED') {
            const next = payload.schedule.status === 'running' ? '' : nextScanMessage(payload.schedule);
            return {
                label: scheduleLabel(payload.schedule),
                tone: 'pending',
                message: scheduleMessage(payload.schedule, scope) + next,
            };
        }
        if (expired) {
            const targetEnd = payload.forecast_until ? ' at ' + formatDate(payload.forecast_until) : '';
            return {
                label: 'Forecast is stale',
                tone: 'pending',
                message: 'The saved forecast is outside its freshness window' + targetEnd
                    + (payload.status === 'READY'
                        ? '. Its last ticker remains visible as a stale research reference.'
                        : payload.provisional_candidate
                            ? '. Its provisional top ticker remains visible as a stale partial-coverage reference.'
                        : '. No current ticker is available until a covered scan is ready.')
                    + nextScanMessage(payload.schedule, 'Next scheduled scan'),
            };
        }
        if (payload.status === 'READY') {
            const topReturn = payload.candidates[0]?.forecast_return_pct;
            return {
                label: topReturn > 0 ? 'Forecast ready' : 'Top-ranked ticker',
                tone: topReturn > 0 ? 'valid' : 'pending',
                message: (topReturn > 0
                    ? 'The scheduled scan met its coverage minimum and ranks the highest projected return.'
                    : 'No positive return is projected. Showing the top-ranked ticker with the highest non-positive return.'
                ) + nextScanMessage(payload.schedule),
            };
        }
        if (payload.status === 'INSUFFICIENT_COVERAGE') {
            const more = Math.max(0, payload.minimum_forecasted_count - payload.forecasted_count);
            const need = more ? ' ' + more.toLocaleString('en-US') + ' more instruments need current history.' : '';
            const provisional = payload.provisional_candidate;
            const retry = nextScanMessage(
                payload.schedule,
                payload.schedule.status === 'waiting_for_coverage' ? 'Coverage retry' : 'Next scheduled scan',
            );
            return {
                label: provisional ? 'Provisional top ticker' : 'Insufficient coverage',
                tone: 'pending',
                message: provisional
                    ? 'Showing ' + candidateName(provisional) + ' (' + formatForecastReturn(provisional.forecast_return_pct)
                        + ') as the highest projection among ' + payload.forecasted_count.toLocaleString('en-US')
                        + ' instruments with current history. This is partial coverage, not a full-market winner.' + need + retry
                    : 'Coverage is below the service minimum and no instrument has usable current history yet.' + need + retry,
            };
        }
        return {
            label: 'Forecast unavailable',
            tone: 'pending',
            message: 'No current ticker is available for this forecast window.' + nextScanMessage(payload.schedule),
        };
    }

    function appendSchedule(payload, host) {
        const scope = scopeConfig(payload);
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
            host.append(element('p', 'gainer-forecast-empty',
                'No saved forecast is available yet. The background scan runs ' + scope.scanLabel + '.'));
        }
        if (schedule.last_error) {
            host.append(element('p', 'gainer-forecast-exclusions', 'Latest scan error: ' + schedule.last_error));
        }
    }

    function appendCoverage(payload, host) {
        if (payload.status === 'SCHEDULED') return;
        const scope = scopeConfig(payload);
        const meta = element('dl', 'gainer-forecast-meta');
        appendMeta(meta, 'Forecast cutoff', formatDate(payload.as_of) + ' · ' + formatAge(payload.as_of));
        appendMeta(meta, 'Target', payload.forecast_target);
        if (payload.forecast_until) appendMeta(meta, 'Target window ends', formatDate(payload.forecast_until));
        appendMeta(meta, 'Model', payload.model);
        appendMeta(meta, 'Data source', scope.source === 'tokocrypto_native'
            ? 'Tokocrypto native Spot'
            : 'Yahoo Finance research proxy');
        appendMeta(meta, 'Price basis', scope.priceBasis === 'venue_native' ? 'Venue native' : 'Unadjusted close');
        appendMeta(meta, 'Forecasted / eligible instruments', payload.forecasted_count.toLocaleString('en-US') + ' / ' + payload.eligible_count.toLocaleString('en-US'));
        appendMeta(meta, 'Minimum forecast coverage', payload.minimum_forecasted_count.toLocaleString('en-US'));
        if (payload.eligible_count > 0) {
            const percentage = payload.forecasted_count / payload.eligible_count * 100;
            appendMeta(meta, 'Forecast coverage', percentage.toLocaleString('en-US', { maximumFractionDigits: 1 }) + '%');
        }
        host.append(meta);
        host.append(element('p', 'gainer-forecast-exclusions', payload.model_description));
        if (payload.status === 'INSUFFICIENT_COVERAGE') {
            const additional = Math.max(0, payload.minimum_forecasted_count - payload.forecasted_count);
            host.append(element('p', 'gainer-forecast-exclusions',
                additional.toLocaleString('en-US') + ' more instruments need current profile history before full-market coverage is met.'));
            if (payload.provisional_candidate) {
                host.append(element('p', 'gainer-forecast-exclusions',
                    'The provisional ticker ranks only the instruments with usable current history and may not be the full-market leader.'));
            }
        }
        const exclusions = OPTIONAL_COVERAGE_COUNTS
            .filter(([field]) => Object.hasOwn(payload.excluded_counts, field))
            .map(([field, label]) => label + ' ' + payload.excluded_counts[field].toLocaleString('en-US'));
        if (exclusions.length) host.append(element('p', 'gainer-forecast-exclusions', 'Excluded from forecast: ' + exclusions.join(' · ') + '.'));
    }

    function appendOutcomes(payload, host) {
        const outcomes = payload.outcomes;
        const meta = element('dl', 'gainer-forecast-meta');
        appendMeta(meta, 'Settled Top 5 observations', outcomes.sample_count.toLocaleString('en-US'));
        if (outcomes.mean_realized_return_pct !== null) {
            appendMeta(meta, 'Mean realized gross return', outcomes.mean_realized_return_pct.toLocaleString('en-US', { maximumFractionDigits: 2 }) + '%');
        }
        if (outcomes.positive_return_rate_pct !== null) {
            appendMeta(meta, 'Positive outcome rate', outcomes.positive_return_rate_pct.toLocaleString('en-US', { maximumFractionDigits: 1 }) + '%');
        }
        if (outcomes.latest_settled_at) appendMeta(meta, 'Latest outcome settled', formatDate(outcomes.latest_settled_at));
        host.append(meta);
        host.append(element('p', 'gainer-forecast-exclusions',
            'Prospective observations are descriptive and do not validate this forecast model or include costs.'));
    }

    function candidateName(candidate) {
        return candidate.venue_symbol || candidate.symbol;
    }

    function sourceName(candidate) {
        return candidate.data_provider === 'yfinance'
            ? 'Yahoo Finance research proxy'
            : 'Tokocrypto native Spot';
    }

    function formatForecastReturn(returnPct) {
        const sign = returnPct > 0 ? '+' : '';
        return sign + returnPct.toLocaleString('en-US', { maximumFractionDigits: 2 }) + '%';
    }

    function appendCandidateMeta(host, candidate) {
        const parts = ['Source: ' + sourceName(candidate)];
        if (candidate.quote_currency) parts.push('Quote: ' + candidate.quote_currency);
        if (candidate.instrument_id) parts.push('Instrument: ' + candidate.instrument_id);
        if (candidate.reference_bar_time) parts.push('Reference candle: ' + formatDate(candidate.reference_bar_time));
        const meta = element('p', 'gainer-forecast-candidate-meta');
        for (const part of parts) meta.append(element('span', '', part));
        host.append(meta);
    }

    function appendCandidates(payload, host, expired) {
        const scope = scopeConfig(payload);
        if (payload.status === 'SCHEDULED') return;
        if (expired && payload.status === 'READY' && payload.candidates.length) {
            const expiry = payload.forecast_until
                ? ' at ' + formatDate(payload.forecast_until)
                : payload.schedule.target_window_passed
                    ? ' after the targeted IDX session has closed'
                    : payload.as_of < payload.schedule.forecast_cutoff
                        ? ' while the next IDX scan is pending'
                        : ' after five days without a new IDX forecast';
            host.append(element('p', 'gainer-forecast-exclusions',
                'This forecast is stale' + expiry + '. The last saved candidate list stays visible with its cutoff; it is not a current recommendation.'));
        }
        if (payload.status === 'INSUFFICIENT_COVERAGE' && payload.provisional_candidate) {
            const top = payload.provisional_candidate;
            const topCard = element('article', 'gainer-forecast-top');
            topCard.append(element('span', 'gainer-forecast-top-label',
                expired ? 'Provisional projection · stale' : 'Partial coverage · provisional'));
            topCard.append(element('h3', '', candidateName(top)));
            topCard.append(element('strong', 'gainer-forecast-return', formatForecastReturn(top.forecast_return_pct)));
            appendCandidateMeta(topCard, top);
            host.append(topCard);
            return;
        }
        if (payload.status !== 'READY' || !payload.candidates.length) {
            const empty = payload.status === 'INSUFFICIENT_COVERAGE'
                ? 'No instrument has usable current history yet. The background service will retry automatically.'
                : 'No current ticker is available for the selected target window.';
            host.append(element('p', 'gainer-forecast-empty', empty));
            return;
        }
        const grid = element('div', 'gainer-forecast-candidate-grid');
        const top = payload.candidates[0];
        const topCard = element('article', 'gainer-forecast-top');
        topCard.append(element('span', 'gainer-forecast-top-label',
            expired ? 'Last forecast · stale' : 'Top-ranked ticker'));
        topCard.append(element('h3', '', candidateName(top)));
        topCard.append(element('strong', 'gainer-forecast-return', formatForecastReturn(top.forecast_return_pct)));
        appendCandidateMeta(topCard, top);
        grid.append(topCard);

        const shortlist = element('section', 'gainer-forecast-shortlist');
        shortlist.append(element('h4', '', expired ? 'Other stale projections' : 'Other ranked projections'));
        const remaining = payload.candidates.slice(1, SHORTLIST_LIMIT);
        if (!remaining.length) {
            shortlist.append(element('p', 'gainer-forecast-empty', 'No additional ticker is shown in this shortlist.'));
        } else {
            const list = element('ol');
            for (const rowCandidate of remaining) {
                const row = element('li');
                const line = element('div');
                line.append(element('strong', '', candidateName(rowCandidate)), element('span', '', formatForecastReturn(rowCandidate.forecast_return_pct)));
                row.append(line);
                const details = [];
                if (rowCandidate.quote_currency) details.push('Quote ' + rowCandidate.quote_currency);
                details.push(sourceName(rowCandidate));
                details.push(scope.priceBasis === 'yahoo_unadjusted' ? 'Unadjusted close' : 'Venue native');
                if (rowCandidate.instrument_id) details.push(rowCandidate.instrument_id);
                row.append(element('small', '', details.join(' · ')));
                list.append(row);
            }
            shortlist.append(list);
        }
        grid.append(shortlist);
        host.append(grid);
    }

    function render(payload) {
        const expired = isExpired(payload);
        const scope = scopeConfig(payload);
        const resultState = statusFor(payload, expired);
        results.replaceChildren();
        const header = element('div', 'gainer-forecast-result-header');
        header.append(element('h3', '', 'Latest scheduled ' + scope.label + ' forecast'));
        header.append(element('span', 'state-badge ' + resultState.tone, resultState.label));
        results.append(header);
        appendCandidates(payload, results, expired);
        appendCoverage(payload, results);
        appendOutcomes(payload, results);
        appendSchedule(payload, results);
        setStatus(
            resultState.message,
            payload.status === 'READY' && !expired && payload.candidates[0]?.forecast_return_pct > 0
                ? 'ready'
                : 'pending',
        );
        clearTimer('expiry');
        if (payload.status !== 'SCHEDULED' && !expired) {
            const remaining = payload.scope === 'tokocrypto_spot'
                ? payload.forecast_until * 1000 - Date.now()
                : (payload.as_of + MAX_STOCKBIT_FORECAST_AGE_SECONDS) * 1000 - Date.now();
            expiryTimer = win.setTimeout(() => {
                expiryTimer = null;
                if (latestPayload === payload) render(payload);
            }, Math.max(0, remaining + 25));
        }
    }

    function setSelectedTab(scopeId) {
        for (const scope of Object.values(FORECAST_SCOPES)) {
            const tab = $('gainer-forecast-tab-' + scope.id);
            if (!tab) continue;
            tab.setAttribute('aria-pressed', String(scope.id === scopeId));
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
        const requestedScope = currentScope;
        const controller = new AbortController();
        requestController = controller;
        const timeout = win.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
        if (!quiet) {
            setBusy(true);
            setStatus('Loading the latest saved ' + FORECAST_SCOPES[requestedScope].label + ' forecast…', 'loading');
        }
        try {
            const url = FORECAST_URL + '?scope=' + encodeURIComponent(requestedScope);
            const payload = normalizeGainerForecast(await requestJson(url, {
                signal: controller.signal,
                cache: 'no-store',
            }), requestedScope);
            if (requestController !== controller || requestedScope !== currentScope) return;
            latestPayload = payload;
            render(payload);
        } catch (error) {
            if (requestController !== controller || requestedScope !== currentScope) return;
            const detail = error?.name === 'AbortError'
                ? 'The scheduled result request timed out.'
                : String(error?.message || error).slice(0, 240);
            setStatus('Could not load this scheduled forecast: ' + detail + ' It will retry automatically.', 'error');
        } finally {
            win.clearTimeout(timeout);
            if (requestController === controller) {
                requestController = null;
                setBusy(false);
                schedulePoll();
            }
        }
    }

    for (const scope of Object.values(FORECAST_SCOPES)) {
        const tab = $('gainer-forecast-tab-' + scope.id);
        tab?.addEventListener('click', () => {
            if (currentScope === scope.id) return;
            currentScope = scope.id;
            latestPayload = null;
            clearTimer('poll');
            clearTimer('expiry');
            requestController?.abort();
            requestController = null;
            setSelectedTab(currentScope);
            results.replaceChildren();
            void refreshForecast();
        });
    }

    doc.addEventListener('visibilitychange', () => {
        if (!doc.hidden) void refreshForecast({ quiet: true });
    });
    win.addEventListener?.('pagehide', () => {
        clearTimer('poll');
        clearTimer('expiry');
        requestController?.abort();
    });
    setSelectedTab(currentScope);
    setBusy(false);
    void refreshForecast();
}

if (typeof document !== 'undefined') initializeGainerForecast(document);
