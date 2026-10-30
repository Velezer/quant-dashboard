import { getDashboardBinding } from './runtime-context.js';
import { QuantChart } from './chart-math.js';
import { marketSymbolForSignal } from './chart-market-identity.js';
import {
    APP_LABELS,
    signalPriceBarAgeLabel,
    signalScoreState,
    signalScoreSummary,
    signalTotalStrength,
} from './signal-screener-model.js';

export function createSignalScreenerChartContext({
    getAvailableTimeframes,
    getRows,
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
        const markovButton = document.getElementById('openSelectedSignalMarkov');
        const symbolBadge = document.getElementById('chart-symbol-badge');
        const timeframeBadge = document.getElementById('chart-signal-timeframe-badge');
        const directionBadge = document.getElementById('chart-style-badge');
        const evidenceBadge = document.getElementById('chart-evidence-badge');
        const scoreSummary = document.getElementById('chart-score-summary');
        if (!signal) {
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
            updateAppContextChip(null);
            return;
        }
        if (markovButton) markovButton.disabled = false;
        const venue = executionVenueMeta(signal);
        const marketSymbol = marketSymbolForSignal(signal);
        const product = String(signal.product_id || '').replaceAll('_', ' ');
        const category = String(signal.asset_category_id || 'asset').replaceAll('_', ' ');
        const scoreState = signalScoreState(signal);
        const directionLabel = scoreState && !scoreState.current ? `last ${signal.direction}` : signal.direction;
        if (summaryNode) summaryNode.textContent = `${venue.title} · ${product || category} · ${marketSymbol} · ${signal.timeframe.toUpperCase()} · ${directionLabel}`;
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
