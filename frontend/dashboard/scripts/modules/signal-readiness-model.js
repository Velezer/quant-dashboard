import { APP_LABELS } from './signal-screener-model.js';

const count = value => Number.isSafeInteger(value) && value >= 0 ? value : 0;
const sum = (rows, key) => rows.reduce((total, row) => total + count(row[key]), 0);
const words = value => String(value || '').replaceAll('_', ' ');

function latestAfterFeeMetric(liveScoring, app, product, timeframe) {
    const evaluations = Array.isArray(liveScoring?.latest_evaluations) ? liveScoring.latest_evaluations : [];
    const appId = String(app || '').toLowerCase();
    const productId = String(product || 'all').toLowerCase();
    const wantsFutures = productId !== 'all' && /future|perpetual/i.test(productId);
    const wantsSpot = productId !== 'all' && /spot/i.test(productId);
    const tokenizedStock = productId.includes('tokenized_stock');
    const appCategory = {
        stockbit: /stock/i,
        pintu: /crypto/i,
        pluang: /crypto/i,
        tokocrypto: /crypto|tokenized/i,
    }[appId];
    const candidates = evaluations.filter(evaluation => {
        const scope = evaluation?.scope || {};
        const category = String(scope.asset_category_id || '').toLowerCase();
        if (timeframe !== 'all' && scope.timeframe !== timeframe) return false;
        if (appCategory && !appCategory.test(category)) return false;
        if (tokenizedStock && category !== 'tokenized_stock') return false;
        if (wantsFutures && !/perpetual|future/i.test(category)) return false;
        if (wantsSpot && !/spot/i.test(category)) return false;
        return typeof evaluation?.holdout?.win_rate_after_fees_pct === 'number'
            && Number.isFinite(evaluation.holdout.win_rate_after_fees_pct);
    }).sort((a, b) => Number(b.evaluated_at || 0) - Number(a.evaluated_at || 0));
    const evaluation = candidates[0];
    if (!evaluation) return null;
    const scope = evaluation.scope || {};
    const holdout = evaluation.holdout || {};
    const rate = holdout.win_rate_after_fees_pct;
    const lowerBound = typeof holdout.win_rate_after_fees_wilson_95_lower_pct === 'number'
        ? holdout.win_rate_after_fees_wilson_95_lower_pct : null;
    const trades = typeof holdout.trades === 'number' ? holdout.trades : null;
    const target = Number(evaluation.target_after_fee_win_rate_pct ?? liveScoring?.target_after_fee_win_rate_pct ?? 50);
    if (!Number.isFinite(rate) || !Number.isFinite(target)) return null;
    return {
        label: 'Latest after-fee holdout',
        value: `${rate.toFixed(1)}% after fees · ${rate >= target ? 'met' : 'below'} ${target.toFixed(0)}%${Number.isFinite(trades) ? ` · ${Math.trunc(trades)} trades` : ''}`,
        detail: `${Number.isFinite(lowerBound) ? `95% lower bound ${lowerBound.toFixed(1)}%. ` : ''}${scope.asset_category_id || 'All assets'} · ${String(scope.timeframe || '').toUpperCase()}`,
        highlight: true,
    };
}

export function readinessTime(epochSeconds) {
    const date = new Date(Number(epochSeconds) * 1000);
    return Number(epochSeconds) > 0 && Number.isFinite(date.getTime())
        ? `${new Intl.DateTimeFormat('en-GB', { timeZone: 'Asia/Jakarta', dateStyle: 'medium', timeStyle: 'short' }).format(date)} WIB`
        : 'Not recorded';
}

