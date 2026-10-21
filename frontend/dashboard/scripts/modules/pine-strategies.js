import { getDashboardBinding } from './runtime-context.js';
import { requestJson, requestPineCapabilities } from './api.js';
import { renderPineBacktestDiagnostics } from './pine-backtest-diagnostics.js';
import { pineBacktestCommissionLabel } from './pine-backtest-commission.js';
import { createPineBacktestJobClient, createPineBacktestJobUi, displayCandleTimeUtc, parseBacktestDateRange, setPineBacktestStatus } from './pine-backtest-jobs.js';
import { createPineStrategyInputs } from './pine-strategy-inputs.js';
export { displayCandleTimeUtc } from './pine-backtest-jobs.js';
export { parseBacktestDateRange } from './pine-backtest-jobs.js';

const STRATEGY_REGISTRY_REQUEST_TIMEOUT_MS = 10_000;


import {
    closedTradeProfitPercent, displayCount, displayNumber, magnifierCoverageSummary,
    renderPineBacktestSummaryMetrics, renderTrades, tradeDisplayValues,
} from './pine-strategy-presenters.js';
import {
    chronologicalValidationRows, monthlyBacktestRows, renderChronologicalValidation,
    renderDrawdownCurve, renderMonthlyPerformance, renderTradeAnalysis,
    renderTradeReturnDistribution, summarizeEquityCurve, tradeAnalysisRows,
    tradeReturnDistributionBins,
} from './pine-strategy-analysis-presenters.js';
export {
    closedTradeProfitPercent, chronologicalValidationRows, magnifierCoverageSummary, monthlyBacktestRows,
    tradeAnalysisRows, tradeDisplayValues, tradeReturnDistributionBins,
};

