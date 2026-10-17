import { publishDashboardBindings } from './runtime-context.js';
import { stockbitExpectedSessionDate } from './chart-stockbit-session.js';

export function chartDiagnosticsSeries(seriesInput, symbol, interval, source, exchangeId) {
    const series = Array.isArray(seriesInput)
        ? seriesInput
        : (seriesInput && typeof seriesInput === 'object' ? Object.values(seriesInput) : []);
    const wantedSymbol = String(symbol || '').toUpperCase();
    const wantedInterval = String(interval || '').toLowerCase();
    const matchingSymbolSeries = series.filter(item => String(item?.symbol || '').toUpperCase() === wantedSymbol);
    const matchingIntervalSeries = matchingSymbolSeries.filter(item => String(item?.interval || '').toLowerCase() === wantedInterval);
    const wantedSource = String(source || '').trim();
    if (wantedSource) {
        return matchingIntervalSeries.find(item => String(item?.source || '') === wantedSource) || null;
    }

    const wantedExchange = String(exchangeId || '').trim().toLowerCase();
    if (wantedExchange) {
        return matchingIntervalSeries.find(item => String(item?.exchange_id || '').trim().toLowerCase() === wantedExchange) || null;
    }

    return matchingIntervalSeries[0] || matchingSymbolSeries[0] || null;
}