export function appSignalReadiness(app, payload, summary = {}, selection = {}) {
    const label = APP_LABELS[app] || app;
    const product = selection.product || 'all';
    const timeframe = selection.timeframe || 'all';
    const scopeLabel = `${label} · ${product === 'all' ? 'All products' : words(product)} · ${timeframe === 'all' ? 'All timeframes' : timeframe}`;
    const supportsListingRefresh = ['stockbit', 'pintu', 'tokocrypto'].includes(app);
    const afterFeeMetric = latestAfterFeeMetric(summary.live_scoring, app, product, timeframe);
    const snapshot = summary.readiness;
    const products = (Array.isArray(snapshot?.products) ? snapshot.products : [])
        .filter(row => row.exchange_id === app && (product === 'all' || row.product_id === product));
    const worker = (Array.isArray(snapshot?.workers) ? snapshot.workers : [])
        .find(row => row.exchange_id === app);
    const workerMetric = worker ? {
        label: 'Last completed app worker',
        value: `${String(worker.status || 'unknown').replaceAll('_', ' ')} · ${count(worker.registered_instruments)} tickers`,
    } : null;
    const result = { scopeLabel, title: 'Checking signal availability', tone: 'unknown', message: '', metrics: [],
        examples: [], checkedAt: '', nextStep: '', retryListings: false, hasScanDetails: false };
    if (![1, 2].includes(snapshot?.version) || !(snapshot.checked_at > 0) || !products.length) {
        const listings = (Array.isArray(payload?.products) ? payload.products : [])
            .filter(row => row.exchange_id === app && (product === 'all' || row.product_id === product));
        const registered = sum(listings, 'registered');
        const confirmed = sum(listings, 'confirmed');
        const history = sum(listings, 'with_15m_candles');
        const error = String(payload?.refresh_errors?.[app] || '').slice(0, 250);
        result.title = !listings.length ? 'Detailed data checks are not available yet'
            : !registered ? 'No registered tickers to scan'
            : !confirmed ? 'Tickers are waiting for verification'
            : !history ? 'No stored 15m history found' : 'Freshness has not been checked yet';
        result.message = listings.length
            ? `${label}: ${confirmed} verified of ${registered} registered instruments; ${history} with stored 15m history. `
            : `${label} has no detailed scan diagnostics in this snapshot. `;
        result.message += error ? `Latest listing refresh failed: ${error}. ` : '';
        result.message += !registered && listings.length ? 'No verified listings are available in this selection.'
            : !confirmed && listings.length ? 'Listings pending verification cannot generate signals yet.'
            : !history && confirmed ? 'Market history is being gathered; fresh matching candles are required.'
            : 'Stored candles alone do not confirm fresh prices or a qualifying setup. The next completed scan will provide detailed checks.';
        if (workerMetric) result.metrics = [workerMetric];
        if (afterFeeMetric) result.metrics.push(afterFeeMetric);
        if (count(summary.stale_price_signal_count)) {
            result.title = 'Signal setups retained with stale reference prices';
            result.message += ` ${count(summary.stale_price_signal_count)} active setups remain visible for inspection; stale reference prices make them ineligible as current market entries.`;
        }
        result.retryListings = supportsListingRefresh && Boolean(error || (listings.length && (!registered || !confirmed)));
        result.nextStep = result.retryListings ? 'Retry listings, then scan again after market history is available.'
            : 'Use Refresh signals to run the checks. Candle ingestion continues in the background.';
        if (summary.status?.last_attempt_status === 'error') {
            result.title = 'The latest signal scan failed';
            result.tone = 'error';
            result.message = `Detailed checks are unavailable. ${String(summary.status.last_error || 'The service did not record a detailed error.').slice(0, 250)}`;
        } else if (summary.status?.last_attempt_status === 'waiting') {
            result.title = 'Waiting for the first signal scan';
            result.message += ' The first scan has not completed yet.';
        }
        return result;
    }

    result.hasScanDetails = true;
    const frames = products.flatMap(row => Array.isArray(row.timeframes) ? row.timeframes : [])
        .filter(row => timeframe === 'all' || row.timeframe === timeframe);
    const instruments = sum(products, 'instrument_count');
    const fresh = sum(products, 'fresh_price_count');
    const stale = sum(products, 'stale_price_count');
    const missing = sum(products, 'missing_price_count');
    const errors = sum(products, 'data_error_count');
    const evaluated = sum(frames, 'evaluated_count');
    const qualified = sum(frames, 'qualifying_signal_count');
    const insufficient = sum(frames, 'insufficient_history_count');
    const nweInsufficient = sum(frames, 'nwe_insufficient_history_count');
    const missingFrames = sum(frames, 'missing_history_count');
    const staleFrames = sum(frames, 'stale_history_count');
    const frameErrors = sum(frames, 'data_error_count');
    const mismatched = sum(frames, 'source_mismatch_count');
    const afterFeeChecks = sum(frames, 'after_fee_metric_count');
    const missingBacktestProfile = sum(frames, 'after_fee_profile_missing_count');
    const belowBacktestCutoff = sum(frames, 'after_fee_score_below_cutoff_count');
    const blockers = [stale && `${stale} stale 15m prices`, missing && `${missing} missing 15m prices`,
        errors && `${errors} unreadable price histories`, missingFrames && `${missingFrames} missing timeframe histories`,
        frameErrors && `${frameErrors} unreadable timeframe histories`,
        staleFrames && `${staleFrames} stale timeframe histories`, insufficient && `${insufficient} histories below model warm-up`,
        nweInsufficient && `${nweInsufficient} histories below NWE warm-up`, mismatched && `${mismatched} price/history source mismatches`].filter(Boolean);
    const backtestGateSummary = [
        missingBacktestProfile && `${missingBacktestProfile} setups have no promoted after-fee profile; equal weights were used`,
        belowBacktestCutoff && `${belowBacktestCutoff} setups are below their historical after-fee cutoff but remain shown`,
        afterFeeChecks && !missingBacktestProfile && !belowBacktestCutoff
            && `${afterFeeChecks} setups have no comparable after-fee metric`,
    ].filter(Boolean);
    const firstChecked = Math.min(...products.map(row => Number(row.first_checked_at) || snapshot.scan_started_at || snapshot.checked_at));
    const lastChecked = Math.max(...products.map(row => Number(row.checked_at) || snapshot.checked_at));
    result.checkedAt = `Last completed scan: ${readinessTime(snapshot.checked_at)}. Checks describe inputs when each ticker was scanned.`
        + (lastChecked - firstChecked > 60 ? ` Selected data was checked from ${readinessTime(firstChecked)} to ${readinessTime(lastChecked)}.` : '')
        + (nweInsufficient && snapshot.nwe_required_bars > 0 ? ` NWE needs ${snapshot.nwe_required_bars} chronological bars for its warm-up.` : '');
    result.metrics = [
        ...(workerMetric ? [workerMetric] : []),
        { label: 'Verified tickers checked', value: String(instruments) },
        { label: 'Fresh 15m prices at scan', value: `${fresh} / ${instruments}` },
        { label: 'Ticker / timeframe checks evaluated', value: String(evaluated) },
        { label: 'Qualifying rows at scan', value: String(qualified) },
        ...(afterFeeChecks ? [{ label: 'After-fee checks (informational)', value: String(afterFeeChecks) }] : []),
        ...(afterFeeMetric ? [afterFeeMetric] : []),
    ];
    const informationalAfterFeeReasons = new Set(['after_fee_profile_missing', 'below_after_fee_score_cutoff', 'after_fee_score_unavailable']);
    result.examples = products.flatMap(row => (Array.isArray(row.examples) ? row.examples : [])
        .filter(example => !informationalAfterFeeReasons.has(example.reason))
        .filter(example => timeframe === 'all' || example.timeframe === timeframe
            || (example.timeframe === '15m' && ['stale_price', 'missing_completed_price', 'missing_history', 'data_error'].includes(example.reason)))
        .map(example => ({ ...example, product_id: row.product_id }))).slice(0, 6);
    result.nextStep = 'Candle ingestion runs in the background. Recheck status after data recovers, then Refresh signals to rescan stored candles.';
    result.tone = 'warning';
    if (summary.status?.last_attempt_status === 'error') {
        result.title = 'The latest signal scan failed';
        result.message = `The checks below belong to the last successful scan. ${String(summary.status.last_error || 'The service did not record a detailed error.').slice(0, 250)}`;
    } else if (summary.stale) {
        result.title = 'The last signal checks are outdated';
        result.message = `A new completed scan is needed to explain this view now.${blockers.length ? ` Last observed data blockers: ${blockers.join('; ')}.` : ''}${backtestGateSummary.length ? ` After-fee results (informational): ${backtestGateSummary.join('; ')}.` : ''}`;
        result.nextStep = 'Refresh signals or wait for the running scan to complete, then Recheck status. After-fee profile promotion does not control signal visibility.';
    } else if (count(summary.stale_price_signal_count)) {
        result.title = 'Signal setups retained with stale reference prices';
        result.message = `${count(summary.stale_price_signal_count)} active setups remain visible for inspection. Their reference prices are stale, so they are not current market entries.`;
    } else if (!instruments) {
        result.title = 'No verified tickers were scanned';
        result.message = 'This product has no eligible app listings in the completed scan.';
        result.retryListings = supportsListingRefresh;
    } else if (!fresh) {
        result.title = 'Waiting for fresh 15m prices';
        result.message = `None of ${instruments} verified tickers had a fresh matching 15m price at the last scan.${blockers.length ? ` Observed: ${blockers.join('; ')}.` : ''}`;
    } else if (!evaluated) {
        result.title = insufficient || nweInsufficient ? 'Not enough usable model history' : 'Timeframe data is not ready';
        result.message = `${fresh} tickers had fresh 15m prices, but no selected ticker / timeframe check could be evaluated.${blockers.length ? ` Observed: ${blockers.join('; ')}.` : ''}`;
    } else if (selection.search && selection.visibleCount === 0) {
        result.title = 'No current signals match your search';
        result.message = `Data checks cover the selected app, product, and timeframe. Your search filters the returned signal rows only.`;
        result.nextStep = 'Clear the search or choose another product or timeframe.';
        result.tone = 'neutral';
    } else if (!qualified && blockers.length) {
        result.title = 'Some tickers still need usable data';
        result.message = `${evaluated} ticker / timeframe checks were evaluated without a qualifying setup.${backtestGateSummary.length ? ` After-fee metrics (informational): ${backtestGateSummary.join('; ')}.` : ''} Other checks remain blocked: ${blockers.join('; ')}.`;
    } else if (!qualified && afterFeeChecks) {
        result.title = 'No qualifying setup in the last scan';
        result.message = `${evaluated} ticker / timeframe checks had usable inputs but did not meet the technical setup and score rules. After-fee backtest metrics are informational and do not filter signals.${backtestGateSummary.length ? ` Status: ${backtestGateSummary.join('; ')}.` : ''}`;
    } else if (!qualified) {
        result.title = 'No qualifying setup in the last scan';
        result.message = `${evaluated} ticker / timeframe checks had usable data but produced no qualifying signal. An empty result is valid when the model rules are not met.`;
        result.nextStep = 'Wait for the next scan or explore another timeframe. Refresh signals reruns the models on stored candles.';
        result.tone = 'neutral';
    } else if (selection.visibleCount === 0) {
        result.title = 'Last scan found signals; none are current in this view';
        result.message = `The last scan recorded ${qualified} qualifying rows in this scope. This response contains no current matching rows; the saved counts are not a live signal count.`;
    } else {
        result.title = 'Signals are available in this view';
        result.message = blockers.length ? `Some checks still need data: ${blockers.join('; ')}.` : 'The selected models had usable inputs at the last completed scan.';
        result.tone = blockers.length ? 'warning' : 'ready';
        result.nextStep = 'Select a signal to inspect its score, candle source, and freshness.';
    }
    return result;
}