export function installPineStrategies() {
    const select = document.getElementById('pineStrategySelect');
    const historySelect = document.getElementById('pineStrategyHistoryLimit');
    const fromDateInput = document.getElementById('pineStrategyBacktestFrom');
    const toDateInput = document.getElementById('pineStrategyBacktestTo');
    const splitSelect = document.getElementById('pineStrategyWalkForwardSplit');
    const retryButton = document.getElementById('retryPineStrategies');
    const runButton = document.getElementById('runPineStrategyBacktest');
    const cancelButton = document.getElementById('cancelPineStrategyBacktest');
    const status = document.getElementById('pineStrategyBacktestStatus');
    const registryState = document.getElementById('pineStrategyRegistryState');
    const summary = document.getElementById('pineStrategyBacktestSummary');
    const inputControls = document.getElementById('pineStrategyInputControls');
    const fillAssumptions = document.getElementById('pineStrategyFillAssumptions');
    const magnifierCoverage = document.getElementById('pineStrategyMagnifierCoverage');
    const runtimeDiagnostics = document.getElementById('pineStrategyRuntimeDiagnostics');
    const pnlLabel = document.getElementById('pineBacktestPnlLabel');
    const equitySummary = document.getElementById('pineBacktestEquity');
    const monthlyPerformance = document.getElementById('pineBacktestMonthlyPerformance');
    const chronologicalValidation = document.getElementById('pineBacktestChronologicalValidation');
    const drawdownSummary = document.getElementById('pineBacktestDrawdown');
    const tradeBody = document.getElementById('pineBacktestTrades');
    const tradeLimit = document.getElementById('pineBacktestTradeLimit');
    const tradeAnalysis = document.getElementById('pineBacktestTradeAnalysis');
    const tradeReturnDistribution = document.getElementById('pineBacktestReturnDistribution');
    const tradePnlHeading = document.getElementById('pineBacktestTradePnlHeading');
    const tradeReturnHeading = document.getElementById('pineBacktestTradeReturnHeading');
    if (!select || !runButton || !status || !summary) return;
    const setStatus = (message, state = 'waiting') => setPineBacktestStatus(status, message, state);

    let strategies = [];
    let historyLimits = null;
    let strategyController;
    let strategyListsAttempted = false;
    let strategyListsPromise = null;
    let contextRevision = 0;
    const jobClient = createPineBacktestJobClient(requestJson);

    function resetSummary() {
        for (const metric of summary.querySelectorAll('[data-metric]')) metric.textContent = '—';
        if (pnlLabel) pnlLabel.textContent = 'Closed-trade P&L';
        if (fillAssumptions) {
            fillAssumptions.textContent = 'Fill assumptions appear after a successful backtest.';
            fillAssumptions.dataset.state = 'empty';
        }
        if (magnifierCoverage) {
            magnifierCoverage.textContent = 'Bar Magnifier coverage appears after a successful backtest.';
            magnifierCoverage.dataset.state = 'empty';
        }
        renderPineBacktestDiagnostics(runtimeDiagnostics, null);
        summarizeEquityCurve(equitySummary, []);
        if (monthlyPerformance) monthlyPerformance.textContent = 'Run a backtest to view monthly performance.';
        renderChronologicalValidation(chronologicalValidation, null);
        renderDrawdownCurve(drawdownSummary, null);
        renderTrades(tradeBody, [], null, tradePnlHeading, tradeReturnHeading);
        if (tradeLimit) tradeLimit.textContent = 'The recent closed-trade row limit appears after a successful backtest.';
        renderTradeAnalysis(tradeAnalysis, null);
        renderTradeReturnDistribution(tradeReturnDistribution, null);
        jobUi.resetProgress();
    }

    const jobUi = createPineBacktestJobUi({
        runButton, cancelButton, jobClient, setStatus, clearResults: resetSummary,
        canRun: () => Boolean(select.value && strategies.length && historySelect?.value && !strategyInputs.isLoading),
        currentRevision: () => contextRevision,
    });

    const updateButton = () => jobUi.updateControls();
    const strategyInputs = createPineStrategyInputs({
        select,
        inputControls,
        requestJson,
        onStateChange: updateButton,
    });

    function renderFillAssumptions(assumptions, fees) {
        if (!fillAssumptions) return;
        const slippageTicks = assumptions?.slippage_ticks;
        const limitTicks = assumptions?.backtest_fill_limits_assumption_ticks;
        if (!Number.isSafeInteger(slippageTicks) || slippageTicks < 0 || !Number.isSafeInteger(limitTicks) || limitTicks < 0) {
            fillAssumptions.textContent = 'Fill assumptions are unavailable in this response. Reload the local service before comparing this run.';
            fillAssumptions.dataset.state = 'unavailable';
            return;
        }
        const tickSize = typeof assumptions.instrument_tick_size === 'number'
            && Number.isFinite(assumptions.instrument_tick_size)
            && assumptions.instrument_tick_size > 0
            ? assumptions.instrument_tick_size
            : null;
        const tickSource = assumptions.tick_size_source === 'registered_instrument_contract_metadata'
            ? 'registered contract metadata'
            : 'unavailable';
        const pluralizeTicks = value => `${value} ${value === 1 ? 'tick' : 'ticks'}`;
        const slippage = `Market/stop slippage: ${pluralizeTicks(slippageTicks)}${slippageTicks > 0 && tickSize !== null ? ` (${displayNumber(tickSize)} price units per tick)` : ''}`;
        const limitVerification = limitTicks === 0
            ? 'Limit verification: 0 ticks (touch fills)'
            : `Limit verification: ${pluralizeTicks(limitTicks)} favorable${tickSize !== null ? ` (${displayNumber(tickSize)} price units per tick)` : ''}`;
        const tickContext = tickSize === null
            ? 'Instrument tick size: unavailable'
            : `Instrument tick size: ${displayNumber(tickSize)} (${tickSource})`;
        const fillRecalculation = assumptions.calc_on_order_fills === true
            ? 'Fill-triggered recalculation: enabled · inferred OHLC market points; full-bar OHLC lookahead and market-only limits are described in the run diagnostics.'
            : 'Fill-triggered recalculation: disabled.';
        fillAssumptions.textContent = `Fill assumptions · ${pineBacktestCommissionLabel(fees)} · ${slippage} · ${limitVerification} · ${tickContext} · ${fillRecalculation}`;
        fillAssumptions.dataset.state = 'ready';
    }

    function renderMagnifierCoverage(assumptions) {
        if (!magnifierCoverage) return;
        const coverage = magnifierCoverageSummary(assumptions);
        magnifierCoverage.textContent = coverage.text;
        magnifierCoverage.dataset.state = coverage.state;
    }

    function invalidateResult(message) {
        contextRevision += 1;
        jobUi.invalidate(message);
    }

    async function loadStrategies() {
        strategyController?.abort();
        strategyInputs.abortLoad();
        const requestController = new AbortController();
        let registryTimedOut = false;
        const timeoutId = window.setTimeout(() => {
            registryTimedOut = true;
            requestController.abort();
        }, STRATEGY_REGISTRY_REQUEST_TIMEOUT_MS);
        strategyController = requestController;
        resetSummary();
        inputControls?.replaceChildren();
        select.disabled = true;
        if (retryButton) retryButton.disabled = true;
        select.replaceChildren(new Option('Loading registered strategies…', ''));
        setStatus('Loading registered strategies from app SQLite…', 'loading');
        if (registryState) registryState.textContent = 'Loading registered strategies from app SQLite.';
        try {
            const payload = await requestJson('/api/pine/strategies', { signal: requestController.signal });
            strategies = Array.isArray(payload?.strategies) ? payload.strategies : [];
            select.replaceChildren(new Option(
                strategies.length ? 'Choose a saved Pine strategy' : 'No Pine strategies registered yet',
                '',
            ));
            for (const strategy of strategies) {
                const option = new Option(`${strategy.name} · ${strategy.author || 'source author'}`, String(strategy.id));
                select.add(option);
            }
            select.disabled = strategies.length === 0;
            setStatus(strategies.length
                ? 'Select a chart signal, timeframe, and saved strategy to run against its stored SQLite candles.'
                : 'No Pine strategies are registered. Choose Strategy in the source editor, validate its supported Pine syntax, then register it with provenance.', 'waiting');
            if (registryState) registryState.textContent = strategies.length
                ? `${strategies.length} registered Pine ${strategies.length === 1 ? 'strategy' : 'strategies'} available.`
                : 'No registered Pine strategies yet. Use Add strategy source to open the Pine editor, choose Strategy, validate the source, and register it with provenance.';
        } catch (error) {
            if (error?.name === 'AbortError' && strategyController !== requestController) {
                window.clearTimeout(timeoutId);
                return;
            }
            if (registryTimedOut) error = new Error('The registered strategy request timed out');
            strategies = [];
            select.replaceChildren(new Option('Strategies unavailable · retry', ''));
            setStatus(`Saved strategies unavailable: ${error.message}. Use Reload list after the local service recovers.`, 'error');
            if (registryState) registryState.textContent = `Registered strategy list unavailable: ${error.message}. Use Reload list after the local service recovers.`;
        }
        window.clearTimeout(timeoutId);
        if (strategyController === requestController) strategyController = null;
        if (retryButton) retryButton.disabled = false;
        updateButton();
    }

    async function loadHistoryLimits() {
        if (!historySelect) return;
        historySelect.disabled = true;
        historySelect.replaceChildren(new Option('Loading limits…', ''));
        try {
            const capabilities = await requestPineCapabilities();
            const recent = Number(capabilities?.max_candles);
            const deep = Number(capabilities?.max_strategy_candles);
            if (!Number.isSafeInteger(recent) || recent < 1 || !Number.isSafeInteger(deep) || deep < recent) {
                throw new Error('Service returned invalid Pine history limits.');
            }
            historyLimits = { recent, deep };
            historySelect.replaceChildren(
                new Option(`Recent · ${recent.toLocaleString()} bars`, String(recent)),
                ...(deep > recent ? [new Option(`Deep · up to ${deep.toLocaleString()} bars`, String(deep))] : []),
            );
            historySelect.value = String(deep);
            historySelect.disabled = false;
            updateButton();
        } catch (error) {
            historyLimits = null;
            historySelect.replaceChildren(new Option('History limits unavailable · retry list', ''));
            setStatus(`Backtest history limits unavailable: ${error.message}. Reload the strategy list after the local service recovers.`, 'error');
            updateButton();
        }
    }

    function ensureStrategyListsLoaded(force = false) {
        if (strategyListsPromise) return strategyListsPromise;
        if (strategyListsAttempted && !force) return Promise.resolve();
        strategyListsAttempted = true;
        strategyListsPromise = Promise.all([loadStrategies(), loadHistoryLimits()])
            .finally(() => { strategyListsPromise = null; });
        return strategyListsPromise;
    }

    async function runBacktest() {
        const strategyId = Number(select.value);
        const signal = getDashboardBinding('selectedSignal');
        const timeframe = String(getDashboardBinding('activeTimeframe') || '1d').toLowerCase();
        const maxHistoryBars = Number(historySelect?.value);
        if (!Number.isSafeInteger(strategyId) || !signal || !Number.isSafeInteger(maxHistoryBars) || !historyLimits) {
            resetSummary();
            setStatus('Select a registered chart signal and a saved strategy before running the backtest.', 'waiting');
            return;
        }
        const fromDate = fromDateInput?.value || '';
        const toDate = toDateInput?.value || '';
        let dateRange;
        try {
            dateRange = parseBacktestDateRange(fromDate, toDate);
        } catch (error) {
            resetSummary();
            setStatus(error.message, 'error');
            return;
        }
        let inputOverrides;
        try {
            inputOverrides = strategyInputs.collectOverrides();
        } catch (error) {
            setStatus(error.message, 'error');
            return;
        }
        const request = {
            strategy_id: strategyId,
            exchange_id: signal.exchange_id || '',
            asset_category_id: signal.asset_category_id || '',
            product_id: signal.product_id || '',
            symbol: signal.storage_symbol || signal.symbol || signal.asset || '',
            timeframe,
            max_history_bars: maxHistoryBars,
            walk_forward_training_percent: Number(splitSelect?.value || 70),
            inputs: inputOverrides,
            ...(dateRange || {}),
        };
        if (!request.exchange_id || !request.asset_category_id || !request.product_id || !request.symbol) {
            resetSummary();
            setStatus('The selected chart signal is missing its registered app, product, category, or storage symbol. Re-select it from the signal list to restore that context.', 'error');
            return;
        }

        const runRevision = contextRevision;
        resetSummary();
        const chosen = strategies.find(item => item.id === strategyId);
        const rangeLabel = fromDate ? ` · UTC ${fromDate} through ${toDate}` : '';
        setStatus(`Submitting ${chosen?.name || 'Pine strategy'} to the bounded backtest worker for registered ${request.exchange_id} · ${request.product_id} · ${request.symbol} · ${timeframe.toUpperCase()} history${rangeLabel}…`, 'loading');
        try {
            const pendingJob = jobClient.start(request, event => jobUi.render(event, runRevision, chosen?.name));
            updateButton();
            const job = await pendingJob;
            if (job.state !== 'completed' || job.result_unavailable || !job.result || job.detached || runRevision !== contextRevision) return;
            const payload = job.result;
            const result = payload?.backtest || {};
            const fees = payload.fees || {};
            renderFillAssumptions(result.fill_assumptions, fees);
            renderMagnifierCoverage(result.fill_assumptions);
            renderPineBacktestDiagnostics(runtimeDiagnostics, result.diagnostics);
            const pnlPercentLabel = document.getElementById('pineBacktestPnlPctLabel');
            renderPineBacktestSummaryMetrics(summary, pnlLabel, pnlPercentLabel, result, fees);
            summarizeEquityCurve(equitySummary, Array.isArray(result.equity_curve) ? result.equity_curve : []);
            renderMonthlyPerformance(monthlyPerformance, Array.isArray(result.equity_curve) ? result.equity_curve : [], result.initial_capital);
            renderChronologicalValidation(chronologicalValidation, result.chronological_validation);
            renderDrawdownCurve(drawdownSummary, result.close_drawdown);
            renderTrades(tradeBody, Array.isArray(result.trades) ? result.trades : [], fees.known === true, tradePnlHeading, tradeReturnHeading);
            const tradeRowsLimit = Number(result.trades_limit);
            if (tradeLimit) {
                tradeLimit.textContent = Number.isSafeInteger(tradeRowsLimit) && tradeRowsLimit >= 0
                    ? `Most recent closed trades · capped at ${displayCount(tradeRowsLimit)} rows`
                    : 'Most recent closed trades · current row limit unavailable';
            }
            renderTradeAnalysis(tradeAnalysis, result.trade_analysis);
            renderTradeReturnDistribution(tradeReturnDistribution, result.trade_return_distribution);
            const instrument = payload.instrument || {};
            const history = payload.history || {};
            const source = history.source_label || history.source || 'stored SQLite history';
            const stale = history.freshness === 'STALE' ? ' · STALE history' : '';
            const scope = history.execution_scope === 'research_proxy' ? ' · research proxy; not venue-native evidence' : '';
            const validation = payload.validation?.note || 'No execution validation was performed.';
            const actualStart = displayCandleTimeUtc(history.start_time);
            const actualEnd = displayCandleTimeUtc(history.end_time);
            const capped = Number(history.bars) >= Number(result.history_limit_bars || maxHistoryBars);
            const requestedRange = fromDate ? ` · requested UTC ${fromDate} through ${toDate}` : '';
            const sampleRange = ` · sample ${actualStart}–${actualEnd}${capped ? ' · history limit reached; additional earlier bars may exist' : ''}`;
            const curveSampling = result.equity_curve_is_sampled === true
                ? ` · strategy equity curve sampled to ${displayCount(result.equity_curve_points_returned)} points${result.equity_curve_sampling ? ` (${result.equity_curve_sampling})` : ''}`
                : '';
            const appliedInputs = Object.keys(result.input_overrides || {}).length
                ? ` · input overrides ${JSON.stringify(result.input_overrides)}`
                : ' · declared input defaults';
            setStatus(`${payload.strategy?.name || chosen?.name || 'Pine strategy'} · ${instrument.exchange_id} / ${instrument.product_id} / ${instrument.symbol} · ${instrument.timeframe?.toUpperCase()} · ${history.bars || 0} of up to ${displayCount(result.history_limit_bars || maxHistoryBars)} requested bars${requestedRange}${sampleRange}${curveSampling}${appliedInputs} · ${source}${stale}${scope} · ${fees.return_basis || 'gross; cost coverage unknown'} · research only; chronological tail check is not an untouched holdout guarantee. ${validation} Review execution assumptions and runtime diagnostics below.`, 'ready');
        } catch (error) {
            if (runRevision !== contextRevision) return;
            resetSummary();
            setStatus(`Backtest unavailable: ${error.message}. Retry after selected-app SQLite history or the local service recovers.`, 'error');
        } finally {
            updateButton();
        }
    }

    select.addEventListener('change', () => {
        invalidateResult(select.value
            ? 'Strategy selection changed. Run it again to show results for this source.'
            : 'Select a chart signal, timeframe, and saved strategy to run against stored SQLite candles.');
        void strategyInputs.load(select.value);
    });
    historySelect?.addEventListener('change', () => invalidateResult('History depth changed. Run the strategy again to refresh these results.'));
    fromDateInput?.addEventListener('change', () => invalidateResult('Backtest start date changed. Run the strategy again to refresh these results.'));
    toDateInput?.addEventListener('change', () => invalidateResult('Backtest end date changed. Run the strategy again to refresh these results.'));
    splitSelect?.addEventListener('change', () => invalidateResult('Training share changed. Run the strategy again to refresh these results.'));
    inputControls?.addEventListener('change', () => invalidateResult('Strategy input changed. Run the strategy again to refresh these results.'));
    const backtestPanel = document.getElementById('pineStrategyBacktest');
    if (backtestPanel) {
        select.disabled = true;
        select.replaceChildren(new Option('Focus a control to load strategies', ''));
        if (historySelect) {
            historySelect.disabled = true;
            historySelect.replaceChildren(new Option('Focus a control to load limits', ''));
        }
        setStatus('Focus a backtest control to load registered strategies and history limits.', 'waiting');
        if (registryState) registryState.textContent = 'Registered strategies load when the backtest controls are first used.';
        backtestPanel.addEventListener('focusin', () => { void ensureStrategyListsLoaded(); }, { once: true });
    }
    retryButton?.addEventListener('click', () => {
        invalidateResult('Reloading registered strategies and backtest history limits.');
        void ensureStrategyListsLoaded(true);
    });
    document.getElementById('openPineStrategyEditor')?.addEventListener('click', () => {
        const editorButton = document.getElementById('openPineIndicators');
        if (!editorButton) {
            setStatus('Pine source editor is unavailable in this dashboard build. Reload the local dashboard, then retry.', 'error');
            return;
        }
        editorButton.click();
        const scriptType = document.getElementById('pineScriptType');
        if (scriptType) scriptType.value = 'strategy';
        const registerButton = document.getElementById('registerPineIndicator');
        if (registerButton) registerButton.textContent = 'Register strategy';
        document.getElementById('pineIndicatorSource')?.focus();
    });
    runButton.addEventListener('click', () => { void runBacktest(); });
    cancelButton?.addEventListener('click', () => { void jobClient.cancel(); });
    window.addEventListener('quant:pine-backtest-context-changed', () => {
        invalidateResult(getDashboardBinding('selectedSignal')
            ? 'Chart instrument or timeframe changed. Run the strategy again to refresh these results.'
            : 'Select a chart signal before running a Pine strategy.');
    });
    window.addEventListener('quant:pine-strategy-registered', () => { invalidateResult('Registered strategy source changed. Run the selected strategy again to refresh these results.'); void loadStrategies(); });
}
