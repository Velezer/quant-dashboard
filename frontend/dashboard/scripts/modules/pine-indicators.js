import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { requestAllInstruments, requestJson, requestPineCapabilities } from './api.js';
import { MAX_ACTIVE_PINE_INDICATORS } from './pine-chart-output.js';
import { CHART_INDICATOR_IDS } from './chart-data.js';
import { installNweDssBacktestReport } from './nwe-dss-backtest.js';
import { excursionSummary, installNweExcursionReport } from './pine-nwe-excursion-report.js';
import { finiteNumber, pineWorkerInput } from './pine-indicator-input.js';
import { cleanCopiedPineSource, pineSourceMetadata } from './pine-source-text.js';
import { createPineIndicatorStatus } from './pine-indicator-status.js';
import {
    BUILTIN_PINE_INDICATORS,
    NWE_MAX_OUTSIDE_DISTANCE_ID,
} from './pine-sources.js';

const DSS_INDICATOR_NAME = 'DSS Bressert (Double Smoothed Stochastic)';

export function installPineIndicators() {
    const dialog = document.getElementById('pineIndicatorDialog');
    const status = document.getElementById('pineIndicatorStatus');
    const diagnosticList = document.getElementById('pineIndicatorDiagnosticList');
    const sourceInput = document.getElementById('pineIndicatorSource');
    const scriptTypeSelect = document.getElementById('pineScriptType');
    const registrationButton = document.getElementById('registerPineIndicator');
    const registeredList = document.getElementById('pineRegisteredIndicators');
    const useButton = document.getElementById('usePineIndicator');
    const selectionSummary = document.getElementById('pineIndicatorSelectionSummary');
    const sourceViewer = document.getElementById('pineSourceViewer');
    const sourceViewerTitle = document.getElementById('pineSourceViewerTitle');
    const sourceViewerCode = document.getElementById('pineSourceViewerCode');
    const sourceViewerStatus = document.getElementById('pineSourceViewerStatus');
    const copySourceButton = document.getElementById('copyPineSource');
    const reportStatus = document.getElementById('nweExcursionReportStatus');
    const { setStatus, showDiagnostics } = createPineIndicatorStatus({ status, diagnosticList });
    let worker;
    let workerReady = false;
    let activeRecords = [];
    let registeredIndicators = [];
    const selectedIndicatorIds = new Set();
    let maxSourceBytes = 0;
    let requestId = 0;
    let evaluationSerial = 0;
    const pending = new Map();
    const autoFilledMetadata = new Map();
    let rightsBasisWasChosen = false;

    installNweDssBacktestReport();
    installNweExcursionReport({ status: reportStatus, ensureWorker, callWorker, requestJson, requestAllInstruments, finiteNumber, displayedHistorySource });

    window.addEventListener('quant:signal-selection-changing', event => {
        evaluationSerial += 1;
        const signal = event.detail?.signal;
        if (signal) signal.pine_indicator_outputs = [];
    });


    function updateSelectionControls() {
        const checkboxes = registeredList?.querySelectorAll('input[type=\"checkbox\"]') || [];
        const selectedCount = [...checkboxes].filter(checkbox => checkbox.checked).length;
        for (const checkbox of checkboxes) {
            checkbox.disabled = !checkbox.checked && selectedCount >= MAX_ACTIVE_PINE_INDICATORS;
        }
        if (useButton) useButton.disabled = selectedCount === 0 || !workerReady;
        if (selectionSummary) {
            selectionSummary.textContent = selectedCount
                ? selectedCount + ' of ' + MAX_ACTIVE_PINE_INDICATORS + ' selected. Pine source inputs use the matching active chart history in browser WASM.'
                : 'Choose up to two built-in or saved indicators. Each runs in browser WASM on stored candles.';
        }
    }

    function replaceWorker() {
        worker?.terminate();
        worker = new Worker(new URL('../workers/pine-worker.js', import.meta.url), { type: 'module' });
        workerReady = false;
        updateSelectionControls();
        for (const entry of pending.values()) entry.reject(new Error('WASM runtime is restarting.'));
        pending.clear();
        worker.addEventListener('message', event => {
            const { id, type, output, error } = event.data || {};
            const entry = pending.get(id);
            if (!entry) return;
            pending.delete(id);
            if (type === 'ready') { workerReady = true; entry.resolve(true); }
            else if (type === 'result') entry.resolve(output);
            else entry.reject(new Error(error || 'WASM worker failed.'));
        });
        worker.addEventListener('error', event => {
            workerReady = false;
            updateSelectionControls();
            for (const entry of pending.values()) entry.reject(new Error(event.message || 'worker error'));
            pending.clear();
            setStatus(`Browser WASM worker failed: ${event.message || 'worker error'}. Retry the runtime.`, 'error');
        });
        return new Promise((resolve, reject) => {
            const id = ++requestId;
            const timeout = setTimeout(() => { pending.delete(id); reject(new Error('WASM initialization timed out.')); }, 10000);
            pending.set(id, { resolve: value => { clearTimeout(timeout); resolve(value); }, reject: error => { clearTimeout(timeout); reject(error); } });
            worker.postMessage({ id, type: 'init' });
        });
    }

    function callWorker(payload) {
        return new Promise((resolve, reject) => {
            const id = ++requestId;
            const timeout = setTimeout(() => { pending.delete(id); reject(new Error('WASM calculation timed out.')); }, 15000);
            pending.set(id, {
                resolve: value => { clearTimeout(timeout); resolve(value); },
                reject: error => { clearTimeout(timeout); reject(error); },
            });
            worker.postMessage({ id, ...payload });
        });
    }

    async function ensureWorker() {
        if (workerReady) return true;
        setStatus('Loading the browser WASM Pine runtime…', 'loading');
        try {
            await replaceWorker();
            setStatus('Browser WASM runtime is ready for bounded historical indicator and strategy validation.', 'ready');
            updateSelectionControls();
            return true;
        } catch (error) {
            setStatus(`WASM unavailable: ${error.message}. Check the local service/build, then retry.`, 'error');
            updateSelectionControls();
            return false;
        }
    }

    async function loadCapabilities() {
        const payload = await requestPineCapabilities();
        const limit = Number(payload?.max_source_bytes);
        if (!Number.isSafeInteger(limit) || limit <= 0) throw new Error('Runtime returned an invalid source limit.');
        maxSourceBytes = limit;
        sourceInput?.setAttribute('maxlength', String(limit));
        const label = document.getElementById('pineSourceLimitLabel');
        if (label) label.textContent = `(max ${(limit / 1024).toFixed(0)} KB)`;
        return payload;
    }

    function prefillMetadataFromSource() {
        const source = sourceInput?.value || '';
        const { title, author, license, licenseUrl, reference } = pineSourceMetadata(source);
        const setHeaderValue = (id, value) => {
            const field = document.getElementById(id);
            if (!field || !value || (field.value && field.value !== autoFilledMetadata.get(id))) return false;
            field.value = value;
            autoFilledMetadata.set(id, value);
            return true;
        };
        if (!title && !author && !license) return;
        const filled = [setHeaderValue('pineIndicatorName', title), setHeaderValue('pineIndicatorAuthor', author)].some(Boolean);
        let detailsChanged = false;
        if (license) {
            const rightsBasis = document.getElementById('pineIndicatorRightsBasis');
            if (rightsBasis && !rightsBasisWasChosen) {
                rightsBasis.value = 'license';
                document.getElementById('pineIndicatorLicense').required = true;
            }
            detailsChanged = [
                setHeaderValue('pineIndicatorLicense', `${license} · NonCommercial · ShareAlike · ${licenseUrl}`),
                setHeaderValue('pineIndicatorAttribution', `© ${author || 'LuxAlgo'}; ${title || 'Nadaraya-Watson Envelope [LuxAlgo]'}; ${licenseUrl}; indicate adaptations.`),
                setHeaderValue('pineIndicatorSourceReference', reference),
            ].some(Boolean);
            const terms = document.getElementById('pineRightsTermsNote');
            if (terms) {
                terms.textContent = `${license} permits noncommercial use; adaptations must remain under the same license. Retain © ${author || 'LuxAlgo'} and the license link. Confirm rights only if your intended use meets these terms.`;
                terms.classList.remove('hidden');
            }
        }
        if (filled || detailsChanged) {
            const message = title === 'Nadaraya-Watson Envelope [LuxAlgo]'
                ? 'Source details filled from the header. The Pine NWE can replace the built-in overlay after you register and use it; confirm rights only if your use meets these terms.'
                : 'Source details filled from the header. Confirm rights only if your use meets the listed terms.';
            setStatus(message, 'ready');
        }
    }

    async function loadSavedIndicators(preferredId) {
        if (!registeredList) return;
        registeredList.setAttribute('aria-busy', 'true');
        registeredList.replaceChildren(document.createTextNode('Loading saved indicators…'));
        try {
            const payload = await requestJson('/api/pine/indicators');
            registeredIndicators = Array.isArray(payload?.indicators) ? payload.indicators : [];
            if (preferredId !== undefined && preferredId !== null) {
                selectedIndicatorIds.clear();
                if ([...BUILTIN_PINE_INDICATORS, ...registeredIndicators].some(item => String(item.id) === String(preferredId))) {
                    selectedIndicatorIds.add(String(preferredId));
                }
            }
            const allIndicators = [...BUILTIN_PINE_INDICATORS, ...registeredIndicators];
            const availableIds = new Set(allIndicators.map(item => String(item.id)));
            for (const id of selectedIndicatorIds) {
                if (!availableIds.has(id)) selectedIndicatorIds.delete(id);
            }
            if (allIndicators.length) {
                const list = document.createDocumentFragment();
                for (const item of allIndicators) {
                    const id = String(item.id);
                    const row = document.createElement('div');
                    row.className = 'flex items-start justify-between gap-2 rounded px-1.5 py-1 hover:bg-gray-800';
                    const label = document.createElement('label');
                    label.className = `flex min-w-0 items-start gap-2 text-xs font-normal ${item.runnable === false ? 'text-gray-300' : 'cursor-pointer text-gray-200'}`;
                    if (item.runnable !== false) {
                        const checkbox = document.createElement('input');
                        checkbox.type = 'checkbox';
                        checkbox.value = id;
                        checkbox.checked = selectedIndicatorIds.has(id);
                        checkbox.className = 'mt-0.5 accent-cyan-500';
                        checkbox.setAttribute('aria-label', `Use ${item.name} by ${item.author}`);
                        checkbox.addEventListener('change', () => {
                            const checkedCount = [...registeredList.querySelectorAll('input[type="checkbox"]')]
                                .filter(candidate => candidate.checked).length;
                            if (checkedCount > MAX_ACTIVE_PINE_INDICATORS) {
                                checkbox.checked = false;
                                setStatus('Choose at most two indicators. Uncheck one before adding another.', 'waiting');
                            }
                            if (checkbox.checked) selectedIndicatorIds.add(id);
                            else selectedIndicatorIds.delete(id);
                            updateSelectionControls();
                        });
                        label.append(checkbox);
                    }
                    const description = document.createElement('span');
                    description.className = 'min-w-0';
                    description.textContent = `${item.name} · ${item.author} · ${item.rights_basis}`;
                    label.append(description);
                    const viewButton = document.createElement('button');
                    viewButton.type = 'button';
                    viewButton.className = 'shrink-0 rounded border border-gray-700 px-1.5 py-0.5 text-[9px] text-cyan-200 hover:bg-gray-900 focus:outline focus:outline-2 focus:outline-cyan-500';
                    viewButton.textContent = 'View Pine source';
                    viewButton.setAttribute('aria-label', `View Pine source for ${item.name}`);
                    viewButton.addEventListener('click', () => { void showPineSource(item); });
                    row.append(label, viewButton);
                    list.append(row);
                }
                registeredList.replaceChildren(list);
            } else {
                const empty = document.createElement('p');
                empty.className = 'px-1.5 py-1 font-normal text-gray-500';
                empty.textContent = 'No Pine sources are available.';
                registeredList.replaceChildren(empty);
                setStatus('No saved Pine indicators yet. Built-in sources remain available above.', 'waiting');
            }
            registeredList.removeAttribute('aria-busy');
            updateSelectionControls();
            return registeredIndicators;
        } catch (error) {
            registeredIndicators = [];
            registeredList.replaceChildren(document.createTextNode('Could not load saved indicators.'));
            registeredList.removeAttribute('aria-busy');
            updateSelectionControls();
            setStatus(`Saved indicators unavailable: ${error.message}. Retry after the local service recovers.`, 'error');
            return [];
        }
    }

    function currentChart() {
        return {
            signal: getDashboardBinding('selectedSignal'),
            candles: getDashboardBinding('getChartCandles')?.() || [],
            timeframe: getDashboardBinding('activeTimeframe') || '1d',
        };
    }

    function displayedHistorySource(freshness = {}) {
        const sourceLabel = freshness.source_label || freshness.source || freshness.data_provider || freshness.provider || 'SQLite stored history';
        const isResearchProxy = freshness.source_is_proxy === true || /research proxy|_proxy\b/i.test(sourceLabel);
        const source = isResearchProxy && !/research proxy/i.test(sourceLabel)
            ? `${sourceLabel} · research proxy`
            : sourceLabel;
        const quality = freshness.data_quality || freshness.quality || '';
        const stale = freshness.is_stale === true && !/stale/i.test(quality) ? 'STALE' : '';
        return [source, quality, stale].filter(Boolean).join(' · ');
    }

    function dataContext(signal, timeframe) {
        const series = signal?.chart_freshness?.[timeframe] || {};
        const app = signal?.exchange || signal?.Exchange || signal?.venue || signal?.Venue || 'selected app';
        const source = displayedHistorySource(series);
        return `${app} · ${signal?.asset || 'asset'} · ${timeframe} · ${source}`;
    }

    function setRegisteredPaneState(signal, record, state, timeframe, message = '', requestedTimeframe = '') {
        if (!signal) return;
        if (!record) {
            delete signal.pine_indicator_ui_state;
            return;
        }
        signal.pine_indicator_ui_state = {
            id: record.id,
            name: record.name,
            author: record.author,
            state,
            chart_timeframe: timeframe,
            requested_timeframe: requestedTimeframe,
            message,
            data_context: requestedTimeframe ? dataContext(signal, requestedTimeframe) : '',
        };
    }

    function workerInputForSignal(source, signal, candles, timeframe, modeOverride = null, scriptType = 'indicator') {
        return pineWorkerInput({
            source,
            signal,
            candles,
            timeframe,
            repainting: getDashboardBinding('nadarayaWatsonRepaintingEnabled')?.(signal) === true,
            modeOverride,
            scriptType,
        });
    }

    async function loadRecord(id) {
        const builtin = BUILTIN_PINE_INDICATORS.find(item => String(item.id) === String(id));
        if (builtin) return builtin;
        const payload = await requestJson(`/api/pine/indicators?id=${encodeURIComponent(id)}`);
        const record = payload?.indicators?.[0];
        if (!record) throw new Error('Saved indicator was not found. Refresh the saved indicator list.');
        return record;
    }

    async function showPineSource(item) {
        try {
            const record = item?.source ? item : await loadRecord(item?.id);
            if (!record?.source || !sourceViewerCode) throw new Error('Pine source is not available for this indicator.');
            sourceViewerTitle.textContent = `${record.name} · ${record.author || 'saved source'}`;
            sourceViewerCode.value = record.source;
            sourceViewerStatus.textContent = record.source_reference
                ? `Source: ${record.source_reference} · rights: ${record.rights_basis || 'recorded with indicator'}`
                : 'Built-in Pine source. Registered user sources retain their own rights provenance.';
            copySourceButton.disabled = false;
            if (sourceViewer) sourceViewer.open = true;
        } catch (error) {
            if (sourceViewerStatus) sourceViewerStatus.textContent = `Source unavailable: ${error.message}`;
        }
    }

    async function runRecords(records) {
        const { signal, candles, timeframe } = currentChart();
        if (!signal) { setStatus('Select a chart signal before running registered indicators.', 'waiting'); return false; }
        activeRecords = records;
        const paneRecord = records.find(record => String(record.id) === NWE_MAX_OUTSIDE_DISTANCE_ID)
            || records.find(record => record.name === DSS_INDICATOR_NAME);
        setRegisteredPaneState(signal, paneRecord, 'loading', timeframe, 'Calculating from approved stored candles.');
        const runId = ++evaluationSerial;
        signal.pine_indicator_outputs = [];
        getDashboardBinding('drawCandleChartNow')?.(signal);
        if (!Array.isArray(candles) || candles.length === 0) {
            const message = `No ${timeframe.toUpperCase()} candles are available. Retry chart history after the app/provider source recovers.`;
            setRegisteredPaneState(signal, paneRecord, 'error', timeframe, message);
            setStatus(message, 'error');
            showDiagnostics([{ severity: 'error', message: 'Indicator evaluation needs stored OHLCV candles; no fallback data was fabricated.' }]);
            getDashboardBinding('drawCandleChartNow')?.(signal);
            return false;
        }
        if (!await ensureWorker()) {
            setRegisteredPaneState(signal, paneRecord, 'error', timeframe, 'The browser WASM worker is unavailable. Use Pine indicators → Retry runtime.');
            getDashboardBinding('drawCandleChartNow')?.(signal);
            return false;
        }
        setStatus('Calculating ' + records.map(record => record.name).join(' and ')
            + ' on ' + candles.length + ' stored ' + timeframe.toUpperCase() + ' candles in browser WASM…', 'loading');
        const results = await Promise.all(records.map(async record => {
            try {
                return {
                    record,
                    output: await callWorker({ type: 'run', ...workerInputForSignal(record.source, signal, candles, timeframe) }),
                };
            } catch (error) {
                return { record, error };
            }
        }));
        if (runId !== evaluationSerial) return false;

        const successful = [];
        const diagnostics = [];
        const failures = [];
        for (const result of results) {
            const { record, output, error } = result;
            if (error || !output?.valid) {
                const message = error?.message || output?.diagnostics?.[0]?.message || 'The WASM runtime returned no supported output.';
                failures.push(record.name);
                diagnostics.push({ severity: 'error', message: record.name + ': ' + message });
                for (const item of output?.diagnostics || []) diagnostics.push({ ...item, message: record.name + ': ' + item.message });
                continue;
            }
            successful.push({ record, output });
            for (const item of output.diagnostics || []) diagnostics.push({ ...item, message: record.name + ': ' + item.message });
        }

        signal.pine_indicator_outputs = successful.map(({ record, output }) => ({
            id: record.id,
            name: record.name,
            author: record.author,
            timeframe,
            requested_timeframes: output.requested_timeframes || [],
            requested_data_contexts: (output.requested_timeframes || []).map(value => dataContext(signal, value)),
            requested_data_stale: (output.requested_timeframes || []).map(value => signal.chart_freshness?.[value]?.is_stale === true),
            plots: output.plots || [],
            data_context: dataContext(signal, timeframe),
        }));
        const nweMetric = successful.find(({ record }) => String(record.id) === NWE_MAX_OUTSIDE_DISTANCE_ID);
        if (nweMetric) {
            const summary = excursionSummary(nweMetric.output, candles);
            const mode = getDashboardBinding('nadarayaWatsonRepaintingEnabled')?.(signal) === true ? 'repainting' : 'endpoint';
            const upperBandCount = candles.filter(candle => Number.isFinite(candle?.indicators?.[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]?.[mode]?.upper)).length;
            const lowerBandCount = candles.filter(candle => Number.isFinite(candle?.indicators?.[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]?.[mode]?.lower)).length;
            const freshness = signal.chart_freshness?.[timeframe] || {};
            const source = displayedHistorySource(freshness);
            const appCategory = `${signal.exchange || signal.venue || 'Selected app'} · ${signal.class_badge || signal.class || 'registered ticker'}`;
            renderReportRows([{
                ticker: signal.asset || signal.symbol || 'Selected ticker',
                appCategory,
                ...(summary || { upper: null, lower: null, maximum: null, side: '—', peakCandle: null }),
                source,
                history: summary ? `${candles.length} bars` : (upperBandCount && lowerBandCount ? 'No band touch in history' : 'NWE warming up'),
            }]);
            if (reportStatus) reportStatus.textContent = `Selected ticker summary · ${timeframe.toUpperCase()} · ${mode}${freshness.is_stale === true ? ' · stale bars' : ''}. Run the report below for all active registered tickers in the selected app/product scope.`;
        }
        const paneResult = successful.find(({ record }) => String(record.id) === NWE_MAX_OUTSIDE_DISTANCE_ID)
            || successful.find(({ record }) => record.name === DSS_INDICATOR_NAME);
        const paneFailure = results.find(result => result.record.id === paneRecord?.id);
        const paneFailureMessage = paneFailure?.error?.message || paneFailure?.output?.diagnostics?.[0]?.message
            || 'The selected Pine indicator did not produce plots for this timeframe.';
        setRegisteredPaneState(
            signal,
            paneRecord,
            paneResult ? 'ready' : (paneRecord ? 'error' : ''),
            timeframe,
            paneResult ? '' : paneFailureMessage,
            paneResult?.output?.requested_timeframes?.[0] || '',
        );
        getDashboardBinding('resetIndicatorScale')?.();
        const undisplayedPaneIndicators = paneResult
            ? successful.filter(({ record, output }) => String(record.id) !== String(paneResult.record.id)
                && (output.plots || []).some(plot => !plot.overlay && plot.values?.length === candles.length))
                .map(({ record }) => record.name)
            : [];
        if (undisplayedPaneIndicators.length) {
            diagnostics.push({
                severity: 'approximation',
                message: `The shared indicator pane draws ${paneResult.record.name}; ${undisplayedPaneIndicators.join(', ')} also calculated, but its separate-pane plots are not drawn alongside it.`,
            });
        }
        for (const { record } of successful) {
            if (record.name === 'Nadaraya-Watson Envelope [LuxAlgo]') {
                getDashboardBinding('disableBuiltInNweForPineReplacement')?.();
            }
        }
        showDiagnostics(diagnostics);
        const plotGroups = successful.map(({ output }) => output);
        const overlayCount = plotGroups.reduce((count, output) => count + (output.plots || []).filter(plot => plot.overlay && plot.values?.length === candles.length).length, 0);
        const paneCount = plotGroups.reduce((count, output) => count + (output.plots || []).filter(plot => !plot.overlay && plot.values?.length === candles.length).length, 0);
        const sourceNames = successful.map(({ record }) => record.name).join(' + ');
        const requestedTimeframes = [...new Set(successful.flatMap(({ output }) => output.requested_timeframes || []).filter(value => value !== timeframe))];
        const requestedContext = requestedTimeframes.map(value => dataContext(signal, value)).join('; ');
        if (!successful.length) {
            setStatus('No selected indicator produced supported output. Nothing was drawn; review the diagnostics and retry after correcting the source or data.', 'error');
        } else if (nweMetric) {
            setStatus(`${nweMetric.record.name} calculated in browser WASM on ${dataContext(signal, timeframe)}. Above and below excursion plots are drawn in the indicator pane; the selected-ticker summary is in the NWE report. Historical research only, not execution evidence.`, failures.length ? 'error' : 'ready');
        } else if (paneCount && !successful.some(({ record }) => record.name === 'DSS Bressert (Double Smoothed Stochastic)') && !nweMetric) {
            diagnostics.push({ severity: 'approximation', message: 'Non-overlay plots from this source have no dedicated pane renderer yet; price-overlay plots remain visible.' });
            showDiagnostics(diagnostics);
            setStatus(sourceNames + ' ran in browser WASM on ' + dataContext(signal, timeframe)
                + '. ' + overlayCount + ' price-overlay plots shown; separate-pane plots are calculated but not drawn.', 'ready');
        } else {
            const resultSummary = successful.length + ' of ' + records.length + ' selected indicators calculated in browser WASM; '
                + overlayCount + ' price-overlay plots and ' + paneCount + ' indicator-pane plots shown.';
            const failureSummary = failures.length ? ' Failed: ' + failures.join(', ') + '. Successful outputs remain visible.' : '';
            const timeframeSummary = requestedContext ? ' Higher-timeframe history: ' + requestedContext + '.' : '';
            setStatus(resultSummary + ' ' + sourceNames + ' · ' + dataContext(signal, timeframe) + '.'
                + timeframeSummary + failureSummary + ' These outputs are not strategy or execution evidence.',
            failures.length ? 'error' : 'ready');
        }
        getDashboardBinding('drawCandleChartNow')?.(signal);
        return successful.length === records.length;
    }

    async function validateSource() {
        if (!maxSourceBytes) await loadCapabilities();
        const source = sourceInput?.value || '';
        if (new TextEncoder().encode(source).length > maxSourceBytes) throw new Error('Pine source exceeds the runtime source limit.');
        setStatus('Validating supported Pine syntax and candle behavior…', 'loading');
        const validation = await requestJson('/api/pine/validate', { method: 'POST', body: JSON.stringify({ source }) });
        showDiagnostics(validation.diagnostics || [], validation.approximations || []);
        if (!validation.valid) {
            setStatus('Validation stopped. Correct the reported syntax/unsupported behavior before registration.', 'error');
            return false;
        }
        const requestedType = scriptTypeSelect?.value || 'indicator';
        if (validation.script_type !== requestedType) {
            setStatus(`This source declares Pine ${validation.script_type || 'unknown'}(); choose the matching script type before registration.`, 'error');
            return false;
        }
        const { signal, candles, timeframe } = currentChart();
        if (candles.length && await ensureWorker()) {
            const output = await callWorker({ type: 'run', ...workerInputForSignal(source, signal, candles, timeframe, null, requestedType) });
            if (!output?.valid) {
                showDiagnostics(output?.diagnostics || [], validation.approximations || []);
                setStatus('The syntax parser accepted this code, but candle evaluation found unsupported behavior. Registration is blocked.', 'error');
                return false;
            }
            showDiagnostics(output.diagnostics || [], validation.approximations || []);
            setStatus(requestedType === 'strategy'
                ? `Pine strategy syntax and bounded broker runtime passed on ${candles.length} ${timeframe.toUpperCase()} chart candles. Fees are not inferred in this registration check; run the saved strategy through the selected-app history report.`
                : `Validation passed on ${candles.length} ${timeframe.toUpperCase()} candles for ${signal?.asset || 'the selected chart'}.`, 'ready');
        } else {
            setStatus('Syntax validated; runtime output has not been checked because no historical chart candles are available.', 'ready');
        }
        return true;
    }

    async function registerCurrentSource() {
        if (!document.getElementById('pineIndicatorRightsConfirmed')?.checked) throw new Error('Rights confirmation is required before registration.');
        if (!await validateSource()) return;
        const payload = {
            name: document.getElementById('pineIndicatorName')?.value || '',
            source: sourceInput?.value || '',
            author: document.getElementById('pineIndicatorAuthor')?.value || '',
            source_reference: document.getElementById('pineIndicatorSourceReference')?.value || '',
            rights_basis: document.getElementById('pineIndicatorRightsBasis')?.value || '',
            license_or_permission: document.getElementById('pineIndicatorLicense')?.value || '',
            attribution: document.getElementById('pineIndicatorAttribution')?.value || '',
            rights_confirmed: true,
        };
        const scriptType = scriptTypeSelect?.value || 'indicator';
        const route = scriptType === 'strategy' ? '/api/pine/strategies' : '/api/pine/indicators';
        const response = await requestJson(route, { method: 'POST', body: JSON.stringify(payload) });
        if (scriptType === 'strategy') {
            window.dispatchEvent(new Event('quant:pine-strategy-registered'));
            setStatus(`Registered “${response.strategy?.name || payload.name}” in app SQLite with rights provenance. Select it in Pine strategy backtests to run against the active chart instrument’s stored app candles.`, 'ready');
            return;
        }
        await loadSavedIndicators(response.indicator?.id);
        setStatus(`Registered “${response.indicator.name}” in app SQLite with rights provenance. Calculating historical output…`, 'loading');
        if (await runRecords([response.indicator])) dialog?.close();
    }

    async function useSelectedIndicators() {
        const ids = [...selectedIndicatorIds];
        if (!ids.length) return false;
        const loadId = ++evaluationSerial;
        activeRecords = [];
        const { signal, timeframe } = currentChart();
        const selectedPane = registeredIndicators.find(item => ids.includes(String(item.id)) && String(item.id) === NWE_MAX_OUTSIDE_DISTANCE_ID)
            || registeredIndicators.find(item => ids.includes(String(item.id)) && item.name === DSS_INDICATOR_NAME);
        if (signal) {
            signal.pine_indicator_outputs = [];
            setRegisteredPaneState(signal, selectedPane, selectedPane ? 'loading' : '', timeframe, 'Loading the registered indicator source.');
            getDashboardBinding('drawCandleChartNow')?.(signal);
        }
        setStatus('Loading ' + ids.length + ' selected Pine source' + (ids.length === 1 ? '' : 's') + '…', 'loading');
        try {
            const records = await Promise.all(ids.map(loadRecord));
            if (loadId !== evaluationSerial) return false;
            const rendered = await runRecords(records);
            if (rendered) dialog?.close();
            return rendered;
        } catch (error) {
            if (loadId !== evaluationSerial) return false;
            setRegisteredPaneState(signal, selectedPane, 'error', timeframe, `Could not load the saved source: ${error.message}`);
            getDashboardBinding('drawCandleChartNow')?.(signal);
            setStatus(`Could not load selected indicators: ${error.message}. Retry after the app service recovers.`, 'error');
            return false;
        }
    }

    document.getElementById('openPineIndicators')?.addEventListener('click', async () => {
        if (!dialog?.open) dialog?.showModal();
        await Promise.all([
            ensureWorker(),
            loadSavedIndicators(),
            loadCapabilities().catch(error => {
                setStatus(`Runtime capabilities unavailable: ${error.message}. Source can still be recovered after the service restarts.`, 'error');
            }),
        ]);
    });
    document.getElementById('closePineIndicators')?.addEventListener('click', () => dialog?.close());
    dialog?.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });
    copySourceButton?.addEventListener('click', async () => {
        if (!sourceViewerCode?.value) return;
        try {
            await navigator.clipboard.writeText(sourceViewerCode.value);
            if (sourceViewerStatus) sourceViewerStatus.textContent = 'Pine source copied to the clipboard.';
        } catch {
            sourceViewerCode.focus();
            sourceViewerCode.select();
            if (sourceViewerStatus) sourceViewerStatus.textContent = 'Clipboard access is unavailable; the source is selected for copying with Ctrl+C.';
        }
    });
    document.getElementById('retryPineRuntime')?.addEventListener('click', async () => {
        workerReady = false;
        await Promise.all([ensureWorker(), loadSavedIndicators()]);
        if (workerReady && activeRecords.length) await runRecords(activeRecords);
    });
    document.getElementById('validatePineIndicator')?.addEventListener('click', () => { void validateSource().catch(error => setStatus(error.message, 'error')); });
    sourceInput?.addEventListener('input', () => {
        const cleaned = cleanCopiedPineSource(sourceInput.value);
        if (cleaned.changed) {
            sourceInput.value = cleaned.source;
        }
        if (/\bstrategy\s*\(/i.test(sourceInput.value)) scriptTypeSelect.value = 'strategy';
        else if (/\bindicator\s*\(/i.test(sourceInput.value)) scriptTypeSelect.value = 'indicator';
        if (registrationButton) registrationButton.textContent = scriptTypeSelect.value === 'strategy' ? 'Register strategy' : 'Register and use indicator';
        if (/\b(?:indicator|strategy)\s*\(/i.test(sourceInput.value) && /CC BY-NC-SA 4\.0/i.test(sourceInput.value)) prefillMetadataFromSource();
    });
    scriptTypeSelect?.addEventListener('change', () => {
        if (registrationButton) registrationButton.textContent = scriptTypeSelect.value === 'strategy' ? 'Register strategy' : 'Register and use indicator';
    });
    document.getElementById('pineIndicatorForm')?.addEventListener('submit', event => { event.preventDefault(); void registerCurrentSource().catch(error => setStatus(error.message, 'error')); });
    useButton?.addEventListener('click', () => { void useSelectedIndicators(); });
    document.getElementById('pineIndicatorRightsBasis')?.addEventListener('change', event => {
        rightsBasisWasChosen = true;
        const license = document.getElementById('pineIndicatorLicense');
        if (license) license.required = event.target.value !== 'original';
    });
    document.getElementById('pineIndicatorFile')?.addEventListener('change', async event => {
        const file = event.target.files?.[0];
        if (!file) return;
        try { if (!maxSourceBytes) await loadCapabilities(); }
        catch (error) { setStatus(`Could not load source limit: ${error.message}`, 'error'); event.target.value = ''; return; }
        if (file.size > maxSourceBytes) { setStatus('File exceeds the runtime source limit.', 'error'); event.target.value = ''; return; }
        try {
            const loaded = cleanCopiedPineSource(await file.text());
            sourceInput.value = loaded.source;
            prefillMetadataFromSource();
            setStatus(`Loaded ${file.name}; source stays in this editor until you register it.`, 'ready');
        }
        catch (error) { setStatus(`Could not read Pine file: ${error.message}`, 'error'); }
    });
    window.addEventListener('quant:pine-chart-data-changed', () => { if (activeRecords.length) void runRecords(activeRecords); });

    publishDashboardBindings('pine-indicators', {
        runPineIndicator: { get: () => record => runRecords(Array.isArray(record) ? record : [record]) },
        reloadPineIndicators: { get: () => loadSavedIndicators },
    });
}
