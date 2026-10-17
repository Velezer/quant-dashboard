import { publishDashboardBindings } from './runtime-context.js';
import { QuantChart } from './chart-math.js';

/** Initializes the chart inspection dashboard feature. */
export function installChartInspection() {
            let chartCrosshair = {
                active: false,
                x: 0,
                y: null,
                dssY: null,
                barIdx: -1,
                price: null,
                dssVal: null,
                candle: null
            };

            // TradingView Dynamic Top-Left OHLC Inspector
            function updateOHLCBar(c, signal) {
                // An older chart request or pointer event must never overwrite the
                // inspector after the user has switched symbols or venues.
                if (signal && selectedSignal && !isSelectedSignal(signal)) return;
                const symEl = document.getElementById('tv-ohlc-symbol');
                const tfEl = document.getElementById('tv-ohlc-tf');
                const exchangeEl = document.getElementById('tv-ohlc-exchange');
                const oEl = document.getElementById('tv-ohlc-o');
                const hEl = document.getElementById('tv-ohlc-h');
                const lEl = document.getElementById('tv-ohlc-l');
                const cEl = document.getElementById('tv-ohlc-c');
                const chgEl = document.getElementById('tv-ohlc-chg');
                const dateEl = document.getElementById('tv-crosshair-date');
                if (!c || !oEl) return;

                if (symEl && signal) symEl.innerText = signal.asset || '--';
                if (tfEl) tfEl.innerText = activeTimeframe.toUpperCase();
                if (exchangeEl) {
                    const venue = executionVenueMeta(signal);
                    exchangeEl.innerText = venue.key === 'stockbit' ? 'STOCKBIT · IDR'
                        : venue.key === 'tokocrypto' ? 'TOKOCRYPTO · USD'
                        : ['pintu', 'pluang'].includes(venue.key)
                            ? `${venue.label} · ${String(signal.product_id || '').replaceAll('_', ' ').toUpperCase()} · USD`
                            : 'APP · --';
                }

                oEl.innerText = formatPrice(c.open);
                hEl.innerText = formatPrice(c.high);
                lEl.innerText = formatPrice(c.low);
                cEl.innerText = formatPrice(c.close);

                const diff = (c.close !== undefined && c.open !== undefined) ? (c.close - c.open) : 0;
                const diffPct = (c.open && c.open > 0) ? (diff / c.open * 100) : 0;
                const isUp = diff >= 0;
                const colorClass = isUp ? 'text-[#089981]' : 'text-[#f23645]';

                if (cEl) {
                    cEl.className = `${colorClass} font-bold`;
                }
                if (chgEl) {
                    chgEl.className = `${colorClass} font-bold`;
                    chgEl.innerText = `${isUp ? '+' : ''}${formatPrice(diff)} (${isUp ? '+' : ''}${diffPct.toFixed(2)}%)`;
                }
                if (dateEl) {
                    dateEl.innerText = formatChartCandleTime(c);
                }
            }

            function resetOHLCBarForSignal(signal) {
                const symEl = document.getElementById('tv-ohlc-symbol');
                const tfEl = document.getElementById('tv-ohlc-tf');
                const exchangeEl = document.getElementById('tv-ohlc-exchange');
                if (!signal) {
                    if (symEl) symEl.innerText = '--';
                    if (tfEl) tfEl.innerText = '--';
                    if (exchangeEl) exchangeEl.innerText = 'APP · --';
                    ['tv-ohlc-o', 'tv-ohlc-h', 'tv-ohlc-l', 'tv-ohlc-c'].forEach(id => {
                        const el = document.getElementById(id);
                        if (el) {
                            el.innerText = '--';
                            if (id === 'tv-ohlc-c') el.className = 'text-gray-400 font-bold';
                        }
                    });
                    const change = document.getElementById('tv-ohlc-chg');
                    if (change) { change.className = 'text-gray-400 font-bold'; change.innerText = '--'; }
                    const date = document.getElementById('tv-crosshair-date');
                    if (date) date.innerText = 'Latest Bar';
                    return;
                }
                const venue = executionVenueMeta(signal);
                if (symEl) symEl.innerText = signal.asset || '--';
                if (tfEl) tfEl.innerText = activeTimeframe.toUpperCase();
                if (exchangeEl) exchangeEl.innerText = venue.key === 'stockbit' ? 'STOCKBIT · IDR'
                    : venue.key === 'tokocrypto' ? 'TOKOCRYPTO · USD'
                    : ['pintu', 'pluang'].includes(venue.key)
                        ? `${venue.label} · ${String(signal?.product_id || '').replaceAll('_', ' ').toUpperCase()} · USD`
                        : 'APP · --';
                ['tv-ohlc-o', 'tv-ohlc-h', 'tv-ohlc-l', 'tv-ohlc-c'].forEach(id => {
                    const el = document.getElementById(id);
                    if (el) el.innerText = '--';
                });
                const change = document.getElementById('tv-ohlc-chg');
                if (change) { change.className = 'text-gray-400 font-bold'; change.innerText = 'Loading…'; }
                const date = document.getElementById('tv-crosshair-date');
                if (date) date.innerText = 'Loading latest bar…';
            }

            let chartViewport = null;
            let chartFrameRequest = null;
            let chartPendingSignal = null;

            function isSelectedSignal(signal) {
                return !!signal && !!selectedSignal && QuantChart.key(signal) === QuantChart.key(selectedSignal);
            }

            function showChartRenderError(error) {
                const canvas = document.getElementById('candleCanvas');
                if (!canvas) return;
                if (typeof setChartStatus === 'function') setChartStatus('error', 'The chart renderer failed safely. Retry the chart data after the current selection is stable.', { signal: selectedSignal });
                const ctx = canvas.getContext && canvas.getContext('2d');
                if (!ctx) return;
                const rect = canvas.getBoundingClientRect();
                if (!rect.width || !rect.height) return;
                const ratio = window.devicePixelRatio || 1;
                const w = Math.max(1, Math.round(rect.width * ratio));
                const h = Math.max(1, Math.round(rect.height * ratio));
                if (canvas.width !== w) canvas.width = w;
                if (canvas.height !== h) canvas.height = h;
                ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
                ctx.clearRect(0, 0, rect.width, rect.height);
                ctx.fillStyle = '#111827';
                ctx.fillRect(0, 0, rect.width, rect.height);
                ctx.fillStyle = '#fbbf24';
                ctx.font = '600 14px sans-serif';
                ctx.fillText('Chart temporarily unavailable', 24, Math.max(32, rect.height / 2 - 8));
                ctx.fillStyle = '#9ca3af';
                ctx.font = '12px sans-serif';
                ctx.fillText('Refresh the signal or reload the dashboard.', 24, Math.max(54, rect.height / 2 + 16));
                console.error('[Chart] render failed', error);
            }

            // Coalesce high-frequency wheel/drag/SSE updates into one canvas paint per frame.
            function drawCandleChart(signal) {
                chartPendingSignal = signal;
                if (chartFrameRequest !== null) return;
                chartFrameRequest = requestAnimationFrame(() => {
                    chartFrameRequest = null;
                    const pending = chartPendingSignal;
                    chartPendingSignal = null;
                    if (!pending || (selectedSignal && !isSelectedSignal(pending))) return;
                    try {
                        drawCandleChartNow(pending);
                    } catch (error) {
                        chartViewport = null;
                        showChartRenderError(error);
                    }
                });
            }

            // Candlestick & Dedicated Indicator Drawing (Direct Time & Price Axis Dragging)

    publishDashboardBindings('chart-inspection', {
        chartCrosshair: { get: () => chartCrosshair, set: value => { chartCrosshair = value; } },
        updateOHLCBar: { get: () => updateOHLCBar },
        resetOHLCBarForSignal: { get: () => resetOHLCBarForSignal },
        chartViewport: { get: () => chartViewport, set: value => { chartViewport = value; } },
        chartFrameRequest: { get: () => chartFrameRequest, set: value => { chartFrameRequest = value; } },
        chartPendingSignal: { get: () => chartPendingSignal, set: value => { chartPendingSignal = value; } },
        isSelectedSignal: { get: () => isSelectedSignal },
        showChartRenderError: { get: () => showChartRenderError },
        drawCandleChart: { get: () => drawCandleChart },
    });
}
