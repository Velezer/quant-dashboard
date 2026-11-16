import { getDashboardBinding } from './runtime-context.js';
import { QuantChart } from './chart-math.js';
import { marketSymbolForSignal } from './chart-market-identity.js';
import {
    APP_LABELS,
    DIRECTIONAL_VOTE_POINTS,
    oppositeEntryExitCue,
    signalPriceBarAgeLabel,
    signalScoreState,
    signalScoreSummary,
    signalTotalStrength,
} from './signal-screener-model.js';

export function createSignalScreenerChartContext({
    getAvailableTimeframes,
    getRows,
    getSignalStrategies,
    scoreModelFor,
    formatPrice,
    renderSignalsList,
}) {
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
        const selectedSetupPanel = document.getElementById('selectedSignalSetup');
        const selectedSetupName = document.getElementById('selectedSignalSetupName');
        const selectedSetupDetail = document.getElementById('selectedSignalSetupDetail');
        const selectedWinRate = document.getElementById('selectedSignalWinRate');
        const selectedWinRateDetail = document.getElementById('selectedSignalWinRateDetail');
        const selectedDirectionalVote = document.getElementById('selectedSignalDirectionalVote');
        const selectedDirectionalVoteDetail = document.getElementById('selectedSignalDirectionalVoteDetail');
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
            if (selectedSetupPanel) selectedSetupPanel.hidden = true;
            if (selectedSetupName) selectedSetupName.textContent = '—';
            if (selectedSetupDetail) selectedSetupDetail.textContent = 'Select a signal to inspect its rules.';
            if (selectedWinRate) selectedWinRate.textContent = '—';
            if (selectedWinRateDetail) selectedWinRateDetail.textContent = 'Historical holdout evidence';
            if (selectedDirectionalVote) {
                selectedDirectionalVote.textContent = '—';
                selectedDirectionalVote.removeAttribute('data-tone');
            }
            if (selectedDirectionalVoteDetail) selectedDirectionalVoteDetail.textContent = `Each current LONG contributes +${DIRECTIONAL_VOTE_POINTS} and SHORT −${DIRECTIONAL_VOTE_POINTS}. Net groups same-category ticker/timeframe entries across products in the selected app; visible filters do not change it.`;
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
                ? `${Math.trunc(winRateTrades)} after-fee holdout outcomes${confidenceText}${winRateStatus === 'TICKER_HOLDOUT_RATE' ? ' · long/short pooled · not score-matched' : ''}`
                : winRateStatus === 'INSUFFICIENT_SAMPLE'
                    ? `${Number.isFinite(winRateTrades) ? Math.trunc(winRateTrades) : 0} matching outcomes; 30 needed for a rate.`
                    : String(winRate.reason || 'No matching after-fee holdout result is available for this signal.');
            selectedWinRateDetail.title = selectedWinRateDetail.textContent;
        }
        const directionalVote = signal.directional_vote || {};
        const ownVote = Number(directionalVote.signal_score);
        const netVote = Number(directionalVote.net_score);
        const longVoteCount = Number(directionalVote.long_count);
        const shortVoteCount = Number(directionalVote.short_count);
        const hasDirectionalVote = [DIRECTIONAL_VOTE_POINTS, -DIRECTIONAL_VOTE_POINTS].includes(ownVote)
            && Number.isInteger(netVote)
            && Number.isInteger(longVoteCount) && longVoteCount >= 0
            && Number.isInteger(shortVoteCount) && shortVoteCount >= 0;
        const signedVote = value => value > 0 ? `+${value}` : String(value);
        if (selectedDirectionalVote) {
            selectedDirectionalVote.textContent = hasDirectionalVote
                ? `${signal.direction} ${signedVote(ownVote)} · net ${signedVote(netVote)}`
                : 'No current vote';
            if (hasDirectionalVote) {
                selectedDirectionalVote.dataset.tone = netVote > 0 ? 'long' : netVote < 0 ? 'short' : 'neutral';
            } else {
                selectedDirectionalVote.removeAttribute('data-tone');
            }
            selectedDirectionalVote.title = hasDirectionalVote
                ? `One ±${DIRECTIONAL_VOTE_POINTS} vote per current setup. Net score is ${signedVote(netVote)}.`
                : 'Only fresh current market-entry signals contribute votes.';
        }
        if (selectedDirectionalVoteDetail) {
            selectedDirectionalVoteDetail.textContent = hasDirectionalVote
                ? `${longVoteCount} LONG · ${shortVoteCount} SHORT for ${marketSymbol} ${String(signal.timeframe || '--').toUpperCase()} across products in the selected app. Visible filters do not change this net.`
                : 'Stale, held, and non-qualifying rows do not vote. This does not change setup strength or win rate.';
            selectedDirectionalVoteDetail.title = selectedDirectionalVoteDetail.textContent;
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
        const ensureChartData = getDashboardBinding('ensureChartData');
        if (ensureChartData) void Promise.resolve(ensureChartData(signal)).then(() => {
            if (globalThis.selectedSignal?.signal_id !== signal.signal_id) return;
            getDashboardBinding('hydrateRustIndicators')?.(signal);
            getDashboardBinding('drawCandleChart')?.(globalThis.selectedSignal);
            window.dispatchEvent(new Event('quant:pine-chart-data-changed'));
        });
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
        signal.candles = signal.candles_by_tf?.[next] || [];
        updateChartTimeframeContext(signal);
        getDashboardBinding('resetOHLCBarForSignal')?.(signal);
        const ensureChartData = getDashboardBinding('ensureChartData');
        if (ensureChartData) void Promise.resolve(ensureChartData(signal)).then(async loaded => {
            if (!loaded
                || globalThis.selectedSignal?.signal_id !== signal.signal_id
                || getDashboardBinding('activeTimeframe') !== next) return;
            await getDashboardBinding('hydrateRustIndicators')?.(signal);
            if (globalThis.selectedSignal?.signal_id !== signal.signal_id
                || getDashboardBinding('activeTimeframe') !== next) return;
            getDashboardBinding('drawCandleChart')?.(globalThis.selectedSignal);
            window.dispatchEvent(new Event('quant:pine-chart-data-changed'));
        });
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
