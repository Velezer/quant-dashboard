import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { createSignalScoreBasisMarkup } from './signal-score-basis.js';
import { createSignalScreenerChartContext } from './signal-screener-chart-context.js';
import { createSignalScreenerSummary } from './signal-screener-summary.js';
import { createSignalScreenerCard } from './signal-screener-card.js';
import { appSignalReadiness } from './signal-readiness-model.js';

import {
    APP_LABELS,
    PRODUCTS_BY_APP,
    SIGNAL_LIST_PAGE_SIZE,
    ageLabelFromSeconds,
    averageNormalizedScore,
    cryptoSignalModelsForSelection,
    stockbitSignalModelsForSelection,
    signalIdentity,
    normalizeSignalRecord,
    signalDisplayScore,
    signalMeetsRenderMinimum,
    signalPriceBarAgeLabel,
    signalPriceCoverageMessage,
    signalPriceIsFresh,
    signalRowsForDisplay,
    signalScoreState,
    signalScoreComponentWeights,
    signalScorePresentation,
    signalScoreWeightSummary,
    signalStrategyLabel,
    scoreWeightLabel,
} from './signal-screener-model.js';

const MAX_BROWSER_TIMER_DELAY_MS = 2_147_483_647;

export {
    SIGNAL_LIST_PAGE_SIZE,
    cryptoSignalModelsForSelection,
    stockbitSignalModelsForSelection,
    signalIdentity,
    signalDisplayScore,
    signalMeetsRenderMinimum,
    signalPriceBarAgeLabel,
    signalPriceCoverageMessage,
    signalRowsForDisplay,
    signalScoreComponentWeights,
    signalScorePresentation,
    signalScoreSummary,
    signalScoreWeightSummary,
    signalStrategyLabel,
} from './signal-screener-model.js';
/** Owns current entry-signal filters, rows, and the selected-signal chart context. */
export function installSignalScreener() {
    let rows = [];
    let signalScoreModels = [];
    let signalStrategies = [];
    let summary = { stale: true, status: { last_attempt_status: 'waiting', signal_count: 0 } };
    let requestState = 'loading';
    let requestError = '';
    let timeframeFilter = 'all';
    let availableTimeframes = [];
    let searchQuery = '';
    let visibleSignalLimit = SIGNAL_LIST_PAGE_SIZE;
    let nextSignalOffset = 0;
    let hasMoreSignalRows = false;
    let pendingAlertNavigation = null;
    let searchRefreshTimer = null;
    let priceExpiryTimer = null;
    const renderScoreBasis = createSignalScoreBasisMarkup({ escape, scoreModelFor });
    const renderSignalCard = createSignalScreenerCard({
        escape,
        appLabel,
        scoreModelFor,
        signalScoreState,
        signalScorePresentation,
        signalDisplayScore,
        signalScoreWeightSummary,
        signalPriceBarAgeLabel,
        formatPrice,
        ageLabel,
        renderScoreBasis,
    });
    const { updateStatusText, renderKpis } = createSignalScreenerSummary({
        getSummary: () => summary,
        getRequestState: () => requestState,
        getRequestError: () => requestError,
        getRows: () => rows,
        getVisibleSignals: filteredSignals,
        stockbitSignalModelsForView,
        signalPriceCoverageMessage,
        ageLabel,
        formatTime,
        signalScoreState,
        scoreModelFor,
        averageNormalizedScore,
        escape,
    });
    const {
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
    } = createSignalScreenerChartContext({
        getAvailableTimeframes: () => availableTimeframes,
        getRows: () => rows,
        scoreModelFor,
        formatPrice,
        renderSignalsList,
    });

    function refreshSignalRows({ append = false } = {}) {
        void getDashboardBinding('fetchFreshSignals')?.({
            quiet: append,
            append,
            offset: append ? nextSignalOffset : 0,
        });
    }

    function schedulePriceExpiry() {
        if (priceExpiryTimer !== null) clearTimeout(priceExpiryTimer);
        priceExpiryTimer = null;
        const earliestExpiry = rows
            .filter(signal => signal.persistent_score !== true)
            .map(signal => Number(signal.price_fresh_until_ms))
            .filter(expiry => Number.isFinite(expiry) && expiry > 0)
            .reduce((earliest, expiry) => Math.min(earliest, expiry), Number.POSITIVE_INFINITY);
        if (!Number.isFinite(earliestExpiry)) return;
        const delay = Math.max(0, Math.min(
            earliestExpiry - Date.now() + 1,
            MAX_BROWSER_TIMER_DELAY_MS,
        ));
        priceExpiryTimer = setTimeout(expireStalePrices, delay);
    }

    function expireStalePrices() {
        const nowEpochMs = Date.now();
        const expiredIds = new Set(rows
            .filter(signal => !signalPriceIsFresh(signal, nowEpochMs))
            .map(signal => signal.signal_id));
        if (expiredIds.size === 0) {
            schedulePriceExpiry();
            return;
        }
        rows = rows.filter(signal => !expiredIds.has(signal.signal_id));
        if (Array.isArray(globalThis.DATA?.signals)) {
            globalThis.DATA.signals = globalThis.DATA.signals.filter(signal =>
                !expiredIds.has(signalIdentity(signal)));
        }
        summary.stale_price_signal_count =
            (Number(summary.stale_price_signal_count) || 0) + expiredIds.size;
        for (const countKey of ['matching_rows', 'total_signals']) {
            const count = Number(summary[countKey]);
            if (Number.isFinite(count)) summary[countKey] = Math.max(0, count - expiredIds.size);
        }
        if (globalThis.DATA?.summary) {
            globalThis.DATA.summary.stale_price_signal_count = summary.stale_price_signal_count;
        }
        if (globalThis.selectedSignal
            && expiredIds.has(globalThis.selectedSignal.signal_id)) {
            clearSelectedSignalChart('The selected signal price expired. Its entry row was removed.');
        }
        renderSignalsList();
        schedulePriceExpiry();
    }

    function alertSignalIdentity(signal) {
        return signalIdentity({
            exchange_id: String(signal.exchange_id || '').toLowerCase(),
            product_id: String(signal.product_id || '').toLowerCase(),
            asset_category_id: String(signal.asset_category_id || '').toLowerCase(),
            symbol: String(signal.symbol || '').trim().toUpperCase(),
            signal_strategy_id: String(signal.signal_strategy_id || ''),
            timeframe: String(signal.timeframe || '').toLowerCase(),
        });
    }

    function finishAlertNavigation(result, signal = null) {
        const pending = pendingAlertNavigation;
        if (!pending) return;
        pendingAlertNavigation = null;
        if (signal) focusSignal(signal);
        pending.resolve(result);
    }

    function focusSignal(signal) {
        const visibleIndex = filteredSignals().findIndex(row => row.signal_id === signal.signal_id);
        if (visibleIndex >= 0) visibleSignalLimit = Math.max(visibleSignalLimit, visibleIndex + 1);
        selectSignal(signal);
        window.requestAnimationFrame(() => {
            document.getElementById('workspaceGrid')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            const row = [...(document.querySelectorAll('#signalsList [data-signal-id]') || [])]
                .find(button => button.dataset.signalId === signal.signal_id);
            row?.focus({ preventScroll: true });
            row?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        });
    }

    function openSignalFromAlert(alert) {
        const identity = alertSignalIdentity(alert || {});
        const symbol = String(alert?.symbol || '').trim();
        const exchange = String(alert?.exchange_id || '').toLowerCase();
        const product = String(alert?.product_id || '').toLowerCase();
        const timeframe = String(alert?.timeframe || '').toLowerCase();
        const strategy = String(alert?.signal_strategy_id || '');
        if (!symbol || !exchange || !product || !timeframe || !strategy) return Promise.resolve('not_current');

        globalThis.activeExchangeFilter = exchange;
        globalThis.activeProductFilter = product;
        timeframeFilter = availableTimeframes.includes(timeframe) ? timeframe : 'all';
        updateAppButtons();
        updateProductFilters();
        updateSignalTimeframeButtons();
        const search = document.getElementById('searchInput');
        if (search) search.value = symbol;
        setSearchQuery(symbol, { refresh: false });

        const currentSignal = filteredSignals().find(row => alertSignalIdentity(row) === identity);
        if (currentSignal) {
            focusSignal(currentSignal);
            return Promise.resolve('opened');
        }
        if (typeof getDashboardBinding('fetchFreshSignals') !== 'function') {
            return Promise.resolve('unavailable');
        }

        if (pendingAlertNavigation) finishAlertNavigation('replaced');
        return new Promise(resolve => {
            pendingAlertNavigation = { identity, resolve };
            refreshSignalRows();
        });
    }

    function setSearchQuery(value, { refresh = true } = {}) {
        searchQuery = String(value || '');
        visibleSignalLimit = SIGNAL_LIST_PAGE_SIZE;
        renderSignalsList();
        clearTimeout(searchRefreshTimer);
        if (refresh) searchRefreshTimer = setTimeout(() => refreshSignalRows(), 250);
    }

    function escape(value) {
        return getDashboardBinding('filterHtmlEscape')?.(value) ?? String(value ?? '');
    }

    function appLabel(exchange) {
        return APP_LABELS[String(exchange || '').toLowerCase()] || String(exchange || 'App').toUpperCase();
    }

    function scoreModelFor(signal) {
        return signalScoreModels.find(item =>
            String(item.id || '') === String(signal.signal_strategy_id || '')
        );
    }

    function stockbitSignalModelsForView() {
        return stockbitSignalModelsForSelection(
            signalScoreModels,
            getDashboardBinding('activeExchangeFilter'),
            getDashboardBinding('activeProductFilter'),
            timeframeFilter,
        );
    }

    function stockbitSignalEmptyText(models) {
        if (!models.length) {
            return `No Stockbit NWE/DSS model is configured for ${timeframeFilter}.`;
        }
        if (summary.stale) {
            return timeframeFilter === 'all'
                ? 'The snapshot is stale. Retry or wait for a fresh scan.'
                : `${timeframeFilter} scores are stale. Retry or wait for a fresh scan.`;
        }
        const timeframeLabel = timeframeFilter === 'all' ? 'Fresh bars' : `Fresh ${timeframeFilter} bars`;
        return `No market-entry signal now. The latest ${timeframeLabel.toLowerCase()} must qualify at the model’s minimum, with four /10 indicators and a fresh same-source 15m price.`;
    }

    function cryptoSignalModelsForView() {
        return cryptoSignalModelsForSelection(
            signalScoreModels,
            getDashboardBinding('activeExchangeFilter'),
            getDashboardBinding('activeProductFilter'),
            timeframeFilter,
        );
    }

    function cryptoSignalEmptyText() {
        if (summary.stale) return 'The snapshot is stale. Retry or wait for a fresh scan.';
        return 'No current signal rows were returned. Detailed scan checks are not available yet. Recheck status or run a signal scan to identify missing inputs or model conditions.';
    }

    function filteredSignals() {
        const exchange = String(getDashboardBinding('activeExchangeFilter') || 'stockbit').toLowerCase();
        const product = String(getDashboardBinding('activeProductFilter') || 'all').toLowerCase();
        const query = searchQuery.trim().toLowerCase();
        return rows.filter(signal => {
            if (!signalMeetsRenderMinimum(signal, scoreModelFor(signal))) return false;
            if (signal.exchange_id !== exchange) return false;
            if (product !== 'all' && signal.product_id !== product) return false;
            if (timeframeFilter !== 'all' && signal.timeframe !== timeframeFilter) return false;
            if (!query) return true;
            return signal.symbol.toLowerCase().includes(query)
                || signal.signal_strategy_name.toLowerCase().includes(query)
                || signal.signal_strategy_id.toLowerCase().includes(query);
        }).sort((left, right) => {
            const leftIsCurrent = signalScoreState(left)?.current !== false;
            const rightIsCurrent = signalScoreState(right)?.current !== false;
            return Number(rightIsCurrent) - Number(leftIsCurrent)
                || (signalDisplayScore(right) ?? -1) - (signalDisplayScore(left) ?? -1)
                || right.last_updated_at - left.last_updated_at
                || left.symbol.localeCompare(right.symbol);
        });
    }

    function formatPrice(value) {
        const formatter = getDashboardBinding('formatPrice');
        return formatter ? formatter(value) : Number(value).toLocaleString('en-US', { maximumFractionDigits: 8 });
    }

    function formatTime(epochSeconds) {
        const timestamp = Number(epochSeconds);
        if (!Number.isFinite(timestamp) || timestamp <= 0) return 'time unavailable';
        return new Intl.DateTimeFormat('en-GB', {
            timeZone: 'Asia/Jakarta',
            day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit',
        }).format(new Date(timestamp * 1000));
    }

    function ageLabel(epochSeconds) {
        const timestamp = Number(epochSeconds);
        if (!Number.isFinite(timestamp) || timestamp <= 0) return 'age unavailable';
        const seconds = Math.max(0, Math.floor(Date.now() / 1000 - timestamp));
        return ageLabelFromSeconds(seconds);
    }

    function updateProductFilters() {
        const app = String(getDashboardBinding('activeExchangeFilter') || 'stockbit').toLowerCase();
        const products = new Set(PRODUCTS_BY_APP[app] || []);
        document.querySelectorAll('.product-btn[data-product]').forEach(button => {
            const product = button.dataset.product;
            button.hidden = product !== 'all' && !products.has(product);
            const selected = product === String(getDashboardBinding('activeProductFilter') || 'all');
            button.setAttribute('aria-pressed', String(selected));
        });
    }

    function updateAppButtons() {
        const app = String(getDashboardBinding('activeExchangeFilter') || 'stockbit').toLowerCase();
        document.querySelectorAll('.exchange-btn[data-exchange]').forEach(button => {
            const selected = button.dataset.exchange === app;
            button.setAttribute('aria-pressed', String(selected));
            button.classList.toggle('active', selected);
            button.classList.toggle('bg-gray-800', selected);
            button.classList.toggle('bg-gray-900', !selected);
            button.classList.toggle('text-white', selected);
            button.classList.toggle('text-gray-400', !selected);
        });
    }

    function updateSignalTimeframeButtons() {
        document.querySelectorAll('.screener-timeframe-btn').forEach(button => {
            const selected = button.dataset.screenerTimeframe === timeframeFilter;
            button.setAttribute('aria-pressed', String(selected));
            button.classList.toggle('active', selected);
        });
    }

    function renderTimeframeControls() {
        const chartControls = document.getElementById('chartTimeframeButtons');
        if (chartControls) {
            chartControls.replaceChildren(...availableTimeframes.map(timeframe => {
                const button = document.createElement('button');
                button.type = 'button';
                button.className = 'chart-timeframe-btn px-2 py-0.5 text-xs font-semibold rounded text-gray-300 hover:text-white hover:bg-gray-700';
                button.dataset.chartTimeframe = timeframe;
                button.setAttribute('aria-label', `Show ${timeframe} candles`);
                button.setAttribute('aria-pressed', 'false');
                button.title = `${timeframe} candles from stored local history when available.`;
                button.textContent = timeframe.toUpperCase();
                return button;
            }));
        }
        const screenerControls = document.getElementById('screenerTimeframeButtons');
        if (screenerControls) {
            const all = document.createElement('button');
            all.type = 'button';
            all.className = 'screener-timeframe-btn';
            all.dataset.screenerTimeframe = 'all';
            all.textContent = 'All';
            screenerControls.replaceChildren(screenerControls.firstElementChild, all,
                ...availableTimeframes.map(timeframe => {
                    const button = document.createElement('button');
                    button.type = 'button';
                    button.className = 'screener-timeframe-btn';
                    button.dataset.screenerTimeframe = timeframe;
                    button.textContent = timeframe;
                    return button;
                }));
        }
        initChartTimeframeControls();
        updateSignalTimeframeButtons();
    }

    function renderSignalsList() {
        const list = document.getElementById('signalsList');
        if (!list) return;
        updateAppButtons();
        updateProductFilters();
        updateSignalTimeframeButtons();
        const signals = filteredSignals();
        const selectedApp = getDashboardBinding('activeExchangeFilter') || 'stockbit';
        const readinessScope = { product: getDashboardBinding('activeProductFilter') || 'all',
            timeframe: timeframeFilter, search: searchQuery, visibleCount: signals.length };
        const appReadiness = appSignalReadiness(selectedApp, null, summary, readinessScope);
        getDashboardBinding('showAppSignalReadiness')?.(selectedApp, summary, readinessScope);
        const selectedId = globalThis.selectedSignal?.signal_id || '';
        const stalePriceSignalCount = Number(summary.stale_price_signal_count || 0);
        const priceDataStatus = String(summary.status?.price_data_status || 'ready');
        const priceDataUnavailableCount = Number(summary.status?.price_data_unavailable_count || 0);
        const stockbitSignalModels = stockbitSignalModelsForView();
        const stockbitSignalSelected = stockbitSignalModels.length > 0;
        const stockbitSignalView = stockbitSignalSelected
            && !searchQuery.trim();
        const cryptoSignalModels = cryptoSignalModelsForView();
        const cryptoSignalView = cryptoSignalModels.length > 0
            && !searchQuery.trim();
        const countBadge = document.getElementById('signal-count-badge');
        const renderedSignals = signalRowsForDisplay(signals, visibleSignalLimit);
        if (countBadge) {
            const storedMatches = Number(summary.matching_rows);
            const countText = Number.isFinite(storedMatches)
                ? `${signals.length} loaded · ${storedMatches} stored matches`
                : `${signals.length} ${summary.stale ? 'saved ' : ''}signal${signals.length === 1 ? '' : 's'}`;
            const visibleText = renderedSignals.length < signals.length
                ? `${renderedSignals.length} shown`
                : '';
            const snapshotText = summary.stale ? 'stale snapshot' : '';
            countBadge.textContent = [countText, visibleText, snapshotText]
                .filter(Boolean)
                .join(' · ');
        }
        updateStatusText(signals.length);
        renderKpis(signals);
        const clearButton = document.getElementById('clearSearchBtn');
        if (clearButton) clearButton.disabled = !searchQuery;

        if (signals.length === 0) {
            const emptyText = requestState === 'error' && rows.length === 0
                ? 'Signal data could not be loaded. Retry after checking the service connection.'
                : appReadiness.hasScanDetails
                    ? 'The app data checks and recovery actions are shown in Signal availability above.'
                : summary.status?.last_attempt_status === 'waiting'
                    ? 'The first scan has not completed yet. Signals will appear after the scheduled scan or a manual refresh.'
                : summary.stale && rows.length === 0
                        ? 'The saved snapshot is stale and has no current signals. Retry the scan or wait for the running scan to complete.'
                    : stockbitSignalView
                        ? stockbitSignalEmptyText(stockbitSignalModels)
                    : cryptoSignalView
                        ? cryptoSignalEmptyText(cryptoSignalModels)
                    : rows.length > 0
                        ? 'No signals match the selected app, product, timeframe, and search. Clear a filter to see more.'
                        : stalePriceSignalCount > 0 && !stockbitSignalSelected
                            ? 'Saved signals with stale price bars are hidden. They can return when a fresh provider bar passes the price check.'
                        : 'No current signal rows were returned. Detailed scan checks are not available yet; see Signal availability for the known listing and data status.';
            const priceCoverageMessage = appReadiness.hasScanDetails ? ''
                : signalPriceCoverageMessage(priceDataStatus, priceDataUnavailableCount);
            const emptyStateText = [emptyText, priceCoverageMessage]
                .filter(Boolean)
                .join(' ');
            const emptyHeading = requestState === 'error'
                ? 'Signal data unavailable'
                : appReadiness.hasScanDetails
                    ? `${appLabel(selectedApp)}: ${appReadiness.title}`
                : searchQuery.trim()
                    ? 'No matching signals'
                : cryptoSignalView
                    ? `No current ${appLabel(getDashboardBinding('activeExchangeFilter'))} crypto signals`
                    : stockbitSignalSelected
                        ? 'No current Stockbit signal'
                    : 'No matching signals';
            list.innerHTML = `<div class="signal-empty-state" role="status"><strong>${escape(emptyHeading)}</strong><p>${escape(emptyStateText)}</p>${rows.length > 0 && (searchQuery || timeframeFilter !== 'all' || getDashboardBinding('activeProductFilter') !== 'all') ? '<button type="button" data-clear-signal-filters>Clear filters</button>' : ''}${hasMoreSignalRows ? '<button type="button" data-load-more-signals>Continue loading matching signals</button>' : ''}</div>`;
            const chartText = requestState === 'error' ? 'Signal data is unavailable. Retry to restore the screener.' : 'No matching signal is selected.';
            if (selectedId && !signals.some(signal => signal.signal_id === selectedId)) clearSelectedSignalChart(chartText);
            return;
        }

        const selectedVisible = renderedSignals.find(signal => signal.signal_id === selectedId);
        if (selectedId && !selectedVisible) clearSelectedSignalChart('The selected signal is outside the current screener filters. Select a visible row to inspect its chart.');
        const cards = renderedSignals
            .map((signal, index) => renderSignalCard(signal, index + 1, signal.signal_id === selectedId))
            .join('');
        const loadMore = hasMoreSignalRows
            ? '<button type="button" class="w-full rounded-lg border border-gray-700 bg-gray-950 px-3 py-2 text-xs font-semibold text-gray-300 hover:bg-gray-800" data-load-more-signals>Load more matching signals</button>'
            : '';
        list.innerHTML = `${cards}${loadMore}`;
    }

    function filterByUniverse(exchange) {
        visibleSignalLimit = SIGNAL_LIST_PAGE_SIZE;
        globalThis.activeExchangeFilter = String(exchange || 'stockbit').toLowerCase();
        globalThis.activeProductFilter = 'all';
        updateAppButtons();
        updateProductFilters();
        renderSignalsList();
        refreshSignalRows();
    }

    function applySnapshot(payload, { append = false } = {}) {
        signalStrategies = Array.isArray(payload?.master_data?.signal_strategies)
            ? payload.master_data.signal_strategies
            : [];
        signalScoreModels = Array.isArray(payload?.master_data?.signal_score_models)
            ? payload.master_data.signal_score_models
            : [];
        const timeframes = Array.isArray(payload?.available_timeframes)
            ? payload.available_timeframes.map(value => String(value || '').toLowerCase()).filter(value => /^\d+(m|h|d|w)$/.test(value))
            : [];
        availableTimeframes = [...new Set(timeframes)];
        const activeTimeframe = String(getDashboardBinding('activeTimeframe') || '').toLowerCase();
        if (!availableTimeframes.includes(activeTimeframe)) globalThis.activeTimeframe = availableTimeframes[0] || '';
        if (!availableTimeframes.includes(timeframeFilter)) timeframeFilter = 'all';
        renderTimeframeControls();
        const previousSelection = globalThis.selectedSignal;
        const nextRows = (Array.isArray(payload?.signals) ? payload.signals : [])
            .map(raw => normalizeSignalRecord(raw, signalStrategies));
        if (previousSelection) {
            const replacement = nextRows.find(row => row.signal_id === previousSelection.signal_id);
            if (replacement) {
                replacement.candles = previousSelection.candles;
                replacement.candles_by_tf = previousSelection.candles_by_tf;
                replacement.chart_freshness = previousSelection.chart_freshness;
                replacement.chart_data_loaded = previousSelection.chart_data_loaded;
                replacement.chart_indicator_config = previousSelection.chart_indicator_config;
                replacement.markov_regimes_by_tf = previousSelection.markov_regimes_by_tf;
                replacement._rustIndicatorCache = previousSelection._rustIndicatorCache;
                replacement._chartBundlePromise = previousSelection._chartBundlePromise;
                replacement._chartBundleRequestToken = previousSelection._chartBundleRequestToken;
                replacement._chartBundleAbortController = previousSelection._chartBundleAbortController;
                replacement._rustIndicatorAbortController = previousSelection._rustIndicatorAbortController;
                globalThis.selectedSignal = replacement;
            }
        }
        if (append) {
            const merged = new Map(rows
                .filter(row => signalPriceIsFresh(row))
                .map(row => [row.signal_id, row]));
            for (const row of nextRows) {
                if (signalPriceIsFresh(row)) merged.set(row.signal_id, row);
            }
            rows = [...merged.values()];
        } else {
            rows = nextRows.filter(row => signalPriceIsFresh(row));
        }
        summary = payload?.summary && typeof payload.summary === 'object' ? payload.summary : { stale: true, status: {} };
        nextSignalOffset = Number(summary.next_offset) || 0;
        hasMoreSignalRows = summary.has_more === true;
        requestState = 'ready';
        requestError = '';
        schedulePriceExpiry();
        renderSignalsList();
        if (globalThis.selectedSignal) renderSelectedSignalSummary(globalThis.selectedSignal);
        if (pendingAlertNavigation) {
            const signal = filteredSignals().find(row => alertSignalIdentity(row) === pendingAlertNavigation.identity);
            if (signal) finishAlertNavigation('opened', signal);
            else if (hasMoreSignalRows) refreshSignalRows({ append: true });
            else finishAlertNavigation('not_current');
        }
    }

    function setLoading() {
        requestState = 'loading';
        requestError = '';
        renderSignalsList();
    }

    function setFetchError(error) {
        requestState = 'error';
        requestError = String(error || 'Signal request failed').slice(0, 220);
        renderSignalsList();
        finishAlertNavigation('unavailable');
    }

    document.getElementById('signalsList')?.addEventListener('click', event => {
        const loadMore = event.target.closest('[data-load-more-signals]');
        if (loadMore) {
            visibleSignalLimit += SIGNAL_LIST_PAGE_SIZE;
            refreshSignalRows({ append: true });
            return;
        }
        const clear = event.target.closest('[data-clear-signal-filters]');
        if (clear) {
            searchQuery = '';
            timeframeFilter = 'all';
            globalThis.activeProductFilter = 'all';
            const search = document.getElementById('searchInput');
            if (search) search.value = '';
            filterByUniverse(getDashboardBinding('activeExchangeFilter') || 'stockbit');
            return;
        }
        const button = event.target.closest('[data-signal-id]');
        if (button) selectSignal(button.dataset.signalId);
    });
    document.getElementById('screenerTimeframeButtons')?.addEventListener('click', event => {
        const button = event.target.closest('.screener-timeframe-btn');
        if (!button) return;
        timeframeFilter = button.dataset.screenerTimeframe || 'all';
        visibleSignalLimit = SIGNAL_LIST_PAGE_SIZE;
        renderSignalsList();
        refreshSignalRows();
    });
    document.getElementById('searchInput')?.addEventListener('input', event => {
        setSearchQuery(event.currentTarget.value);
    });
    document.getElementById('clearSearchBtn')?.addEventListener('click', () => {
        searchQuery = '';
        const search = document.getElementById('searchInput');
        if (search) search.value = '';
        renderSignalsList();
        refreshSignalRows();
        search?.focus();
    });
    document.addEventListener?.('click', event => {
        if (!event.target.closest?.('.product-btn[data-product]')) return;
        visibleSignalLimit = SIGNAL_LIST_PAGE_SIZE;
        refreshSignalRows();
    });
    document.getElementById('screener-retry')?.addEventListener('click', () => {
        const needsScan = summary.stale
            || ['waiting', 'error'].includes(summary.status?.last_attempt_status)
            || ['unavailable', 'partial_stale'].includes(summary.status?.price_data_status)
            || rows.some(signal => signalScoreState(signal)?.tone === 'stale');
        if (needsScan) void getDashboardBinding('triggerManualRefresh')?.();
        else void getDashboardBinding('fetchFreshSignals')?.();
    });

    publishDashboardBindings('signal-screener', {
        applySignalSnapshot: { get: () => applySnapshot },
        getSignalScreenerQuery: { get: () => () => ({
            exchange: getDashboardBinding('activeExchangeFilter') || 'stockbit',
            product: getDashboardBinding('activeProductFilter') || 'all',
            timeframe: timeframeFilter,
            search: searchQuery,
            limit: SIGNAL_LIST_PAGE_SIZE,
        }) },
        setScreenerLoading: { get: () => setLoading },
        setScreenerFetchError: { get: () => setFetchError },
        renderSignalsList: { get: () => renderSignalsList },
        renderKPIs: { get: () => renderKpis },
        getFilteredSignals: { get: () => filteredSignals },
        selectSignal: { get: () => selectSignal },
        openSignalFromAlert: { get: () => openSignalFromAlert },
        prepareChartForSignal: { get: () => prepareChartForSignal },
        clearSelectedSignalChart: { get: () => clearSelectedSignalChart },
        updateChartTimeframeContext: { get: () => updateChartTimeframeContext },
        updateChartTimeframeButtons: { get: () => updateChartTimeframeButtons },
        setChartTimeframe: { get: () => setChartTimeframe },
        initChartTimeframeControls: { get: () => initChartTimeframeControls },
        renderSelectedSignalSummary: { get: () => renderSelectedSignalSummary },
        updateProductFilters: { get: () => updateProductFilters },
        filterByUniverse: { get: () => filterByUniverse },
        executionVenueMeta: { get: () => executionVenueMeta },
        updateAppContextChip: { get: () => updateAppContextChip },
    });
    updateProductFilters();
    renderSignalsList();
}