/** Owns chart-history freshness, source labels, and diagnostic disclosure. */
export function installChartDataDiagnostics() {
            function formatDataAge(seconds) {
                const age = Number(seconds);
                if (!Number.isFinite(age) || age < 0) return 'unknown age';
                if (age < 90) return `${Math.round(age)}s`;
                if (age < 90 * 60) return `${Math.round(age / 60)}m`;
                if (age < 48 * 3600) return `${(age / 3600).toFixed(1)}h`;
                return `${(age / 86400).toFixed(1)}d`;
            }

            function formatStoredBarTime(timestamp) {
                const value = Number(timestamp);
                if (!Number.isFinite(value)) return 'unknown time';
                const date = new Date(value / 1e6);
                if (Number.isNaN(date.getTime())) return 'unknown time';
                return new Intl.DateTimeFormat('en-GB', {
                    timeZone: 'Asia/Jakarta', year: 'numeric', month: '2-digit', day: '2-digit',
                    hour: '2-digit', minute: '2-digit', hourCycle: 'h23'
                }).format(date) + ' WIB';
            }

            function formatDiagnosticTimestamp(value) {
                if (value === null || value === undefined || value === '') return 'unknown';
                const numeric = Number(value);
                if (Number.isFinite(numeric) && String(value).trim() !== '') {
                    let millis = numeric;
                    if (Math.abs(numeric) > 1e17) millis = numeric / 1e6;
                    else if (Math.abs(numeric) > 1e14) millis = numeric / 1e3;
                    else if (Math.abs(numeric) < 1e11) millis = numeric * 1e3;
                    const numericDate = new Date(millis);
                    if (!Number.isNaN(numericDate.getTime())) {
                        return new Intl.DateTimeFormat('en-GB', {
                            timeZone: 'Asia/Jakarta', year: 'numeric', month: '2-digit', day: '2-digit',
                            hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23'
                        }).format(numericDate) + ' WIB';
                    }
                }
                const raw = String(value).trim();
                // Rust sync metadata is UTC. Convert it to the same WIB display
                // used by the latest-bar field so the comparison is unambiguous.
                if (/\bUTC\s*$/i.test(raw)) {
                    const utcDate = new Date(raw.replace(/\s+UTC\s*$/i, 'Z').replace(' ', 'T'));
                    if (!Number.isNaN(utcDate.getTime())) {
                        return new Intl.DateTimeFormat('en-GB', {
                            timeZone: 'Asia/Jakarta', year: 'numeric', month: '2-digit', day: '2-digit',
                            hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23'
                        }).format(utcDate) + ' WIB';
                    }
                }
                // Older persisted metadata can be a display string without an
                // explicit timezone. Preserve it instead of guessing its offset.
                return raw;
            }

            function toggleChartDataDiagnostics() {
                const details = document.getElementById('chartDataDiagnostics');
                const trigger = document.getElementById('chartDataDebugBtn');
                if (!details) return;
                details.open = !details.open;
                if (trigger) trigger.setAttribute('aria-expanded', details.open ? 'true' : 'false');
                if (details.open) details.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
            }

            const chartDataDiagnosticsDetails = document.getElementById('chartDataDiagnostics');
            if (chartDataDiagnosticsDetails && !chartDataDiagnosticsDetails.dataset.bound) {
                chartDataDiagnosticsDetails.dataset.bound = 'true';
                chartDataDiagnosticsDetails.addEventListener('toggle', () => {
                    const trigger = document.getElementById('chartDataDebugBtn');
                    if (trigger) trigger.setAttribute('aria-expanded', chartDataDiagnosticsDetails.open ? 'true' : 'false');
                });
            }

            function stockbitMissingSessionDate(signal, timeframe, lastTs) {
                const exchange = String(signal?.exchange || signal?.Exchange || signal?.venue || signal?.Venue || marketVenueForSignal(signal) || '').toLowerCase();
                if (!exchange.includes('stockbit') || lastTs == null || lastTs === '') return null;
                const expectedDate = stockbitExpectedSessionDate(timeframe);
                if (!expectedDate) return null;
                const value = Number(lastTs);
                if (!Number.isFinite(value)) return null;
                const timestampMs = Math.abs(value) > 1e14 ? value / 1e6 : Math.abs(value) > 1e11 ? value : value * 1000;
                const lastDateParts = new Intl.DateTimeFormat('en-US', {
                    timeZone: 'Asia/Jakarta', year: 'numeric', month: '2-digit', day: '2-digit'
                }).formatToParts(new Date(timestampMs)).reduce((result, part) => { result[part.type] = part.value; return result; }, {});
                const lastDate = `${lastDateParts.year}-${lastDateParts.month}-${lastDateParts.day}`;
                return lastDate < expectedDate ? expectedDate : null;
            }

            function updateChartDataDiagnostics(signal = selectedSignal) {
                const summary = document.getElementById('chart-data-diagnostics-summary');
                const state = document.getElementById('chart-data-diagnostics-state');
                const why = document.getElementById('chart-diagnostics-why');
                const latest = document.getElementById('chart-diagnostics-latest');
                const age = document.getElementById('chart-diagnostics-age');
                const coverage = document.getElementById('chart-diagnostics-coverage');
                const source = document.getElementById('chart-diagnostics-source');
                const attempt = document.getElementById('chart-diagnostics-attempt');
                const success = document.getElementById('chart-diagnostics-success');
                const error = document.getElementById('chart-diagnostics-error');
                const runList = document.getElementById('chart-diagnostics-run-list');
                const retry = document.getElementById('chart-diagnostics-retry');
                const sync = document.getElementById('chart-diagnostics-sync');
                if (!summary || !state || !why || !latest || !age || !coverage || !source || !attempt || !success || !error || !runList) return;

                const setText = (element, value) => { element.textContent = value == null || value === '' ? '--' : String(value); };
                const reset = (message = 'Select a signal to inspect its local history and provider sync state.') => {
                    setText(summary, 'Select a chart to inspect stored data');
                    setText(state, 'NO SIGNAL SELECTED');
                    setText(why, message);
                    [latest, age, coverage, source, attempt, success, error].forEach(element => setText(element, '--'));
                    runList.textContent = 'No sync events loaded.';
                    if (retry) retry.disabled = true;
                    if (sync) sync.disabled = true;
                    state.className = 'shrink-0 px-1.5 py-0.5 rounded border border-gray-700 bg-gray-900 text-[9px] font-mono text-gray-500';
                };
                if (!signal) {
                    reset();
                    return;
                }

                const meta = signal.chart_freshness?.[activeTimeframe] || {};
                const symbol = marketSymbolForSignal(signal);
                const storageInterval = String(meta.storage_timeframe || activeTimeframe || '').toLowerCase();
                const storedSeries = chartDiagnosticsSeries(
                    CHART_STORAGE_DIAGNOSTICS?.series,
                    symbol,
                    storageInterval,
                    meta.source,
                    meta.exchange_id,
                ) || {};
                const rows = Number(meta.rows ?? storedSeries.rows ?? 0);
                const coverageDays = Number(meta.coverage_days ?? storedSeries.coverage_days);
                const targetDays = Number(meta.history_target_days ?? storedSeries.history_target_days ?? 730);
                const lastTs = meta.last_ts ?? meta.last_bar_ts ?? storedSeries.last_ts;
                const lastError = String(meta.last_error || storedSeries.last_error || '').trim();
                const reason = String(meta.error_reason || storedSeries.error_reason || (lastError ? syncFailureLabel(lastError) : '')).trim();
                const missingSessionDate = stockbitMissingSessionDate(signal, activeTimeframe, lastTs);
                const stale = rows <= 0 || meta.is_stale === true || !!missingSessionDate;
                const stateLabel = rows <= 0 ? 'NO DATA' : lastError ? 'SYNC ERROR' : stale ? 'STALE' : 'READY';
                const stateClass = rows <= 0 || lastError || stale
                    ? 'shrink-0 px-1.5 py-0.5 rounded border border-amber-700/80 bg-amber-950/70 text-[9px] font-mono text-amber-300'
                    : 'shrink-0 px-1.5 py-0.5 rounded border border-emerald-700/80 bg-emerald-950/70 text-[9px] font-mono text-emerald-300';
                const latestText = lastTs ? (typeof lastTs === 'number' || /^\d+$/.test(String(lastTs)) ? formatStoredBarTime(lastTs) : formatDiagnosticTimestamp(lastTs)) : 'No local bar';
                const ageSeconds = Number.isFinite(Number(meta.age_seconds)) ? Number(meta.age_seconds) : null;
                const exchange = String(signal.exchange || signal.Exchange || signal.venue || signal.Venue || marketVenueForSignal(signal) || 'app').replaceAll('_', ' ').toUpperCase();
                const sourceName = String(meta.source_label || storedSeries.source_label || meta.source || storedSeries.source || 'SQLite local history').replaceAll('_', ' ');
                const providerId = String(meta.data_provider || storedSeries.data_provider || '').trim().toLowerCase();
                const providerName = ({
                    binance: 'Binance',
                    yfinance: 'Yahoo Finance',
                    tokocrypto: 'Tokocrypto public API',
                })[providerId] || providerId.replaceAll('_', ' ');
                const instrumentKind = String(meta.instrument_kind || storedSeries.instrument_kind || '').replaceAll('_', ' ');
                const providerText = providerName ? ` · Data provider: ${providerName}` : '';
                const sourceText = `App exchange: ${exchange}${providerText} · ${sourceName}${instrumentKind ? ` · ${instrumentKind}` : ''}`;
                const attemptText = formatDiagnosticTimestamp(meta.last_attempt || meta.sync_attempt_at || storedSeries.last_attempt);
                const successText = formatDiagnosticTimestamp(meta.last_success || meta.sync_success_at || storedSeries.last_success);

                setText(summary, `${symbol} · ${String(activeTimeframe || '--').toUpperCase()} · ${latestText}`);
                setText(state, stateLabel);
                state.className = stateClass;
                setText(latest, latestText);
                setText(age, ageSeconds !== null ? formatDataAge(ageSeconds) : '--');
                setText(coverage, `${rows || 0} bars · ${Number.isFinite(coverageDays) ? Math.floor(coverageDays) : 0}/${Number.isFinite(targetDays) ? Math.floor(targetDays) : 730} days`);
                setText(source, sourceText);
                setText(attempt, attemptText);
                setText(success, successText);
                setText(error, lastError ? `${reason || syncFailureLabel(lastError)} · ${lastError}` : 'No unresolved provider error');
                if (retry) retry.disabled = false;
                if (sync) sync.disabled = false;

                const exchangeAsset = `${exchange} · ${symbol} · ${String(activeTimeframe || '--').toUpperCase()}`;
                if (!rows) {
                    setText(why, `${exchangeAsset} has no locally stored candles for this timeframe. Run a sync or retry the chart to request the selected app history.`);
                } else if (lastError) {
                    setText(why, `${exchangeAsset} was last attempted at ${attemptText}, but the provider failed with ${reason || syncFailureLabel(lastError)}. The chart is showing the last candle retained in SQLite at ${latestText}; newer data is not being invented.`);
                } else if (meta.source_is_proxy && meta.native_fallback_reason) {
                    setText(why, `${exchangeAsset} is using ${sourceName} because the native feed failed: ${meta.native_fallback_reason}. The latest saved proxy candle is ${latestText}; proxy history does not validate venue execution.`);
                } else if (missingSessionDate) {
                    setText(why, `${exchangeAsset} has no stored bar for the latest expected weekday session (${missingSessionDate}). The chart keeps the last saved bar at ${latestText}; today's OHLC is not being invented.`);
                } else if (stale) {
                    setText(why, `${exchangeAsset} has not received a newer stored candle since ${latestText}. The stored series is older than its freshness threshold; retry the chart or sync the selected asset.`);
                } else {
                    setText(why, `${exchangeAsset} is reading stored candles from ${sourceName}. The latest saved bar is ${latestText}; provider sync metadata is shown below for comparison.`);
                }
                if (meta.derived_from_timeframe && String(meta.derived_from_timeframe).toLowerCase() !== storageInterval) {
                    setText(why, `${why.textContent} This ${String(activeTimeframe).toUpperCase()} view is rolled up from stored ${String(meta.derived_from_timeframe).toUpperCase()} candles.`);
                }

                const rawRuns = Array.isArray(CHART_STORAGE_DIAGNOSTICS?.recent_runs) ? CHART_STORAGE_DIAGNOSTICS.recent_runs : [];
                const runs = rawRuns.filter(run => String(run?.symbol || '').toUpperCase() === String(symbol).toUpperCase()
                    && (!storageInterval || String(run?.interval || '').toLowerCase() === storageInterval)
                    && (!meta.source || !run?.source || String(run.source) === String(meta.source))).slice(0, 5);
                runList.textContent = '';
                if (!runs.length) {
                    runList.textContent = CHART_STORAGE_DIAGNOSTICS ? 'No recent sync events for this selected series.' : 'Storage diagnostics are still loading.';
                } else {
                    runs.forEach(run => {
                        const row = document.createElement('div');
                        const runReason = run.error_reason || (run.error ? syncFailureLabel(run.error) : 'completed');
                        const runSource = run.source_label || run.source || '';
                        row.textContent = `${formatDiagnosticTimestamp(run.attempted_at)} · ${String(run.status || 'unknown').toUpperCase()} · ${Number(run.rows_received || 0)} rows${runSource ? ` · ${runSource}` : ''} · ${runReason}`;
                        row.className = String(run.status || '').toLowerCase() === 'error' ? 'text-amber-300' : 'text-gray-400';
                        runList.appendChild(row);
                    });
                }
            }

            function syncFailureLabel(rawError) {
                const text = String(rawError || '').trim().toLowerCase();
                if (!text) return 'Provider error';
                if (text.includes('exit code: some(7)') || text.includes('could not connect')) return 'Market-data provider unreachable';
                if (text.includes('exit code: some(28)') || text.includes('timed out')) return 'Market-data provider timeout';
                if (text.includes('exit code: some(6)') || text.includes('could not resolve host')) return 'DNS failure';
                if (text.includes('disabled by')) return 'Sync disabled';
                if (text.includes('invalid payload') || text.includes('no chart result') || text.includes('missing ohlc')) return 'Bad provider data';
                return 'Provider error';
            }

            function syncFailureTitle(failure) {
                if (!failure) return '';
                const label = syncFailureLabel(failure.error);
                const reason = String(failure.reason || label);
                const series = [failure.symbol, failure.interval].filter(Boolean).join(' · ') || 'unknown series';
                const attempted = failure.attempted_at || 'unknown time';
                const stored = failure.last_success ? ` Last successful bar: ${failure.last_success}.` : '';
                const raw = failure.error && failure.error !== reason ? ` Provider detail: ${failure.error}.` : '';
                return `Latest unresolved sync failure for ${series} at ${attempted}. Reason: ${reason} (${label}).${stored}${raw} Stored history is retained; retry Sync Market when the provider is reachable.`;
            }

            function updateChartDataSource(signal = selectedSignal) {
                const badge = document.getElementById('tv-ohlc-source');
                if (!badge) return;
                const meta = signal?.chart_freshness?.[activeTimeframe];
                const sourceTimeframe = String(meta?.derived_from_timeframe || '').trim();
                if (!sourceTimeframe || sourceTimeframe.toLowerCase() === activeTimeframe.toLowerCase()) {
                    badge.className = 'hidden text-[10px] px-1.5 py-0.2 rounded bg-purple-900/60 text-purple-300 font-bold border border-purple-800';
                    badge.textContent = '';
                    badge.removeAttribute('title');
                    return;
                }
                const sourceLabel = sourceTimeframe.toUpperCase();
                const targetLabel = String(activeTimeframe || '--').toUpperCase();
                badge.className = 'inline-flex text-[10px] px-1.5 py-0.2 rounded bg-purple-900/60 text-purple-300 font-bold border border-purple-800';
                badge.textContent = `ROLLUP · ${sourceLabel}`;
                badge.title = `${targetLabel} candles are aggregated from stored ${sourceLabel} candles. Exchange, timezone, and OHLC values remain bound to the selected app history.`;
            }

            function updateChartDataWarning(signal = selectedSignal) {
                updateChartDataDiagnostics(signal);
                const badge = document.getElementById('chart-data-warning');
                if (!badge) return;
                const meta = signal?.chart_freshness?.[activeTimeframe];
                updateChartDataSource(signal);
                const rows = Number(meta?.rows || 0);
                const coverageDays = Number(meta?.coverage_days);
                const targetDays = Number(meta?.history_target_days || 730);
                const underHistoryTarget = rows > 0
                    && Number.isFinite(coverageDays)
                    && Number.isFinite(targetDays)
                    && coverageDays < targetDays;
                const insufficient = String(meta?.chart_status || '').toLowerCase() === 'insufficient_data';
                // A retry can fail after the most recent stored bar is still
                // inside the freshness window. That is a provider warning, not a
                // stale chart; only the effective bar age controls this badge.
                const missingSessionDate = stockbitMissingSessionDate(signal, activeTimeframe, meta?.last_ts ?? meta?.last_bar_ts);
                const stale = rows <= 0 || meta?.is_stale === true || !!missingSessionDate;
                if (!meta || (!stale && !underHistoryTarget && !insufficient)) {
                    badge.className = 'hidden max-w-[280px] px-2 py-0.5 rounded border text-[10px] font-bold leading-tight';
                    badge.textContent = '';
                    badge.removeAttribute('title');
                    return;
                }

                const timeframe = String(activeTimeframe || '--').toUpperCase();
                const source = String(meta.source || 'local history').replaceAll('_', ' ');
                const age = formatDataAge(meta.age_seconds);
                const syncReason = syncFailureLabel(meta.last_error);
                let message = insufficient
                    ? `⚠ INSUFFICIENT ${timeframe} DATA`
                    : rows <= 0
                    ? `⚠ NO LOCAL ${timeframe} DATA`
                    : meta.last_error
                        ? `⚠ SYNC FAILED · ${syncReason}`
                        : missingSessionDate
                            ? '⚠ LATEST SESSION BAR MISSING'
                        : underHistoryTarget
                            ? `⚠ HISTORY ${Math.floor(coverageDays)}/${Math.floor(targetDays)}D`
                        : `⚠ DATA STALE · ${age} old`;
                const coverageText = underHistoryTarget
                    ? `History coverage is ${Math.floor(coverageDays)}/${Math.floor(targetDays)} calendar days. ${meta.history_warning || 'Extend local history before treating long-horizon evidence as complete.'}`
                    : '';
                badge.className = 'max-w-[280px] px-2 py-0.5 rounded border border-amber-700/80 bg-amber-950/70 text-amber-300 text-[10px] font-bold leading-tight';
                badge.textContent = message;
                badge.title = insufficient
                    ? (meta.chart_warning || meta.warning || `At least 2 chronological ${timeframe} candles are required to render this chart.`)
                    : rows <= 0
                    ? `No locally stored ${timeframe} candles. Run Sync Market to ingest history.`
                    : missingSessionDate
                    ? `No Stockbit ${timeframe} bar is stored for the latest expected weekday session (${missingSessionDate}). Last stored bar: ${formatStoredBarTime(meta.last_ts ?? meta.last_bar_ts)}. The chart is using retained SQLite history; no OHLC values were fabricated.`
                    : `Last stored ${timeframe} bar: ${formatStoredBarTime(meta.last_ts)} · ${source} · age ${age}. ${meta.last_error ? `Reason: ${meta.error_reason || syncReason}. Provider detail: ${meta.last_error}. Stored history is retained.` : (meta.warning || 'Refresh market data if this is unexpected.')} ${coverageText}`;
            }

    publishDashboardBindings('chart-data-diagnostics', {
        formatDataAge: { get: () => formatDataAge },
        formatStoredBarTime: { get: () => formatStoredBarTime },
        formatDiagnosticTimestamp: { get: () => formatDiagnosticTimestamp },
        chartDiagnosticsSeries: { get: () => chartDiagnosticsSeries },
        toggleChartDataDiagnostics: { get: () => toggleChartDataDiagnostics },
        chartDataDiagnosticsDetails: { get: () => chartDataDiagnosticsDetails },
        stockbitExpectedSessionDate: { get: () => stockbitExpectedSessionDate },
        stockbitMissingSessionDate: { get: () => stockbitMissingSessionDate },
        updateChartDataDiagnostics: { get: () => updateChartDataDiagnostics },
        syncFailureLabel: { get: () => syncFailureLabel },
        syncFailureTitle: { get: () => syncFailureTitle },
        updateChartDataSource: { get: () => updateChartDataSource },
        updateChartDataWarning: { get: () => updateChartDataWarning },
    });
}
