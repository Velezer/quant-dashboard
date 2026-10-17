import { publishDashboardBindings } from './runtime-context.js';
import { formatChartCandleLabel } from './chart-candle-time.js';

/** Initializes the foundation dashboard feature. */
export function installDashboardFoundation() {
            let DATA = null;
            let MASTER_DATA = {};
            let MASTER_APPS = [];
            let MASTER_ASSET_CATEGORIES = [];

            function applyMasterData(masterData) {
                MASTER_DATA = masterData;
                MASTER_APPS = Array.isArray(masterData.exchanges) ? masterData.exchanges : [];
                MASTER_ASSET_CATEGORIES = Array.isArray(masterData.asset_categories) ? masterData.asset_categories : [];
                document.querySelectorAll('.exchange-btn[data-exchange]').forEach(button => {
                    const app = MASTER_APPS.find(item => item.id === button.dataset.exchange);
                    if (!app) return;
                    const existing = button.textContent.trim();
                    const icon = existing.match(/^[^\p{L}\p{N}]+/u)?.[0]?.trim() || '';
                    button.textContent = `${icon ? `${icon} ` : ''}${app.label}`;
                    button.title = `${app.label} · ${app.kind || 'execution app'}`;
                });
            }
            let activeExchangeFilter = 'stockbit';
            let activeProductFilter = 'all';
            // Watchlist controls are rebuilt after the first API load. Delegate
            // selection so the visible replacement buttons keep working.
            const activateAppProductFilter = event => {
                const appButton = event.target.closest?.('.exchange-btn[data-exchange]');
                if (appButton) {
                    filterByUniverse(appButton.dataset.exchange);
                    return appButton;
                }
                const productButton = event.target.closest?.('.product-btn[data-product]');
                if (productButton) {
                    activeProductFilter = productButton.dataset.product || 'all';
                    updateProductFilters();
                    renderSignalsList();
                    return productButton;
                }
                return null;
            };
            document.addEventListener('click', activateAppProductFilter);
            let currentModalSymbol = '';
            let activeTimeframe = '';
            let selectedSignal = null;
            let countdownSeconds = 3600;
            // The daemon is the schedule authority. Keeping its epoch lets a full
            // page refresh resume the same countdown instead of starting a new one.
            let nextScanEpoch = null;
            // Storage diagnostics are fetched once at bootstrap and refreshed by
            // scan events. The selected chart's own freshness metadata remains
            // authoritative for the active timeframe; this cache supplies the
            // recent-run context for the disclosure panel.
            let CHART_STORAGE_DIAGNOSTICS = null;
            // Focus mode is the default cognitive load budget: one selected signal,
            // one execution venue, and the price chart stay visually dominant.
            let cognitiveFocusMode = true;

            const LAYOUT_STORAGE_KEY = 'quant.dashboard.layout.v1';
            const WATCHLIST_MIN_WIDTH = 320;
            const WATCHLIST_MAX_WIDTH = 560;
            const INDICATOR_PANE_MIN_HEIGHT = 130;
            const INDICATOR_PANE_MAX_HEIGHT = 360;
            const layoutSettings = {
                watchlistWidth: 400,
                indicatorPaneHeight: 180,
            };
            let layoutResizeState = null;
            let layoutRedrawPending = false;

            function clampLayoutValue(value, min, max, fallback) {
                const number = Number(value);
                return Number.isFinite(number) ? Math.min(max, Math.max(min, number)) : fallback;
            }

            function getMaxWatchlistWidth() {
                return Math.min(WATCHLIST_MAX_WIDTH, Math.max(WATCHLIST_MIN_WIDTH, window.innerWidth * 0.46));
            }

            function getWatchlistWidth() {
                return clampLayoutValue(layoutSettings.watchlistWidth, WATCHLIST_MIN_WIDTH, getMaxWatchlistWidth(), WATCHLIST_MIN_WIDTH);
            }

            function readLayoutSettings() {
                try {
                    const saved = new URLSearchParams(window.localStorage?.getItem(LAYOUT_STORAGE_KEY) || '');
                    layoutSettings.watchlistWidth = clampLayoutValue(saved.get('watchlistWidth'), WATCHLIST_MIN_WIDTH, WATCHLIST_MAX_WIDTH, layoutSettings.watchlistWidth);
                    const indicatorPlot = document.getElementById('dssChartContainer');
                    const defaultIndicatorHeight = indicatorPlot?.getBoundingClientRect().height || layoutSettings.indicatorPaneHeight;
                    const savedIndicatorHeight = saved.get('indicatorPaneHeight');
                    layoutSettings.indicatorPaneHeight = savedIndicatorHeight === null
                        ? defaultIndicatorHeight
                        : clampLayoutValue(savedIndicatorHeight, INDICATOR_PANE_MIN_HEIGHT, INDICATOR_PANE_MAX_HEIGHT, defaultIndicatorHeight);
                } catch (_) {
                    // A private browsing context may reject localStorage. Defaults remain usable.
                }
            }

            function saveLayoutSettings() {
                try {
                    window.localStorage?.setItem(LAYOUT_STORAGE_KEY, new URLSearchParams(layoutSettings).toString());
                } catch (_) {
                    // Layout persistence is optional and never blocks chart use.
                }
            }

            function updateLayoutAria() {
                const workspaceHandle = document.getElementById('workspace-resizer');
                const chartHandle = document.getElementById('chart-pane-resizer');
                if (workspaceHandle) {
                    workspaceHandle.setAttribute('aria-valuemin', String(WATCHLIST_MIN_WIDTH));
                    workspaceHandle.setAttribute('aria-valuemax', String(Math.round(getMaxWatchlistWidth())));
                    workspaceHandle.setAttribute('aria-valuenow', String(Math.round(getWatchlistWidth())));
                }
                if (chartHandle) {
                    const height = Math.round(layoutSettings.indicatorPaneHeight);
                    chartHandle.setAttribute('aria-valuemin', String(INDICATOR_PANE_MIN_HEIGHT));
                    chartHandle.setAttribute('aria-valuemax', String(INDICATOR_PANE_MAX_HEIGHT));
                    chartHandle.setAttribute('aria-valuenow', String(height));
                    chartHandle.setAttribute('aria-valuetext', `Momentum plot height ${height} pixels`);
                }
            }

            function requestLayoutRedraw() {
                if (layoutRedrawPending || typeof requestAnimationFrame !== 'function') return;
                layoutRedrawPending = true;
                requestAnimationFrame(() => {
                    layoutRedrawPending = false;
                    if (selectedSignal && typeof drawCandleChart === 'function') drawCandleChart(selectedSignal);
                });
            }

            function applyLayoutSettings() {
                const workspace = document.getElementById('workspaceGrid');
                const momentumPane = document.getElementById('momentum-pane');
                if (workspace) workspace.style.setProperty('--watchlist-width', `${Math.round(getWatchlistWidth())}px`);
                if (momentumPane) momentumPane.style.setProperty('--dss-plot-height', `${Math.round(layoutSettings.indicatorPaneHeight)}px`);
                updateLayoutAria();
                requestLayoutRedraw();
            }

            function setupLayoutResizers() {
                if (window.__quantLayoutResizersInstalled) {
                    applyLayoutSettings();
                    return;
                }
                window.__quantLayoutResizersInstalled = true;
                readLayoutSettings();
                applyLayoutSettings();
                saveLayoutSettings();

                const handles = [
                    { element: document.getElementById('workspace-resizer'), kind: 'watchlist' },
                    { element: document.getElementById('chart-pane-resizer'), kind: 'indicator' },
                ].filter(item => item.element);

                const startResize = (kind, event) => {
                    if (event.button !== undefined && event.button !== 0) return;
                    const element = handles.find(item => item.kind === kind)?.element;
                    if (!element) return;
                    layoutResizeState = {
                        kind,
                        pointerId: event.pointerId,
                        startX: event.clientX,
                        startY: event.clientY,
                        startValue: kind === 'watchlist' ? getWatchlistWidth() : layoutSettings.indicatorPaneHeight,
                    };
                    document.body.classList.add('is-resizing-layout');
                    element.setPointerCapture?.(event.pointerId);
                    event.preventDefault();
                };

                const moveResize = event => {
                    const state = layoutResizeState;
                    if (!state || (state.pointerId !== undefined && event.pointerId !== state.pointerId)) return;
                    if (state.kind === 'watchlist') {
                        layoutSettings.watchlistWidth = clampLayoutValue(state.startValue - (event.clientX - state.startX), WATCHLIST_MIN_WIDTH, getMaxWatchlistWidth(), WATCHLIST_MIN_WIDTH);
                    } else {
                        layoutSettings.indicatorPaneHeight = clampLayoutValue(
                            state.startValue - (event.clientY - state.startY),
                            INDICATOR_PANE_MIN_HEIGHT,
                            INDICATOR_PANE_MAX_HEIGHT,
                            state.startValue
                        );
                    }
                    applyLayoutSettings();
                };

                const finishResize = event => {
                    if (!layoutResizeState || (layoutResizeState.pointerId !== undefined && event.pointerId !== layoutResizeState.pointerId)) return;
                    const element = handles.find(item => item.kind === layoutResizeState.kind)?.element;
                    element?.releasePointerCapture?.(layoutResizeState.pointerId);
                    layoutResizeState = null;
                    document.body.classList.remove('is-resizing-layout');
                    saveLayoutSettings();
                };

                handles.forEach(({ element, kind }) => {
                    element.addEventListener('pointerdown', event => startResize(kind, event));
                    element.addEventListener('pointermove', moveResize);
                    element.addEventListener('pointerup', finishResize);
                    element.addEventListener('pointercancel', finishResize);
                    element.addEventListener('lostpointercapture', finishResize);
                    element.addEventListener('keydown', event => {
                        const isWatchlist = kind === 'watchlist';
                        const step = event.shiftKey ? 48 : 16;
                        let next = isWatchlist ? getWatchlistWidth() : layoutSettings.indicatorPaneHeight;
                        if (event.key === 'Home') next = isWatchlist ? WATCHLIST_MIN_WIDTH : INDICATOR_PANE_MIN_HEIGHT;
                        else if (event.key === 'End') next = isWatchlist ? getMaxWatchlistWidth() : INDICATOR_PANE_MAX_HEIGHT;
                        else if (isWatchlist && event.key === 'ArrowLeft') next += step;
                        else if (isWatchlist && event.key === 'ArrowRight') next -= step;
                        else if (!isWatchlist && event.key === 'ArrowUp') next += step;
                        else if (!isWatchlist && event.key === 'ArrowDown') next -= step;
                        else return;
                        if (isWatchlist) layoutSettings.watchlistWidth = clampLayoutValue(next, WATCHLIST_MIN_WIDTH, getMaxWatchlistWidth(), WATCHLIST_MIN_WIDTH);
                        else layoutSettings.indicatorPaneHeight = clampLayoutValue(next, INDICATOR_PANE_MIN_HEIGHT, INDICATOR_PANE_MAX_HEIGHT, layoutSettings.indicatorPaneHeight);
                        applyLayoutSettings();
                        saveLayoutSettings();
                        event.preventDefault();
                    });
                });
                window.addEventListener('resize', applyLayoutSettings);
            }

            function setCognitiveFocus(enabled = true) {
                cognitiveFocusMode = !!enabled;
                document.body?.classList.toggle('cognitive-focus', cognitiveFocusMode);
                document.body?.classList.toggle('signals-workspace-active', cognitiveFocusMode);
            }

            function filterHtmlEscape(value) {
                return String(value ?? '').replace(/[&<>"']/g, character => ({
                    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
                }[character]));
            }

            // Dynamic precision formatter for macro, penny, and micro-priced assets.
            function formatPrice(p) {
                if (p === null || p === undefined || isNaN(p)) return '--';
                const num = Number(p);
                if (num === 0) return '0.00';
                const abs = Math.abs(num);
                if (abs >= 100) return num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
                if (abs >= 1) return num.toFixed(3);
                if (abs >= 0.01) return num.toFixed(4);
                if (abs >= 0.0001) return num.toFixed(6);
                if (abs >= 0.000001) return num.toFixed(8);
                return num.toFixed(10);
            }

            function formatChartCandleTime(candle) {
                return formatChartCandleLabel(candle, activeTimeframe);
            }

            // Adjustable Chart Time Window & Pan State
            let activeWindowSize = 45;

    publishDashboardBindings('foundation', {
        DATA: { get: () => DATA, set: value => { DATA = value; } },
        MASTER_DATA: { get: () => MASTER_DATA, set: value => { MASTER_DATA = value; } },
        MASTER_APPS: { get: () => MASTER_APPS, set: value => { MASTER_APPS = value; } },
        MASTER_ASSET_CATEGORIES: { get: () => MASTER_ASSET_CATEGORIES, set: value => { MASTER_ASSET_CATEGORIES = value; } },
        applyMasterData: { get: () => applyMasterData },
        activeExchangeFilter: { get: () => activeExchangeFilter, set: value => { activeExchangeFilter = value; } },
        activeProductFilter: { get: () => activeProductFilter, set: value => { activeProductFilter = value; } },
        activateAppProductFilter: { get: () => activateAppProductFilter },
        currentModalSymbol: { get: () => currentModalSymbol, set: value => { currentModalSymbol = value; } },
        activeTimeframe: { get: () => activeTimeframe, set: value => { activeTimeframe = value; } },
        selectedSignal: { get: () => selectedSignal, set: value => { selectedSignal = value; } },
        countdownSeconds: { get: () => countdownSeconds, set: value => { countdownSeconds = value; } },
        nextScanEpoch: { get: () => nextScanEpoch, set: value => { nextScanEpoch = value; } },
        CHART_STORAGE_DIAGNOSTICS: { get: () => CHART_STORAGE_DIAGNOSTICS, set: value => { CHART_STORAGE_DIAGNOSTICS = value; } },
        cognitiveFocusMode: { get: () => cognitiveFocusMode, set: value => { cognitiveFocusMode = value; } },
        LAYOUT_STORAGE_KEY: { get: () => LAYOUT_STORAGE_KEY },
        layoutSettings: { get: () => layoutSettings },
        layoutResizeState: { get: () => layoutResizeState, set: value => { layoutResizeState = value; } },
        layoutRedrawPending: { get: () => layoutRedrawPending, set: value => { layoutRedrawPending = value; } },
        clampLayoutValue: { get: () => clampLayoutValue },
        readLayoutSettings: { get: () => readLayoutSettings },
        saveLayoutSettings: { get: () => saveLayoutSettings },
        updateLayoutAria: { get: () => updateLayoutAria },
        requestLayoutRedraw: { get: () => requestLayoutRedraw },
        applyLayoutSettings: { get: () => applyLayoutSettings },
        setupLayoutResizers: { get: () => setupLayoutResizers },
        setCognitiveFocus: { get: () => setCognitiveFocus },
        filterHtmlEscape: { get: () => filterHtmlEscape },
        formatPrice: { get: () => formatPrice },
        formatChartCandleTime: { get: () => formatChartCandleTime },
        activeWindowSize: { get: () => activeWindowSize, set: value => { activeWindowSize = value; } },
    });
}
