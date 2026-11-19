import { getDashboardBinding } from './runtime-context.js';
import { QuantChart } from './chart-math.js';
import { marketSymbolForSignal } from './chart-market-identity.js';
import { chartCandlesForTimeframe } from './chart-timeframe-candles.js';
import {
    APP_LABELS,
    oppositeEntryExitCue,
    signalDirectionalScore,
    signalPriceBarAgeLabel,
    signalScoreState,
    signalScoreSummary,
    signalTotalStrength,
    signedDirectionScore,
} from './signal-screener-model.js';

export function createSignalScreenerChartContext({
    getAvailableTimeframes,
    getRows,
    getSignalStrategies,
    scoreModelFor,
    formatPrice,
    renderSignalsList,
}) {
    let chartTimeframeLoadGeneration = 0;

    function waitForUiPaint() {
        return new Promise(resolve => {
            let resolved = false;
            let fallbackTimer = 0;
            const finish = () => {
                if (resolved) return;
                resolved = true;
                window.clearTimeout(fallbackTimer);
                resolve();
            };
            fallbackTimer = window.setTimeout(finish, 250);
            if (typeof window.requestAnimationFrame === 'function' && document.visibilityState !== 'hidden') {
                window.requestAnimationFrame(() => window.setTimeout(finish, 0));
            } else {
                window.setTimeout(finish, 0);
            }
        });
    }

    function setChartTimeframeLoadStatus(state = '', timeframe = '') {
        const status = document.getElementById('chart-timeframe-load-status');
        const label = document.getElementById('chart-timeframe-load-text');
        if (!status || !label) return;
        if (!state) {
            status.hidden = true;
            delete status.dataset.state;
            label.textContent = '';
            return;
        }
        const frame = String(timeframe || '--').toUpperCase();
        status.dataset.state = state;
        status.hidden = false;
        label.textContent = state === 'history' ? `Loading ${frame} candles…` : `Updating ${frame} indicators…`;
    }

    async function loadSignalChartTimeframe(signal, timeframe) {
        const generation = ++chartTimeframeLoadGeneration;
        if (!signal) {
            setChartTimeframeLoadStatus();
            return;
        }

        const frame = String(timeframe || '').toLowerCase();
        const cachedCandles = chartCandlesForTimeframe(signal, frame);
        const hasCachedCandles = cachedCandles.length > 0;
        signal.candles = cachedCandles;
        signal._chartCandlesTimeframe = frame;
        setChartTimeframeLoadStatus(hasCachedCandles ? 'studies' : 'history', frame);

        const drawChart = getDashboardBinding('drawCandleChart');
        if (hasCachedCandles) {
            drawChart?.(signal);
        } else {
            getDashboardBinding('setChartStatus')?.(
                'loading',
                `Loading ${marketSymbolForSignal(signal)} · ${frame.toUpperCase()} chart history.`,
                { signal },
            );
        }

        const isCurrentLoad = () => generation === chartTimeframeLoadGeneration
            && getDashboardBinding('activeTimeframe') === frame
            && (getDashboardBinding('isSelectedSignal')?.(signal)
                ?? globalThis.selectedSignal?.signal_id === signal.signal_id);

        try {
            await waitForUiPaint();
            if (!isCurrentLoad()) return;
            const ensureChartData = getDashboardBinding('ensureChartData');
            const loaded = ensureChartData ? await ensureChartData(signal) : hasCachedCandles;
            if (!isCurrentLoad()) return;

            const currentSignal = globalThis.selectedSignal || signal;
            if (!loaded) {
                if (document.getElementById('chart-status-overlay')?.dataset.state !== 'error') {
                    drawChart?.(currentSignal);
                }
                return;
            }

            currentSignal.candles = chartCandlesForTimeframe(currentSignal, frame);
            currentSignal._chartCandlesTimeframe = frame;
            drawChart?.(currentSignal);
            window.dispatchEvent(new Event('quant:pine-chart-data-changed'));
            if (currentSignal.candles.length) {
                setChartTimeframeLoadStatus('studies', frame);
                await waitForUiPaint();
                if (!isCurrentLoad()) return;
                await getDashboardBinding('hydrateRustIndicators')?.(currentSignal);
            }
        } catch (error) {
            if (isCurrentLoad()) {
                const detail = String(error?.message || error).slice(0, 120);
                if (signal.candles?.length) {
                    getDashboardBinding('setIndicatorComputeStatus')?.('error', detail);
                } else {
                    getDashboardBinding('setChartStatus')?.(
                        'error',
                        `Unable to load ${marketSymbolForSignal(signal)} · ${frame.toUpperCase()}. ${detail}`,
                        { signal },
                    );
                }
            }
        } finally {
            if (generation === chartTimeframeLoadGeneration) setChartTimeframeLoadStatus();
        }
    }

    function executionVenueMeta(signal) {
        const key = String(signal?.exchange_id || signal?.exchange || signal?.venue || '').toLowerCase();
        return {
            key,
            label: (APP_LABELS[key] || key || 'App').toUpperCase(),
            title: APP_LABELS[key] || 'Selected app',
        };
    }

    function updateAppContextChip(signal) {
        const chip = document.getElementById('app-context-chip');
        if (!chip) return;
        if (!signal) {
            chip.textContent = 'APP · SELECT SIGNAL';
            chip.title = 'Select a screener signal to show its app and category.';
            delete chip.dataset.exchange;
            return;
        }
        const venue = executionVenueMeta(signal);
        const marketSymbol = marketSymbolForSignal(signal);
        const product = String(signal.product_id || '').replaceAll('_', ' ').toUpperCase();
        const category = String(signal.asset_category_id || 'asset').replaceAll('_', ' ').toUpperCase();
        chip.textContent = `${venue.label}${product ? ` · ${product}` : ''} · ${category}`;
        chip.title = `${venue.title} · ${category}.`;
        chip.dataset.exchange = venue.key;
    }

    function updateChartTimeframeContext(signal = globalThis.selectedSignal) {
        const context = document.getElementById('chart-timeframe-context');
        if (!context) return;
        if (!signal) {
            context.textContent = '';
            context.removeAttribute('title');
            return;
        }
        const signalTimeframe = String(signal.timeframe || '').toLowerCase();
        const chartTimeframe = String(getDashboardBinding('activeTimeframe') || '').toLowerCase();
        context.textContent = signalTimeframe === chartTimeframe
            ? `SIGNAL · ${signalTimeframe.toUpperCase()}`
            : `SIGNAL · ${signalTimeframe.toUpperCase()} · CHART · ${chartTimeframe.toUpperCase()}`;
        context.title = `This signal was generated on ${signalTimeframe.toUpperCase()} candles. The chart is showing ${chartTimeframe.toUpperCase()} candles.`;
    }

    function updateChartTimeframeButtons() {
        const activeTimeframe = getDashboardBinding('activeTimeframe');
        document.querySelectorAll('[data-chart-timeframe]').forEach(button => {
            const selected = button.dataset.chartTimeframe === activeTimeframe;
            button.setAttribute('aria-pressed', String(selected));
            button.classList.toggle('bg-blue-600', selected);
            button.classList.toggle('text-white', selected);
            button.classList.toggle('shadow', selected);
            button.classList.toggle('text-gray-300', !selected);
            button.classList.toggle('hover:text-white', !selected);
            button.classList.toggle('hover:bg-gray-700', !selected);
        });
    }

    function renderSelectedSignalSummary(signal = globalThis.selectedSignal) {
        document.querySelector?.('.tv-terminal')?.classList.toggle('signal-chart-selected', !!signal);
        const summaryNode = document.getElementById('signal-selection-summary');
        const updatedNode = document.getElementById('signal-selection-updated');
        const refreshButton = document.getElementById('refreshSelectedSignal');
        const markovButton = document.getElementById('openSelectedSignalMarkov');
        const symbolBadge = document.getElementById('chart-symbol-badge');
        const timeframeBadge = document.getElementById('chart-signal-timeframe-badge');
        const directionBadge = document.getElementById('chart-style-badge');
        const evidenceBadge = document.getElementById('chart-evidence-badge');
        const scoreSummary = document.getElementById('chart-score-summary');
        const riskRewardBanner = document.getElementById('selectedSignalRiskRewardBanner');
        const riskRewardLabel = document.getElementById('selectedSignalRiskRewardLabel');
        const riskRewardText = document.getElementById('selectedSignalRiskReward');
        const riskRewardDetail = document.getElementById('selectedSignalRiskRewardDetail');
        const selectedSetupPanel = document.getElementById('selectedSignalSetup');
        const selectedSetupName = document.getElementById('selectedSignalSetupName');
        const selectedSetupDetail = document.getElementById('selectedSignalSetupDetail');
        const selectedWinRate = document.getElementById('selectedSignalWinRate');
        const selectedWinRateDetail = document.getElementById('selectedSignalWinRateDetail');
        const selectedDirectionScore = document.getElementById('selectedSignalDirectionScore');
        const selectedDirectionScoreDetail = document.getElementById('selectedSignalDirectionScoreDetail');
        const selectedLongArea = document.getElementById('selectedSignalLongArea');
        const selectedShortArea = document.getElementById('selectedSignalShortArea');
        const selectedZoneStatus = document.getElementById('selectedSignalSetupZoneStatus');
        const retryZonesButton = document.getElementById('retrySelectedSignalSetupZones');
        if (!signal) {
            if (refreshButton && refreshButton.dataset.refreshPending !== 'true') {
                refreshButton.disabled = true;
            }
            if (markovButton) markovButton.disabled = true;
            if (summaryNode) summaryNode.textContent = 'Select a screener row to inspect its timeframe and stored candles.';
            if (updatedNode) updatedNode.textContent = '';
            if (symbolBadge) symbolBadge.textContent = 'Select a signal';
            if (timeframeBadge) timeframeBadge.textContent = '';
            if (directionBadge) directionBadge.textContent = '';
            if (evidenceBadge) evidenceBadge.textContent = '';
            if (scoreSummary) {
                scoreSummary.textContent = '';
                scoreSummary.classList.remove('signal-chart-low-total');
            }
            if (riskRewardBanner) riskRewardBanner.hidden = true;
            if (riskRewardLabel) riskRewardLabel.textContent = '1:4 PLAN · AFTER CONFIGURED FEES';
            if (riskRewardText) riskRewardText.textContent = 'Select a signal';
            if (riskRewardDetail) riskRewardDetail.textContent = 'A planned bracket is a price scenario; it cannot guarantee a future fill or profit.';
            if (selectedSetupPanel) selectedSetupPanel.hidden = true;
            if (selectedSetupName) selectedSetupName.textContent = '—';
            if (selectedSetupDetail) selectedSetupDetail.textContent = 'Select a signal to inspect its rules.';
            if (selectedWinRate) selectedWinRate.textContent = '—';
            if (selectedWinRateDetail) selectedWinRateDetail.textContent = 'Historical holdout evidence';
            if (selectedDirectionScore) {
                selectedDirectionScore.textContent = '—';
                selectedDirectionScore.removeAttribute('data-tone');
            }
            if (selectedDirectionScoreDetail) selectedDirectionScoreDetail.textContent = 'Each current LONG gets a positive score and SHORT a negative score. The net groups same-category ticker/timeframe entries across products in the selected app; visible filters do not change it.';
            if (selectedLongArea) selectedLongArea.textContent = 'Select a signal';
            if (selectedShortArea) selectedShortArea.textContent = 'Select a signal';
            if (selectedZoneStatus) selectedZoneStatus.textContent = 'NWE zones use causal signal-timeframe bands and appear shaded on the chart.';
            if (retryZonesButton) {
                retryZonesButton.hidden = true;
                retryZonesButton.disabled = false;
                retryZonesButton.textContent = 'Request chart history';
            }
            updateAppContextChip(null);
            return;
        }
        if (refreshButton && refreshButton.dataset.refreshPending !== 'true') {
            refreshButton.disabled = false;
        }
        if (markovButton) markovButton.disabled = false;
        const venue = executionVenueMeta(signal);
        const marketSymbol = marketSymbolForSignal(signal);
        const product = String(signal.product_id || '').replaceAll('_', ' ');
        const category = String(signal.asset_category_id || 'asset').replaceAll('_', ' ');
        const scoreState = signalScoreState(signal);
        const directionLabel = scoreState && !scoreState.current ? `last ${signal.direction}` : signal.direction;
        const exitCue = oppositeEntryExitCue(signal);
        const exitText = exitCue
            ? ` · If holding ${exitCue.positionSide}: ${exitCue.provisional ? 'exit watch, wait for bar close' : 'exit cue'} (same model and timeframe)`
            : '';
        if (summaryNode) summaryNode.textContent = `${venue.title} · ${product || category} · ${marketSymbol} · ${signal.timeframe.toUpperCase()} · ${directionLabel}${exitText}`;
        if (updatedNode) updatedNode.textContent = `Price ${formatPrice(signal.current_price)} · ${signalPriceBarAgeLabel(signal.price_bar_time)}${scoreState ? ` · score bar ${signalPriceBarAgeLabel(signal.score_bar_time)}` : ''}`;
        if (symbolBadge) symbolBadge.textContent = marketSymbol;
        if (timeframeBadge) timeframeBadge.textContent = `${signal.timeframe.toUpperCase()} SIGNAL`;
        if (directionBadge) directionBadge.textContent = directionLabel.toUpperCase();
        if (evidenceBadge) evidenceBadge.textContent = String(signal.evidence_status || 'VALIDATION_PENDING').replaceAll('_', ' ');
        if (scoreSummary) {
            const model = scoreModelFor(signal);
            scoreSummary.textContent = signalScoreSummary(signal, model, scoreState);
            scoreSummary.classList.toggle('signal-chart-low-total', signalTotalStrength(signal, model)?.tone === 'low');
        }
        if (riskRewardBanner) riskRewardBanner.hidden = false;
        const bracket = signal.risk_reward_plan || signal.score_basis?.risk_reward_plan || {};
        const bracketStatus = String(bracket.status || 'UNAVAILABLE');
        const minimumRMultiple = Number(bracket.minimum_target_after_fee_r_multiple);
        const plannedRMultiple = Number(bracket.planned_after_fee_r_multiple);
        const minimumRLabel = Number.isFinite(minimumRMultiple) && minimumRMultiple > 0
            ? minimumRMultiple.toFixed(2).replace(/\.?0+$/, '')
            : null;
        const plannedBracketReady = ['PLANNED', 'REFERENCE_ONLY'].includes(bracketStatus)
            && bracket.fees_applied === true
            && minimumRLabel !== null
            && Number.isFinite(plannedRMultiple)
            && plannedRMultiple >= minimumRMultiple
            && ['entry_price', 'stop_price', 'target_price'].every(key => Number.isFinite(Number(bracket[key])) && Number(bracket[key]) > 0);
        if (riskRewardBanner) riskRewardBanner.dataset.status = plannedBracketReady
            ? bracketStatus === 'REFERENCE_ONLY' ? 'reference' : 'planned'
            : 'unavailable';
        if (riskRewardLabel) {
            riskRewardLabel.textContent = plannedBracketReady
                ? bracketStatus === 'REFERENCE_ONLY' ? `REFERENCE ONLY · PLANNED 1:${minimumRLabel} AFTER FEES` : `PLANNED 1:${minimumRLabel} AFTER FEES`
                : '1:4 PLAN UNAVAILABLE';
        }
        if (riskRewardText) {
            riskRewardText.textContent = plannedBracketReady
                ? `Entry ${formatPrice(Number(bracket.entry_price))} · Stop ${formatPrice(Number(bracket.stop_price))} · Target ${formatPrice(Number(bracket.target_price))}`
                : bracketStatus.replaceAll('_', ' ');
        }
        if (riskRewardDetail) {
            const plannedR = Number(bracket.planned_after_fee_r_multiple);
            const stopMultiple = Number(bracket.stop_atr_multiple);
            const feeModel = String(bracket.fee_model || 'configured venue fees');
            riskRewardDetail.textContent = plannedBracketReady
                ? `1:${Number.isFinite(plannedR) ? plannedR.toFixed(2) : minimumRLabel} planned net reward/risk · minimum 1:${minimumRLabel} · ${Number.isFinite(stopMultiple) ? stopMultiple.toFixed(2) : '1.00'}× ${String(bracket.timeframe || signal.timeframe || '').toUpperCase()} ATR stop · ${feeModel}${bracketStatus === 'REFERENCE_ONLY' ? ' · stale/reference close, not an executable entry' : ''}. Gaps, slippage, spread and other omitted costs can reduce realized R.`
                : String(bracket.reason || 'Fee policy or 14-bar ATR input is unavailable. The signal score remains visible.');
        }
        if (selectedSetupPanel) selectedSetupPanel.hidden = false;
        if (selectedSetupName) {
            const strategy = (getSignalStrategies?.() || []).find(item =>
                String(item.id || '') === String(signal.signal_strategy_id || ''),
            );
            const strategyName = strategy?.short_label || strategy?.label
                || signal.signal_strategy_name || signal.signal_strategy_id || 'Signal setup';
            selectedSetupName.textContent = `${directionLabel} · ${strategyName} · ${String(signal.timeframe || '--').toUpperCase()}`;
            selectedSetupName.title = selectedSetupName.textContent;
            if (selectedSetupDetail) {
                const rule = strategy?.description || strategy?.score_basis_description
                    || signal.score_basis?.description;
                selectedSetupDetail.textContent = rule || 'Strategy rule details are not included in this signal response.';
                selectedSetupDetail.title = selectedSetupDetail.textContent;
            }
        }
        const winRate = signal.win_probability && Object.keys(signal.win_probability).length > 0
            ? signal.win_probability
            : signal.score_basis?.win_probability || {};
        const winRateStatus = String(winRate.status || 'UNAVAILABLE');
        const winRatePct = Number(winRate.win_probability_pct);
        const winRateTrades = Number(winRate.sample_trades);
        const confidence = Array.isArray(winRate.confidence_interval_95_pct)
            ? winRate.confidence_interval_95_pct.map(value => value == null ? Number.NaN : Number(value))
            : [];
        const hasWinRate = ['ESTIMATED', 'PROFILE_GATE_ESTIMATE', 'TICKER_HOLDOUT_RATE'].includes(winRateStatus)
            && Number.isFinite(winRatePct) && winRatePct >= 0 && winRatePct <= 100
            && Number.isFinite(winRateTrades) && winRateTrades > 0;
        if (selectedWinRate) {
            if (!hasWinRate) {
                selectedWinRate.textContent = 'After-fee rate unavailable';
            } else if (winRateStatus === 'TICKER_HOLDOUT_RATE') {
                selectedWinRate.textContent = `Ticker holdout · ${winRatePct.toFixed(1)}%`;
            } else if (winRateStatus === 'PROFILE_GATE_ESTIMATE') {
                selectedWinRate.textContent = `Profile estimate · ${winRatePct.toFixed(1)}%`;
            } else {
                selectedWinRate.textContent = `Score-matched · ${winRatePct.toFixed(1)}%`;
            }
            selectedWinRate.title = String(winRate.basis || winRate.reason || 'After-fee historical holdout evidence.');
        }
        if (selectedWinRateDetail) {
            const confidenceText = Number.isFinite(confidence[0]) && Number.isFinite(confidence[1])
                ? ` · 95% interval ${confidence[0].toFixed(1)}–${confidence[1].toFixed(1)}%`
                : '';
            selectedWinRateDetail.textContent = hasWinRate
                ? `${Math.trunc(winRateTrades)} after-fee holdout outcomes${confidenceText}${winRateStatus === 'TICKER_HOLDOUT_RATE' ? ' · long/short pooled · not matched to setup strength' : ''}`
                : winRateStatus === 'INSUFFICIENT_SAMPLE'
                    ? `${Number.isFinite(winRateTrades) ? Math.trunc(winRateTrades) : 0} matching outcomes; 30 needed for a rate.`
                    : String(winRate.reason || 'No matching after-fee holdout result is available for this signal.');
            selectedWinRateDetail.title = selectedWinRateDetail.textContent;
        }
        const directionScore = signalDirectionalScore(signal);
        if (selectedDirectionScore) {
            selectedDirectionScore.textContent = directionScore
                ? directionScore.lastKnown
                    ? `Last ${signal.direction} ${signedDirectionScore(directionScore.score)} · no current net`
                    : directionScore.netUnavailable
                        ? `${signal.direction} ${signedDirectionScore(directionScore.score)} · net unavailable`
                        : `${signal.direction} ${signedDirectionScore(directionScore.score)} · net ${signedDirectionScore(directionScore.netScore)}`
                : 'No current score';
            if (directionScore) {
                selectedDirectionScore.dataset.tone = directionScore.score > 0 ? 'long' : directionScore.score < 0 ? 'short' : 'neutral';
            } else {
                selectedDirectionScore.removeAttribute('data-tone');
            }
            selectedDirectionScore.title = directionScore
                ? directionScore.lastKnown
                    ? `Last saved ${signal.direction} direction score ${signedDirectionScore(directionScore.score)}. It is not a current market entry and is excluded from the ticker/timeframe net.`
                    : directionScore.netUnavailable
                            ? `Signed direction score ${signedDirectionScore(directionScore.score)} for ${signal.direction}. The current ticker/timeframe net is unavailable.`
                        : `Signed direction score ${signedDirectionScore(directionScore.score)}. Net direction score is ${signedDirectionScore(directionScore.netScore)}.`
                : 'Only fresh current market-entry signals have a direction score.';
        }
        if (selectedDirectionScoreDetail) {
            selectedDirectionScoreDetail.textContent = directionScore
                ? directionScore.lastKnown
                    ? `This is the last saved direction for the retained row. It is excluded from the current ${marketSymbol} ${String(signal.timeframe || '--').toUpperCase()} net, which counts fresh market-entry signals only.`
                    : directionScore.netUnavailable
                        ? `This current row votes ${signedDirectionScore(directionScore.score)} for ${marketSymbol} ${String(signal.timeframe || '--').toUpperCase()}; a group net was not supplied. Setup strength and win rate are separate.`
                        : `${directionScore.longCount} LONG · ${directionScore.shortCount} SHORT for ${marketSymbol} ${String(signal.timeframe || '--').toUpperCase()} across products in the selected app. Opposite scores cancel; this is separate from setup strength and win rate.`
                : 'Stale, held, and non-qualifying rows have no current direction score. This does not change setup strength or win rate.';
            selectedDirectionScoreDetail.title = selectedDirectionScoreDetail.textContent;
        }
        if (selectedLongArea) selectedLongArea.textContent = 'Loading signal-timeframe NWE…';
        if (selectedShortArea) selectedShortArea.textContent = 'Loading signal-timeframe NWE…';
        if (selectedZoneStatus) selectedZoneStatus.textContent = `Loading ${String(signal.timeframe || '--').toUpperCase()} chart history for the zones…`;
        if (retryZonesButton) {
            retryZonesButton.hidden = true;
            retryZonesButton.disabled = false;
            retryZonesButton.textContent = 'Request chart history';
        }
        updateAppContextChip(signal);
        updateChartTimeframeContext(signal);
    }

    function prepareChartForSignal(signal) {
        if (!signal) return;
        const availableTimeframes = getAvailableTimeframes();
        const nextTimeframe = availableTimeframes.includes(signal.timeframe)
            ? signal.timeframe
            : (availableTimeframes[0] || signal.timeframe);
        globalThis.activeTimeframe = nextTimeframe;
        getDashboardBinding('updateChartTimeframeButtons')?.();
        globalThis.windowPanOffset = 0;
        globalThis.chartPriceScale = 1;
        globalThis.chartPriceOffset = 0;
        globalThis.chartIndicatorScale = { min: 0, max: 100 };
        if (globalThis.chartCrosshair) globalThis.chartCrosshair.active = false;
        globalThis.chartViewport = null;
        getDashboardBinding('clearMomentumSummary')?.();
        getDashboardBinding('updateChartTimeframeContext')?.(signal);
        getDashboardBinding('resetOHLCBarForSignal')?.(signal);
        updateAppContextChip(signal);
        renderSelectedSignalSummary(signal);
        void getDashboardBinding('fetchChartStorageStats')?.(signal);
        void loadSignalChartTimeframe(signal, nextTimeframe);
    }

    function selectSignal(signalOrId) {
        const rows = getRows();
        const signal = typeof signalOrId === 'string'
            ? rows.find(row => row.signal_id === signalOrId)
            : signalOrId;
        if (!signal) return;
        const changed = QuantChart.key(signal) !== QuantChart.key(globalThis.selectedSignal);
        const previousSignal = globalThis.selectedSignal;
        if (changed && previousSignal) {
            window.dispatchEvent(new CustomEvent('quant:signal-selection-changing', {
                detail: { signal: previousSignal, nextSignal: signal },
            }));
            getDashboardBinding('releaseChartDataForSignal')?.(previousSignal);
        }
        globalThis.selectedSignal = signal;
        if (changed) window.dispatchEvent(new Event('quant:pine-backtest-context-changed'));
        renderSignalsList();
        renderSelectedSignalSummary(signal);
        if (changed) prepareChartForSignal(signal);
    }

    function clearSelectedSignalChart(message = 'No matching signal is selected.') {
        chartTimeframeLoadGeneration += 1;
        setChartTimeframeLoadStatus();
        const previousSignal = globalThis.selectedSignal;
        if (previousSignal) {
            window.dispatchEvent(new CustomEvent('quant:signal-selection-changing', {
                detail: { signal: previousSignal, nextSignal: null },
            }));
            getDashboardBinding('releaseChartDataForSignal')?.(previousSignal);
        }
        globalThis.selectedSignal = null;
        window.dispatchEvent(new Event('quant:pine-backtest-context-changed'));
        globalThis.chartPendingSignal = null;
        globalThis.chartViewport = null;
        globalThis.windowPanOffset = 0;
        globalThis.chartCrosshair && (globalThis.chartCrosshair.active = false);
        getDashboardBinding('clearMomentumSummary')?.();
        getDashboardBinding('updateNadarayaWatsonStatus')?.([], null);
        getDashboardBinding('updateMarkovRegimeStatus')?.(null);
        updateAppContextChip(null);
        renderSelectedSignalSummary(null);
        updateChartTimeframeContext(null);
        getDashboardBinding('resetOHLCBarForSignal')?.(null);
        getDashboardBinding('updateChartDataDiagnostics')?.(null);
        getDashboardBinding('updateChartDataSource')?.(null);
        getDashboardBinding('updateChartDataWarning')?.(null);
        const canvas = document.getElementById('candleCanvas');
        const context = canvas?.getContext?.('2d');
        if (context) context.clearRect(0, 0, canvas.width, canvas.height);
        const indicatorCanvas = document.getElementById('dssCanvas');
        const indicatorContext = indicatorCanvas?.getContext?.('2d');
        if (indicatorContext) indicatorContext.clearRect(0, 0, indicatorCanvas.width, indicatorCanvas.height);
        canvas?.setAttribute('aria-label', `No signal selected. ${message}`);
        getDashboardBinding('setChartStatus')?.('no_selection', message);
    }

    function setChartTimeframe(timeframe) {
        const next = String(timeframe || '').toLowerCase();
        const availableTimeframes = getAvailableTimeframes();
        if (!availableTimeframes.includes(next) || next === getDashboardBinding('activeTimeframe')) return;
        globalThis.activeTimeframe = next;
        window.dispatchEvent(new Event('quant:pine-backtest-context-changed'));
        getDashboardBinding('updateChartTimeframeButtons')?.();
        const signal = globalThis.selectedSignal;
        if (!signal) return;
        signal.candles = chartCandlesForTimeframe(signal, next);
        signal._chartCandlesTimeframe = next;
        updateChartTimeframeContext(signal);
        getDashboardBinding('resetOHLCBarForSignal')?.(signal);
        void loadSignalChartTimeframe(signal, next);
    }

    function initChartTimeframeControls() {
        document.querySelectorAll('[data-chart-timeframe]').forEach(button => {
            if (button.dataset.bound) return;
            button.dataset.bound = 'true';
            button.addEventListener('click', () => setChartTimeframe(button.dataset.chartTimeframe));
        });
        updateChartTimeframeButtons();
    }

    return {
        executionVenueMeta,
        updateAppContextChip,
        updateChartTimeframeContext,
        updateChartTimeframeButtons,
        renderSelectedSignalSummary,
        prepareChartForSignal,
        selectSignal,
        clearSelectedSignalChart,
        setChartTimeframe,
        initChartTimeframeControls,
    };
}
