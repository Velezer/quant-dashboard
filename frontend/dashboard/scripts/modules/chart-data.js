import { publishDashboardBindings } from './runtime-context.js';
import { QuantChart } from './chart-math.js';
import { readApiResponse } from './api.js';
import { pineChartPlots } from './pine-chart-output.js';
import { marketSymbolForSignal, marketVenueForSignal } from './chart-market-identity.js';
import { CHART_INDICATOR_IDS, finiteChartIndicatorValue, normalizeChartIndicators } from './chart-indicator-data.js';
import { formatChartCandleTimestamp } from './chart-candle-time.js';
import { describeMarkovRegimeStatus, describeNweEnvelopeStatus } from './chart-indicator-status.js';

export const CHART_BUNDLE_HISTORY_BARS = 500;
export { CHART_INDICATOR_IDS } from './chart-indicator-data.js';

/** Initializes the chart data dashboard feature. */
export function installChartData() {
            let windowPanOffset = 0;
            let showMarkovRegimeIndicator = true;
            let showVolumeDelta = false;

            function updateVolumeDeltaStatus(candles = [], signal = selectedSignal) {
                const status = document.getElementById('volumeDeltaStatus');
                const button = document.getElementById('toggleVolumeDeltaBtn');
                const legend = document.getElementById('legendVolumeDelta');
                if (button) {
                    button.textContent = showVolumeDelta ? 'VOL Δ ON' : 'VOL Δ OFF';
                    button.setAttribute('aria-pressed', String(showVolumeDelta));
                    button.classList.toggle('bg-teal-700', showVolumeDelta);
                    button.classList.toggle('text-white', showVolumeDelta);
                    button.classList.toggle('border-teal-500', showVolumeDelta);
                    button.classList.toggle('text-gray-400', !showVolumeDelta);
                    button.classList.toggle('border-gray-600/50', !showVolumeDelta);
                }
                if (legend) legend.classList.toggle('hidden', !showVolumeDelta);
                if (status) {
                    status.classList.toggle('hidden', !showVolumeDelta);
                    if (!showVolumeDelta) return;
                    const available = (Array.isArray(candles) ? candles : [])
                        .some(candle => Number.isFinite(Number(candle?.volume)) && Number(candle.volume) > 0);
                    status.textContent = !signal
                        ? 'VOL Δ: Select a signal'
                        : available
                            ? `VOL Δ estimate · ${String(activeTimeframe || 'chart').toUpperCase()}`
                            : 'VOL Δ: Volume unavailable';
                    status.title = available
                        ? 'Research estimate from selected chart bars. Up/down direction is inferred by comparing chart-bar closes; TradingView lower-timeframe and 1 tick aggregation are unavailable.'
                        : 'The selected symbol has no usable volume history. Choose a symbol with volume data or reload chart history.';
                }
            }

            function toggleVolumeDelta() {
                showVolumeDelta = !showVolumeDelta;
                updateVolumeDeltaStatus(getDashboardBinding('getChartCandles')?.() || []);
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function updateMarkovRegimeStatus(signal = selectedSignal) {
                const status = document.getElementById('markov-regime-status');
                const button = document.getElementById('toggleMarkovRegimeBtn');
                if (button) {
                    button.textContent = showMarkovRegimeIndicator ? 'REGIME ON' : 'REGIME OFF';
                    button.setAttribute('aria-pressed', String(showMarkovRegimeIndicator));
                    button.title = 'Show or hide causal Markov regime states calculated from this chart’s completed bars. Historical states describe context, not forecasts.';
                    button.classList.toggle('bg-fuchsia-700', showMarkovRegimeIndicator);
                    button.classList.toggle('text-white', showMarkovRegimeIndicator);
                    button.classList.toggle('border-fuchsia-500', showMarkovRegimeIndicator);
                    button.classList.toggle('text-gray-400', !showMarkovRegimeIndicator);
                    button.classList.toggle('border-gray-600/50', !showMarkovRegimeIndicator);
                }
                if (!status) return;
                const description = describeMarkovRegimeStatus({
                    enabled: showMarkovRegimeIndicator,
                    signal,
                    timeframe: activeTimeframe,
                });
                status.textContent = description.text;
                status.setAttribute('aria-label', description.ariaLabel);
                status.title = description.ariaLabel;
            }

            function toggleMarkovRegimeIndicator() {
                showMarkovRegimeIndicator = !showMarkovRegimeIndicator;
                updateMarkovRegimeStatus();
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            // Liquidation Zones Visibility Toggle (Default FALSE: Do not show liquidation area)
            let showLiquidationZones = false;

            function toggleLiquidationZones() {
                showLiquidationZones = !showLiquidationZones;
                const btn = document.getElementById('toggleLiqBtn');
                if (btn) {
                    btn.innerText = showLiquidationZones ? 'LIQ ON' : 'LIQ OFF';
                    btn.classList.toggle('bg-purple-600', showLiquidationZones);
                    btn.classList.toggle('text-white', showLiquidationZones);
                    btn.classList.toggle('border-purple-500', showLiquidationZones);
                    btn.classList.toggle('text-gray-400', !showLiquidationZones);
                    btn.classList.toggle('border-gray-600/50', !showLiquidationZones);
                }
                const leg = document.getElementById('legendLiqIndicator');
                if (leg) {
                    leg.style.opacity = showLiquidationZones ? '1' : '0.35';
                }
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            // The supplied LuxAlgo reference defaults to repainting mode. Endpoint
            // mode is available for stable, one-sided historical values.
            let showNadarayaWatsonEnvelope = true;
            let nweRepaintingMode;

            function nadarayaWatsonRepaintingEnabled(signal = selectedSignal) {
                if (typeof nweRepaintingMode === 'boolean') return nweRepaintingMode;
                return signal?.chart_indicator_config?.nadaraya_watson_envelope?.repainting_default === true;
            }

            function activeNadarayaWatsonSeries(candle) {
                const envelope = candle?.indicators?.[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope];
                return envelope?.[nadarayaWatsonRepaintingEnabled() ? 'repainting' : 'endpoint'] || null;
            }

            function toggleNadarayaWatsonEnvelope() {
                showNadarayaWatsonEnvelope = !showNadarayaWatsonEnvelope;
                const btn = document.getElementById('toggleNweEnvelopeBtn');
                if (btn) {
                    btn.innerText = showNadarayaWatsonEnvelope ? 'NWE ON' : 'NWE OFF';
                    btn.classList.toggle('bg-violet-700', showNadarayaWatsonEnvelope);
                    btn.classList.toggle('text-white', showNadarayaWatsonEnvelope);
                    btn.classList.toggle('border-violet-500', showNadarayaWatsonEnvelope);
                    btn.classList.toggle('text-gray-400', !showNadarayaWatsonEnvelope);
                    btn.classList.toggle('border-gray-600/50', !showNadarayaWatsonEnvelope);
                    btn.setAttribute('aria-pressed', String(showNadarayaWatsonEnvelope));
                }
                const legend = document.getElementById('legendNweEnvelope');
                if (legend) legend.style.opacity = showNadarayaWatsonEnvelope ? '1' : '0.35';
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function disableBuiltInNweForPineReplacement() {
                if (showNadarayaWatsonEnvelope) toggleNadarayaWatsonEnvelope();
            }

            function toggleNadarayaWatsonRepainting() {
                nweRepaintingMode = !nadarayaWatsonRepaintingEnabled();
                const btn = document.getElementById('toggleNweRepaintBtn');
                if (btn) {
                    btn.innerText = nweRepaintingMode ? 'REPAINT ON' : 'ENDPOINT';
                    btn.setAttribute('aria-pressed', String(nweRepaintingMode));
                    btn.classList.toggle('bg-amber-700', nweRepaintingMode);
                    btn.classList.toggle('text-white', nweRepaintingMode);
                    btn.classList.toggle('border-amber-500', nweRepaintingMode);
                    btn.classList.toggle('text-gray-400', !nweRepaintingMode);
                    btn.classList.toggle('border-gray-600/50', !nweRepaintingMode);
                }
                updateNadarayaWatsonStatus(selectedSignal?.candles || [], selectedSignal);
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function updateNadarayaWatsonStatus(candles = [], signal = selectedSignal) {
                const status = document.getElementById('nwe-envelope-status');
                const button = document.getElementById('toggleNweEnvelopeBtn');
                if (!status) return;
                const config = signal?.chart_indicator_config?.nadaraya_watson_envelope;
                const repaintingMode = nadarayaWatsonRepaintingEnabled(signal);
                if (button && config) {
                    const method = config.method_label || 'Nadaraya-Watson Envelope';
                    const attribution = [config.attribution, config.license].filter(Boolean).join(' · ');
                    const licenseUrl = config.license_url ? ` ${config.license_url}` : '';
                    const modeDescription = repaintingMode
                        ? 'Repainting mode uses later bars to smooth historical points; historical values can change.'
                        : 'Endpoint mode uses current and earlier bars only; past values stay stable.';
                    button.title = `${method}; ${config.window_bars} bar window; ${config.bandwidth_bars} bar bandwidth; ${config.envelope_multiplier}× mean absolute error. ${modeDescription} Research study only. ${attribution}${licenseUrl}`;
                }
                const modeButton = document.getElementById('toggleNweRepaintBtn');
                if (modeButton) {
                    modeButton.innerText = repaintingMode ? 'REPAINT ON' : 'ENDPOINT';
                    modeButton.title = repaintingMode
                        ? 'Repainting uses later bars for historical points. Historical values may change as the series grows. Select Endpoint for one-sided values.'
                        : 'Endpoint mode uses only current and earlier bars. Select Repaint to show centered historical smoothing.';
                    modeButton.setAttribute('aria-pressed', String(repaintingMode));
                }
                if (!showNadarayaWatsonEnvelope) {
                    status.textContent = 'Built-in NWE: Off';
                    status.setAttribute('aria-label', 'Built-in Nadaraya-Watson overlay is off. A registered Pine NWE, if selected, is rendered separately.');
                    return;
                }
                if (!signal) {
                    status.textContent = 'NWE: Select a signal';
                    status.setAttribute('aria-label', 'Select a signal to show the Nadaraya-Watson research study.');
                    return;
                }
                const description = describeNweEnvelopeStatus({
                    candles,
                    envelope: activeNadarayaWatsonSeries(candles[candles.length - 1]),
                    timeframe: activeTimeframe,
                    repaintingMode,
                    endpointWarmupBars: Number(config?.endpoint_envelope_warmup_bars),
                    stale: signal?.chart_freshness?.[activeTimeframe]?.is_stale === true,
                });
                status.textContent = description.text;
                status.setAttribute('aria-label', description.ariaLabel);
            }

            // Logarithmic / Linear Price Scale
            let chartLogScale = false;

            function toggleLogScale() {
                chartLogScale = !chartLogScale;
                const btn = document.getElementById('logScaleBtn');
                if (btn) {
                    btn.innerText = chartLogScale ? 'LOG' : 'LIN';
                    btn.classList.toggle('bg-cyan-600/80', chartLogScale);
                    btn.classList.toggle('text-white', chartLogScale);
                    btn.classList.toggle('border-cyan-500', chartLogScale);
                    btn.classList.toggle('text-gray-400', !chartLogScale);
                    btn.classList.toggle('border-gray-600/50', !chartLogScale);
                }
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function getChartCandles(signal = selectedSignal) {
                if (!signal) return [];
                const meta = signal.chart_freshness?.[activeTimeframe];
                if (String(meta?.chart_status || '').toLowerCase() === 'insufficient_data') return [];
                const map = signal.candles_by_tf || {};
                return Array.isArray(map[activeTimeframe]) ? map[activeTimeframe] : (activeTimeframe === (signal.timeframe || '1d') ? (signal.candles || []) : []);
            }

            function setChartStatus(state = 'ready', detail = '', options = {}) {
                const overlay = document.getElementById('chart-status-overlay');
                if (!overlay) return;
                const signal = options.signal || selectedSignal;
                const title = document.getElementById('chart-status-title');
                const body = document.getElementById('chart-status-detail');
                const dot = document.getElementById('chart-status-dot');
                const retry = document.getElementById('chart-status-retry');
                if (retry) retry.hidden = !signal;
                if (state === 'ready') {
                    overlay.classList.add('hidden');
                    overlay.removeAttribute('data-state');
                    return;
                }
                const labels = {
                    loading: 'Loading chart',
                    empty: 'No chart bars',
                    error: 'Chart needs recovery',
                    no_selection: 'Select a signal'
                };
                const dotClasses = {
                    loading: 'w-2 h-2 rounded-full bg-amber-400 animate-pulse',
                    empty: 'w-2 h-2 rounded-full bg-rose-400',
                    error: 'w-2 h-2 rounded-full bg-rose-400 animate-pulse',
                    no_selection: 'w-2 h-2 rounded-full bg-gray-400'
                };
                const symbol = signal ? marketSymbolForSignal(signal) : 'selected asset';
                const timeframe = String(activeTimeframe || '--').toUpperCase();
                overlay.dataset.state = state;
                overlay.classList.remove('hidden');
                if (title) title.textContent = labels[state] || 'Chart status';
                if (body) body.textContent = detail || `${symbol} · ${timeframe}`;
                if (dot) dot.className = dotClasses[state] || dotClasses.loading;
                if (retry) {
                    retry.disabled = state === 'loading';
                    retry.setAttribute('aria-label', `Retry ${symbol} ${timeframe} chart data`);
                }
            }

            async function retryChartData() {
                const signal = selectedSignal;
                if (!signal) return;
                setChartStatus('loading', `Retrying ${marketSymbolForSignal(signal)} · ${String(activeTimeframe || '--').toUpperCase()} from durable local history.`, { signal });
                const loaded = await refreshChartData(signal);
                if (!isSelectedSignal(signal)) return;
                if (!loaded) {
                    const overlay = document.getElementById('chart-status-overlay');
                    if (overlay?.dataset.state !== 'error') {
                        setChartStatus('error', 'The chart request did not complete. Stored candles were kept; retry after the current selection and local service are ready.', { signal });
                    }
                    return;
                }
                void hydrateRustIndicators(signal);
                drawCandleChart(signal);
            }
            // Bind from the dashboard's own script scope. This keeps the chart
            // recovery action working even when the page runtime is encapsulated
            // by the embedded browser and inline handlers cannot see lexical code.
            const chartRetryButton = document.getElementById('chart-status-retry');
            if (chartRetryButton && !chartRetryButton.dataset.bound) {
                chartRetryButton.dataset.bound = 'true';
                chartRetryButton.addEventListener('click', retryChartData);
            }

            // Normal signal responses carry metadata only.  Fetch the bounded Rust
            // chart bundle when a signal is selected so the dashboard never embeds
            // hundreds of candles for every card. Existing signal fixtures that
            // already contain candles continue to work without a request.
            let chartBundleRequestSerial = 0;
            async function ensureChartData(signal = selectedSignal, force = false) {
                if (!signal) return false;
                if (force) {
                    signal._chartBundleAbortController?.abort();
                    signal.candles_by_tf = {};
                    signal.candles = [];
                    signal.chart_freshness = {};
                    signal.chart_data_loaded = false;
                    signal.chart_data_lazy = true;
                    // A forced retry must be able to replace a stalled request;
                    // otherwise one hung local fetch poisons every later Retry.
                    if (signal._chartBundlePromise) signal._chartBundlePromise = null;
                }
                const existing = signal.candles_by_tf || {};
                if (!force && getChartCandles(signal).length > 0) return true;
                if (!force && Object.prototype.hasOwnProperty.call(existing, activeTimeframe)) return true;
                if (!signal.chart_data_lazy) return false;
                if (signal._chartBundlePromise) {
                    const pendingTimeframe = signal._chartBundleTimeframe;
                    const pendingPromise = signal._chartBundlePromise;
                    const loaded = await pendingPromise;
                    if (pendingTimeframe === activeTimeframe || !isSelectedSignal(signal)) return loaded;
                    return await ensureChartData(signal);
                }

                const requestId = ++chartBundleRequestSerial;
                const requestToken = (signal._chartBundleRequestToken || 0) + 1;
                signal._chartBundleRequestToken = requestToken;
                const controller = new AbortController();
                signal._chartBundleAbortController = controller;
                const symbol = marketSymbolForSignal(signal);
                const exchange = String(signal.exchange || signal.Exchange || signal.venue || signal.Venue || '').trim().toLowerCase();
                const assetClass = String(signal.class || signal.asset_class || signal.Class || '').trim().toLowerCase();
                const requestTimeframe = String(activeTimeframe || signal.timeframe || '1d').trim().toLowerCase();
                signal._chartBundleTimeframe = requestTimeframe;
                const currentChartMatches = () => !!selectedSignal
                    && (isSelectedSignal(signal) || (marketSymbolForSignal(selectedSignal) === symbol
                        && String(selectedSignal.exchange || selectedSignal.Exchange || selectedSignal.venue || selectedSignal.Venue || '').trim().toLowerCase() === exchange));
                if (currentChartMatches()) {
                    setChartStatus('loading', `Loading ${symbol} · ${String(activeTimeframe || '--').toUpperCase()} from durable local history.`, { signal });
                    const markovStatus = document.getElementById('markov-regime-status');
                    if (markovStatus) markovStatus.textContent = `Markov ${String(activeTimeframe || '--').toUpperCase()}: loading chart history`;
                }
                signal._chartBundlePromise = (async () => {
                    const timeoutId = setTimeout(() => controller.abort(), 15_000);
                    try {
                        const product = String(signal.product_id || signal.Product_ID || '');
                        const storageSymbol = String(signal.storage_symbol || signal.data_symbol || signal.Data_Symbol || '');
                        const query = new URLSearchParams({
                            symbol,
                            exchange,
                            product_id: product,
                            storage_symbol: storageSymbol,
                            asset_class: assetClass,
                            timeframe: requestTimeframe,
                            limit: String(CHART_BUNDLE_HISTORY_BARS),
                        });
                        const response = await fetch(`/api/compute/chart-bundle?${query.toString()}`, {
                            cache: 'no-store',
                            signal: controller.signal
                        });
                        const payload = await readApiResponse(response);
                        // A forced retry supersedes every older response.  The
                        // selected symbol may still match, but applying an older
                        // bundle after the retry would put stale candles back on
                        // the chart.
                        if (requestId !== chartBundleRequestSerial) return false;
                        if (!response.ok || !payload || typeof payload.timeframes !== 'object') {
                            throw new Error(payload?.error || `HTTP ${response.status}`);
                        }

                        const formatCandle = (raw, timeframe) => {
                            const display = formatChartCandleTimestamp(raw?.time, timeframe) || '--';
                            return {
                                date: display,
                                open: Number(raw?.open), high: Number(raw?.high), low: Number(raw?.low), close: Number(raw?.close),
                                volume: Number(raw?.volume || 0),
                                dss: raw?.dss == null ? 50 : Number(raw.dss),
                                trigger: raw?.trigger == null ? 50 : Number(raw.trigger),
                                gradient: raw?.gradient == null ? 0 : Number(raw.gradient),
                                accel: raw?.accel == null ? 0 : Number(raw.accel),
                                indicators: normalizeChartIndicators(raw?.indicators),
                                time: raw?.time
                            };
                        };

                        const loaded = { ...(signal.candles_by_tf || {}) };
                        Object.entries(payload.timeframes).forEach(([timeframe, rows]) => {
                            loaded[timeframe] = Array.isArray(rows) ? rows.map(row => formatCandle(row, timeframe)).filter(row => [row.open, row.high, row.low, row.close].every(Number.isFinite)) : [];
                        });
                        signal.candles_by_tf = loaded;
                        signal.chart_freshness = {
                            ...(signal.chart_freshness || {}),
                            ...(payload.series && typeof payload.series === 'object' ? payload.series : {}),
                        };
                        signal.markov_regimes_by_tf = {
                            ...(signal.markov_regimes_by_tf || {}),
                            ...(payload.markov_regimes && typeof payload.markov_regimes === 'object' ? payload.markov_regimes : {}),
                        };
                        signal.chart_indicator_config = payload.indicator_config && typeof payload.indicator_config === 'object' ? payload.indicator_config : {};
                        signal.candles = loaded[requestTimeframe] || [];
                        signal.chart_data_loaded = true;
                        const selectedChartSignal = currentChartMatches() ? selectedSignal : null;
                        if (selectedChartSignal) {
                            // renderSignalsList may replace the normalized object while
                            // this request is in flight. Transfer the loaded bundle
                            // to that current object before repainting.
                            selectedChartSignal.candles_by_tf = loaded;
                            selectedChartSignal.chart_freshness = signal.chart_freshness;
                            selectedChartSignal.markov_regimes_by_tf = signal.markov_regimes_by_tf;
                            selectedChartSignal.chart_indicator_config = signal.chart_indicator_config;
                            selectedChartSignal.candles = loaded[requestTimeframe] || signal.candles;
                            selectedChartSignal.chart_data_loaded = true;
                            if (activeTimeframe === requestTimeframe) {
                                void hydrateRustIndicators(selectedChartSignal);
                                drawCandleChart(selectedChartSignal);
                                window.dispatchEvent(new Event('quant:pine-chart-data-changed'));
                            }
                        }
                        return true;
                    } catch (error) {
                        if (requestId !== chartBundleRequestSerial) return false;
                        signal.chart_data_loaded = false;
                        if (currentChartMatches()) {
                            const reason = error?.name === 'AbortError' ? 'local chart request timed out after 15s' : String(error?.message || error || 'Chart service unavailable');
                            setIndicatorComputeStatus('error', `chart data: ${reason.slice(0, 100)}`);
                            const markovStatus = document.getElementById('markov-regime-status');
                            if (markovStatus) {
                                markovStatus.textContent = `Markov ${String(activeTimeframe || '--').toUpperCase()}: unavailable · retry chart data`;
                                markovStatus.setAttribute('aria-label', `Markov regime history unavailable because chart data failed to load. Retry chart data to recover. ${reason.slice(0, 140)}`);
                            }
                            setChartStatus('error', `Unable to load ${symbol} · ${String(activeTimeframe || '--').toUpperCase()}. ${reason.slice(0, 140)}`, { signal });
                        }
                        return false;
                    } finally {
                        clearTimeout(timeoutId);
                        if (signal._chartBundleAbortController === controller) signal._chartBundleAbortController = null;
                        if (signal._chartBundleRequestToken === requestToken) {
                            signal._chartBundlePromise = null;
                            signal._chartBundleTimeframe = null;
                        }
                    }
                })();
                return await signal._chartBundlePromise;
            }

            function releaseChartDataForSignal(signal) {
                if (!signal) return;
                signal._chartBundleAbortController?.abort();
                signal._rustIndicatorAbortController?.abort();
                chartBundleRequestSerial += 1;
                indicatorRequestSerial += 1;
                signal._chartBundleRequestToken = (signal._chartBundleRequestToken || 0) + 1;
                signal._chartBundleAbortController = null;
                signal._rustIndicatorAbortController = null;
                signal._chartBundlePromise = null;
                signal.candles = [];
                signal.candles_by_tf = null;
                signal.chart_freshness = null;
                signal.chart_data_loaded = false;
                signal.chart_indicator_config = null;
                signal.markov_regimes_by_tf = null;
                signal._rustIndicatorCache = null;
            }

            async function refreshChartData(signal = selectedSignal) {
                if (!signal) return false;
                return await ensureChartData(signal, true);
            }

            async function refreshSelectedChartAfterAssetUpdate(asset) {
                if (!selectedSignal || selectedSignal.asset !== asset) return true;
                const chartUpdated = await refreshChartData(selectedSignal);
                if (!chartUpdated) throw new Error('fresh candles were not available');
                return true;
            }

            // Authoritative indicator hydration.  The Rust service receives only
            // the selected bounded chart window; the signal payload remains
            // usable while a request is in flight, and a visible badge reports
            // service health instead of hiding a failed compute request.
            let indicatorRequestSerial = 0;

            function setIndicatorComputeStatus(state, detail = '') {
                const badge = document.getElementById('dssIndicatorComputeStatus');
                if (!badge) return;
                const pineDss = (Array.isArray(selectedSignal?.pine_indicator_outputs) ? selectedSignal.pine_indicator_outputs : [])
                    .find(output => output.name === 'DSS Bressert (Double Smoothed Stochastic)');
                if (pineDss) {
                    const context = pineDss.requested_data_contexts?.[0] || pineDss.data_context || 'stored app candles';
                    badge.className = 'dss-compute-status text-[10px] font-semibold px-2 py-0.5 rounded border bg-gray-800 text-gray-400 border-gray-700';
                    badge.textContent = 'Stored';
                    badge.setAttribute('aria-label', `Registered Pine output uses historical candles: ${context}`);
                    badge.title = context;
                    return;
                }
                const styles = {
                    ready: 'bg-emerald-950/70 text-emerald-300 border-emerald-800',
                    loading: 'bg-amber-950/70 text-amber-300 border-amber-800',
                    error: 'bg-rose-950/70 text-rose-300 border-rose-800',
                    waiting: 'bg-gray-800 text-gray-400 border-gray-700'
                };
                const labels = { ready: 'Updated', loading: 'Updating', error: 'Unavailable', waiting: 'Not computed' };
                const label = labels[state] || labels.waiting;
                const context = String(detail || '').trim();
                badge.className = `dss-compute-status text-[10px] font-semibold px-2 py-0.5 rounded border ${styles[state] || styles.waiting}`;
                badge.textContent = label;
                badge.setAttribute('aria-label', label);
                badge.title = state === 'error'
                    ? `Momentum values are unavailable${context ? `: ${context}` : ''}. Use Data → Retry chart data.`
                    : state === 'loading'
                        ? 'Updating momentum values.'
                        : state === 'ready'
                            ? 'Momentum values updated.'
                            : 'Momentum values appear when candle data is available.';
            }

            function clearMomentumSummary() {
                const readout = document.getElementById('dssLiveReadout');
                const signal = document.getElementById('dssStatusBadge');
                if (readout) {
                    readout.textContent = '--';
                    readout.setAttribute('aria-label', 'Momentum value unavailable');
                    readout.removeAttribute('title');
                }
                if (signal) {
                    signal.textContent = 'No reading';
                    signal.className = 'dss-signal-status text-[10px] font-semibold px-2 py-0.5 rounded border bg-gray-800 text-gray-400 border-gray-700';
                }
                setIndicatorComputeStatus('waiting');
            }

            async function hydrateRustIndicators(signal = selectedSignal) {
                if (!signal) return false;
                if ((Array.isArray(signal.pine_indicator_outputs) ? signal.pine_indicator_outputs : [])
                    .some(output => output.name === 'DSS Bressert (Double Smoothed Stochastic)')) {
                    setIndicatorComputeStatus('ready', 'Registered Pine WASM output');
                    return true;
                }
                const map = signal.candles_by_tf || {};
                const candles = Array.isArray(map[activeTimeframe])
                    ? map[activeTimeframe]
                    : (activeTimeframe === (signal.timeframe || '1d') ? (signal.candles || []) : []);
                if (!candles.length) {
                    clearMomentumSummary();
                    return false;
                }
                signal._rustIndicatorCache = signal._rustIndicatorCache || {};
                const cached = signal._rustIndicatorCache[activeTimeframe];
                if (cached && cached.count === candles.length) {
                    setIndicatorComputeStatus('ready', `${activeTimeframe} · ${cached.version || 'verified'}`);
                    return true;
                }
                const requestId = ++indicatorRequestSerial;
                signal._rustIndicatorAbortController?.abort();
                const controller = new AbortController();
                signal._rustIndicatorAbortController = controller;
                setIndicatorComputeStatus('loading', activeTimeframe);
                const symbol = marketSymbolForSignal(signal);
                const query = new URLSearchParams({
                    symbol,
                    timeframe: activeTimeframe,
                    exchange: String(signal.exchange || signal.Exchange || signal.venue || signal.Venue || '').trim().toLowerCase(),
                    asset_class: String(signal.class || signal.asset_class || signal.Class || '').trim().toLowerCase(),
                    limit: String(Math.min(5000, candles.length)),
                    pds: '10',
                    ema_len: '9',
                    trigger_len: '5'
                });
                const product = String(signal.product_id || signal.Product_ID || '');
                const storageSymbol = String(signal.storage_symbol || signal.data_symbol || signal.Data_Symbol || '');
                if (product) query.set('product_id', product);
                if (storageSymbol) query.set('storage_symbol', storageSymbol);
                try {
                    const response = await fetch(`/api/compute/indicators?${query.toString()}`, { signal: controller.signal });
                    const payload = await readApiResponse(response);
                    if (requestId !== indicatorRequestSerial) return false;
                    if (!response.ok || !Array.isArray(payload.points)) throw new Error(payload.error || `HTTP ${response.status}`);
                    const points = payload.points;
                    const offset = Math.max(0, candles.length - points.length);
                    points.forEach((point, index) => {
                        const candle = candles[offset + index];
                        if (!candle || !point) return;
                        if (point.dss != null && Number.isFinite(Number(point.dss))) candle.dss = Number(point.dss);
                        if (point.trigger != null && Number.isFinite(Number(point.trigger))) candle.trigger = Number(point.trigger);
                        if (point.gradient != null && Number.isFinite(Number(point.gradient))) candle.gradient = Number(point.gradient);
                        candle.indicators = normalizeChartIndicators(point.indicators);
                    });
                    if (payload.indicator_config && typeof payload.indicator_config === 'object') {
                        signal.chart_indicator_config = payload.indicator_config;
                    }
                    signal._rustIndicatorCache[activeTimeframe] = { count: candles.length, version: payload.version || 'verified' };
                    setIndicatorComputeStatus('ready', `${activeTimeframe} · ${payload.version || 'verified'}`);
                    if (isSelectedSignal(signal)) drawCandleChart(selectedSignal);
                    return true;
                } catch (error) {
                    if (requestId !== indicatorRequestSerial) return false;
                    setIndicatorComputeStatus('error', String(error?.message || error).slice(0, 120));
                    return false;
                } finally {
                    if (signal._rustIndicatorAbortController === controller) signal._rustIndicatorAbortController = null;
                }
            }

            const resolveMarketVenue = signal => marketVenueForSignal(signal || selectedSignal);
            const resolveMarketSymbol = signal => marketSymbolForSignal(signal || selectedSignal);

            function setChartWindowSize(size, anchor = 0.5) {
                const maxB = getChartCandles().length || 120;
                const view = QuantChart.zoom(maxB, activeWindowSize, windowPanOffset, size === 'all' ? maxB : Number(size) || 45, anchor);
                activeWindowSize = view.size;
                windowPanOffset = view.offset;
                const zoomReadout = document.getElementById('chartZoomReadout');
                if (zoomReadout) {
                    const scaleLabel = `Current time scale: ${Math.round(activeWindowSize)} bars`;
                    zoomReadout.textContent = `${Math.round(activeWindowSize)} bars`;
                    zoomReadout.setAttribute('aria-label', scaleLabel);
                }
            
                document.querySelectorAll('.window-preset-btn').forEach(btn => {
                    const val = btn.getAttribute('data-window');
                    if (val === 'all') {
                        btn.classList.toggle('bg-blue-600', activeWindowSize >= maxB);
                        btn.classList.toggle('text-white', activeWindowSize >= maxB);
                        btn.classList.toggle('shadow', activeWindowSize >= maxB);
                    } else {
                        const num = parseInt(val);
                        btn.classList.toggle('bg-blue-600', activeWindowSize === num);
                        btn.classList.toggle('text-white', activeWindowSize === num);
                        btn.classList.toggle('shadow', activeWindowSize === num);
                    }
                });

                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function zoomChart(delta) {
                setChartWindowSize(activeWindowSize + delta);
            }

            function handleChartWheel(event, canvas) {
                event.preventDefault();
                if (!selectedSignal || !getChartCandles().length) return;
                const rect = canvas.getBoundingClientRect();
                const pixels = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? rect.height : 1);
                const amount = Math.max(-240, Math.min(240, pixels));
                chartCrosshair.active = false;
                const onRightScale = event.clientX - rect.left >= rect.width - 96;
                if (canvas.id === 'dssCanvas' && (event.shiftKey || onRightScale)) {
                    const anchor = QuantChart.valueAtScaleY(
                        event.clientY - rect.top,
                        12,
                        Math.max(1, rect.height - 24),
                        chartIndicatorScale.min,
                        chartIndicatorScale.max,
                    );
                    if (anchor !== null) {
                        setIndicatorScale(QuantChart.zoomValueScale(
                            chartIndicatorScale.min,
                            chartIndicatorScale.max,
                            anchor,
                            Math.exp(-amount * 0.002),
                        ));
                    }
                } else if (canvas.id === 'candleCanvas' && (event.shiftKey || onRightScale)) {
                    setPriceScale(chartPriceScale * Math.exp(-amount * 0.002));
                } else {
                    const anchor = (event.clientX - rect.left - 15) / Math.max(1, rect.width - 111);
                    setChartWindowSize(activeWindowSize * Math.exp(amount * 0.002), anchor);
                }
            }

            function resetChartWindow() {
                windowPanOffset = 0;
                setChartWindowSize(45, 1);
            }

            // Vertical Price Scale Zoom & Pan State
            let chartPriceScale = 1.0;
            let chartPriceOffset = 0.0;
            let chartIndicatorScale = { min: 0, max: 100 };

            function setPriceScale(val) {
                const num = parseFloat(val);
                if (!isNaN(num)) {
                    chartPriceScale = Math.max(0.3, Math.min(3.0, num));
                    if (selectedSignal) drawCandleChart(selectedSignal);
                }
            }

            function zoomPriceScale(delta) {
                setPriceScale(chartPriceScale + delta);
            }

            function resetPriceScale() {
                chartPriceScale = 1.0;
                chartPriceOffset = 0.0;
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function setIndicatorScale(bounds) {
                const min = Number(bounds?.min);
                const max = Number(bounds?.max);
                if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) return;
                chartIndicatorScale = { min, max };
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function resetIndicatorScale() {
                const bars = selectedSignal?.candles_by_tf?.[activeTimeframe]
                    || (activeTimeframe === (selectedSignal?.timeframe || '1d') ? selectedSignal?.candles : []);
                const paneOutput = pineChartPlots(
                    selectedSignal?.pine_indicator_outputs,
                    activeTimeframe,
                    Array.isArray(bars) ? bars.length : 0,
                ).paneOutput;
                const values = (paneOutput?.plots || []).flatMap(plot => plot.values).filter(Number.isFinite);
                if (values.length) {
                    const dataMin = Math.min(0, ...values);
                    const dataMax = Math.max(100, ...values);
                    const fittedScale = QuantChart.priceScale(dataMin, dataMax, 5);
                    chartIndicatorScale = {
                        min: Math.min(dataMin, fittedScale.min),
                        max: Math.max(dataMax, fittedScale.max),
                    };
                } else {
                    chartIndicatorScale = { min: 0, max: 100 };
                }
                if (selectedSignal) drawCandleChart(selectedSignal);
            }

            function resetChartAll() {
                resetPriceScale();
                resetIndicatorScale();
                resetChartWindow();
            }

            // Initialize Countdown Timer
            function renderCountdown() {
                const timer = document.getElementById('countdown-timer');
                if (!timer) return;
                const remaining = Number.isFinite(nextScanEpoch)
                    ? Math.max(0, Math.ceil(nextScanEpoch - Date.now() / 1000))
                    : Math.max(0, Math.floor(countdownSeconds));
                countdownSeconds = remaining;
                const mins = Math.floor(remaining / 60);
                const secs = remaining % 60;
                timer.innerText = `${mins}m ${secs.toString().padStart(2, '0')}s`;
            }

            function setNextScanEpoch(value) {
                const epoch = Number(value);
                if (!Number.isFinite(epoch) || epoch <= 0) return false;
                nextScanEpoch = epoch;
                renderCountdown();
                return true;
            }

    publishDashboardBindings('chart-data', {
        windowPanOffset: { get: () => windowPanOffset, set: value => { windowPanOffset = value; } },
        showMarkovRegimeIndicator: { get: () => showMarkovRegimeIndicator, set: value => { showMarkovRegimeIndicator = value; } },
        showVolumeDelta: { get: () => showVolumeDelta },
        toggleVolumeDelta: { get: () => toggleVolumeDelta },
        updateVolumeDeltaStatus: { get: () => updateVolumeDeltaStatus },
        updateMarkovRegimeStatus: { get: () => updateMarkovRegimeStatus },
        toggleMarkovRegimeIndicator: { get: () => toggleMarkovRegimeIndicator },
        CHART_INDICATOR_IDS: { get: () => CHART_INDICATOR_IDS },
        finiteChartIndicatorValue: { get: () => finiteChartIndicatorValue },
        normalizeChartIndicators: { get: () => normalizeChartIndicators },
        showLiquidationZones: { get: () => showLiquidationZones, set: value => { showLiquidationZones = value; } },
        toggleLiquidationZones: { get: () => toggleLiquidationZones },
        showNadarayaWatsonEnvelope: { get: () => showNadarayaWatsonEnvelope, set: value => { showNadarayaWatsonEnvelope = value; } },
        nweRepaintingMode: { get: () => nweRepaintingMode, set: value => { nweRepaintingMode = value; } },
        nadarayaWatsonRepaintingEnabled: { get: () => nadarayaWatsonRepaintingEnabled },
        activeNadarayaWatsonSeries: { get: () => activeNadarayaWatsonSeries },
        toggleNadarayaWatsonEnvelope: { get: () => toggleNadarayaWatsonEnvelope },
        disableBuiltInNweForPineReplacement: { get: () => disableBuiltInNweForPineReplacement },
        toggleNadarayaWatsonRepainting: { get: () => toggleNadarayaWatsonRepainting },
        updateNadarayaWatsonStatus: { get: () => updateNadarayaWatsonStatus },
        chartLogScale: { get: () => chartLogScale, set: value => { chartLogScale = value; } },
        toggleLogScale: { get: () => toggleLogScale },
        getChartCandles: { get: () => getChartCandles },
        setChartStatus: { get: () => setChartStatus },
        retryChartData: { get: () => retryChartData },
        chartRetryButton: { get: () => chartRetryButton },
        chartBundleRequestSerial: { get: () => chartBundleRequestSerial, set: value => { chartBundleRequestSerial = value; } },
        ensureChartData: { get: () => ensureChartData },
        releaseChartDataForSignal: { get: () => releaseChartDataForSignal },
        refreshChartData: { get: () => refreshChartData },
        refreshSelectedChartAfterAssetUpdate: { get: () => refreshSelectedChartAfterAssetUpdate },
        indicatorRequestSerial: { get: () => indicatorRequestSerial, set: value => { indicatorRequestSerial = value; } },
        setIndicatorComputeStatus: { get: () => setIndicatorComputeStatus },
        clearMomentumSummary: { get: () => clearMomentumSummary },
        hydrateRustIndicators: { get: () => hydrateRustIndicators },
        marketVenueForSignal: { get: () => resolveMarketVenue },
        marketSymbolForSignal: { get: () => resolveMarketSymbol },
        setChartWindowSize: { get: () => setChartWindowSize },
        zoomChart: { get: () => zoomChart },
        handleChartWheel: { get: () => handleChartWheel },
        resetChartWindow: { get: () => resetChartWindow },
        chartPriceScale: { get: () => chartPriceScale, set: value => { chartPriceScale = value; } },
        chartPriceOffset: { get: () => chartPriceOffset, set: value => { chartPriceOffset = value; } },
        chartIndicatorScale: { get: () => chartIndicatorScale, set: value => { chartIndicatorScale = value; } },
        setPriceScale: { get: () => setPriceScale },
        setIndicatorScale: { get: () => setIndicatorScale },
        resetIndicatorScale: { get: () => resetIndicatorScale },
        zoomPriceScale: { get: () => zoomPriceScale },
        resetPriceScale: { get: () => resetPriceScale },
        resetChartAll: { get: () => resetChartAll },
        renderCountdown: { get: () => renderCountdown },
        setNextScanEpoch: { get: () => setNextScanEpoch },
    });
}
