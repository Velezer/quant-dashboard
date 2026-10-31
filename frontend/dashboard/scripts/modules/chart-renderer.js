import { installChartCanvasInteractions } from './chart-interactions.js';
import { QuantChart } from './chart-math.js';
import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { drawMomentumPane } from './chart-momentum-renderer.js';
import { pineChartPlots } from './pine-chart-output.js';
import { isIntradayChartTimeframe } from './chart-candle-time.js';
import { estimateVolumeDeltaBars, volumeDeltaExtent } from './volume-delta.js';

/** Draws the selected signal's candles, current price, and indicator pane. */
export function installChartRenderer() {
            function drawCandleChartNow(signal) {
                if (!signal || (selectedSignal && !isSelectedSignal(signal))) return;
                getDashboardBinding('updateMarkovRegimeStatus')?.(signal);
                const canvas = document.getElementById('candleCanvas');
                const dssCanvas = document.getElementById('dssCanvas');
                if (!canvas) return;
                if (!canvas.hasWindowListeners) {
                    canvas.hasWindowListeners = true;
                    installChartCanvasInteractions(canvas, dssCanvas);
                    if (typeof window.addEventListener === 'function' && !window.__quantResizeHandlerInstalled) {
                        window.__quantResizeHandlerInstalled = true;
                        window.addEventListener('resize', () => {
                            if (selectedSignal) drawCandleChart(selectedSignal);
                        }, { passive: true });
                    }
                }
                if (!canvas.hasChartResizeObserver && typeof ResizeObserver === 'function') {
                    const observer = new ResizeObserver(entries => {
                        const rect = entries[0]?.contentRect;
                        if (rect?.width > 0 && rect.height > 0 && selectedSignal) {
                            drawCandleChart(selectedSignal);
                        }
                    });
                    observer.observe(canvas);
                    canvas.chartResizeObserver = observer;
                    canvas.hasChartResizeObserver = true;
                }

                const currentPrice = Number(signal.current_price);
                const validCurrentPrice = Number.isFinite(currentPrice) && currentPrice > 0 ? currentPrice : null;
                const isIdrChart = signal.class === 'idx' || String(signal.asset || '').toUpperCase().endsWith('.JK');
                const chartCurrency = isIdrChart ? 'IDR' : 'USD';
                const venue = executionVenueMeta(signal);
                const signalTimeframe = String(signal.timeframe || '--').toUpperCase();
                const volumeDeltaEnabled = getDashboardBinding('showVolumeDelta') === true;
                const latestMarkovState = signal.markov_regimes_by_tf?.[activeTimeframe]?.at(-1)?.state;
                const markovDescription = getDashboardBinding('showMarkovRegimeIndicator') === false
                    ? 'Markov regime ribbon hidden.'
                    : `Markov regime ribbon shows causal historical states; latest is ${latestMarkovState?.replaceAll('_', ' ') || 'warming up or unavailable'}.`;
                canvas.setAttribute('aria-label', `${venue.label} · ${signal.asset || signal.symbol || 'Selected asset'} · ${activeTimeframe.toUpperCase()} chart. Signal timeframe ${signalTimeframe}. Signal price ${validCurrentPrice === null ? 'unavailable' : formatPrice(validCurrentPrice)}. ${volumeDeltaEnabled ? 'Volume delta estimate from chart-bar direction.' : 'Synchronized volume histogram.'} ${markovDescription} Plus/minus zoom, arrows pan, Home fits.`);
                if (typeof updateChartDataWarning === 'function') updateChartDataWarning(signal);

                const ctx = canvas.getContext('2d');
                if (!ctx) return;
                const rect = canvas.getBoundingClientRect();
                if (!rect.width || !rect.height) {
                    if (typeof setChartStatus === 'function') {
                        setChartStatus('loading', 'Waiting for the chart area to finish laying out.', { signal });
                    }
                    return;
                }
                const pixelRatio = window.devicePixelRatio || 1;
                const bitmapWidth = Math.round(rect.width * pixelRatio);
                const bitmapHeight = Math.round(rect.height * pixelRatio);
                if (canvas.width !== bitmapWidth) canvas.width = bitmapWidth;
                if (canvas.height !== bitmapHeight) canvas.height = bitmapHeight;
                ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);

                const width = rect.width;
                const height = rect.height;
                ctx.clearRect(0, 0, width, height);

                const allCandles = getChartCandles(signal);
                const allVolumeDeltaBars = estimateVolumeDeltaBars(allCandles);
                getDashboardBinding('updateVolumeDeltaStatus')?.(allCandles, signal);
                if (allCandles.length === 0) {
                    chartViewport = null;
                    clearMomentumSummary();
                    updateNadarayaWatsonStatus([], signal);
                    const meta = signal.chart_freshness?.[activeTimeframe];
                    const reason = String(meta?.chart_warning || meta?.warning || '').trim();
                    if (typeof setChartStatus === 'function') setChartStatus('empty', reason || `No ${activeTimeframe.toUpperCase()} candlestick bars are available for ${signal.asset || 'this asset'} in the selected app history.`, { signal });
                    ctx.fillStyle = '#9ca3af';
                    ctx.font = '14px sans-serif';
                    ctx.fillText(`No ${activeTimeframe.toUpperCase()} candlestick bars available for this asset`, width / 2 - 140, height / 2);
                    if (signal.pine_indicator_ui_state) {
                        drawMomentumPane({
                            dssCanvas,
                            pixelRatio,
                            candles: [],
                            allCandles: [],
                            startIndex: 0,
                            barFraction: 0,
                            stepX: 0,
                            barW: 0,
                            chartCrosshair,
                            registeredPaneOutput: null,
                            registeredPaneState: signal.pine_indicator_ui_state,
                            chartTimeframe: activeTimeframe,
                            chartIndicatorScale,
                        });
                    } else if (dssCanvas) {
                        const dCtx = dssCanvas.getContext('2d');
                        if (dCtx) dCtx.clearRect(0, 0, dssCanvas.width, dssCanvas.height);
                    }
                    return;
                }
                if (typeof setChartStatus === 'function') setChartStatus('ready', '', { signal });

                // Dynamic Window Slicing with Pan Offset
                const totalBars = allCandles.length;
                if (chartViewport && chartViewport.key === QuantChart.key(signal) && chartViewport.timeframe === activeTimeframe
                    && chartViewport.allCandles !== allCandles && windowPanOffset > 0) {
                    const priorIndex = Math.floor(chartViewport.left);
                    const priorDate = chartViewport.allCandles[priorIndex]?.date;
                    const index = allCandles.findIndex(c => c.date === priorDate);
                    if (index >= 0) windowPanOffset = totalBars - (index + chartViewport.left - priorIndex) - activeWindowSize;
                }
                const view = QuantChart.viewport(totalBars, activeWindowSize, windowPanOffset);
                const windowSize = view.size;
                activeWindowSize = view.size;
                windowPanOffset = view.offset;
                const endIndex = Math.min(totalBars, Math.ceil(view.right));
                const startIndex = Math.max(0, Math.floor(view.left));
                const barFraction = view.left - startIndex;
                const candles = allCandles.slice(startIndex, endIndex);
                updateNadarayaWatsonStatus(candles, signal);

                if (!chartCrosshair.active && candles.length > 0) {
                    updateOHLCBar(candles[candles.length - 1], signal);
                }

                // Shared Horizontal Alignment (Perfect alignment between main price chart & indicator)
                // Reserve a small volume histogram band above the shared time
                // axis. This keeps the price pane honest while giving the chart
                // the same glanceable participation cue traders expect in
                // TradingView-style layouts.
                const volumeBandH = Math.min(58, Math.max(34, height * 0.16));
                const padding = { top: 20, bottom: 30 + volumeBandH, left: 15, right: 96 };
                const plotW = width - padding.left - padding.right;
                const pricePanelH = Math.max(120, height - padding.top - padding.bottom);
                const volumeTop = padding.top + pricePanelH + 5;
                const volumeBottom = height - 24;

                let minP = Infinity;
                let maxP = -Infinity;
                candles.forEach(c => {
                    if (c.low < minP) minP = c.low;
                    if (c.high > maxP) maxP = c.high;
                });

                if (showNadarayaWatsonEnvelope) {
                    candles.forEach(c => {
                        const envelope = activeNadarayaWatsonSeries(c);
                        if ([envelope?.upper, envelope?.lower].every(Number.isFinite)) {
                            minP = Math.min(minP, envelope.lower);
                            maxP = Math.max(maxP, envelope.upper);
                        }
                    });
                }

                const pinePlots = pineChartPlots(signal.pine_indicator_outputs, activeTimeframe, allCandles.length);
                for (const plot of pinePlots.overlayPlots) {
                    for (const value of plot.values) {
                        if (Number.isFinite(value)) { minP = Math.min(minP, value); maxP = Math.max(maxP, value); }
                    }
                }

                if (signal.current_price) { minP = Math.min(minP, signal.current_price); maxP = Math.max(maxP, signal.current_price); }

                const zones = signal.liquidity_zones || (signal.risk_profile && signal.risk_profile.liquidity_zones) || [];
                if (showLiquidationZones && zones && zones.length > 0) {
                    zones.forEach(z => {
                        if (signal.current_price && Math.abs(z.mid_price - signal.current_price) / signal.current_price < 0.20) {
                            minP = Math.min(minP, z.price_low);
                            maxP = Math.max(maxP, z.price_high);
                        }
                    });
                }

                let rawSpan = maxP - minP;
                if (rawSpan <= 0 || (minP > 0 && rawSpan < minP * 0.02)) {
                    const dynamicBuf = minP > 0 ? (minP * 0.04) : 1.0;
                    minP = Math.max(1e-8, minP - dynamicBuf);
                    maxP = maxP + dynamicBuf;
                    rawSpan = maxP - minP;
                }

                const pRange = rawSpan || (minP > 0 ? minP * 0.05 : 1.0);
                const marginP = pRange * 0.08;
                minP -= marginP;
                maxP += marginP;

                // Snap automatic linear bounds to stable, readable price steps.
                // Small pan changes then leave the axis steady until a real
                // candle extreme crosses the next price interval.
                if (!chartLogScale) {
                    const automaticScale = QuantChart.priceScale(minP, maxP, 5);
                    minP = automaticScale.min;
                    maxP = automaticScale.max;
                }

                // Apply Direct Vertical Price Scale
                const rawMidP = (minP + maxP) / 2.0;
                const rawHalfSpan = (maxP - minP) / 2.0;
                const effectivePriceScale = Math.max(0.3, Math.min(3.0, chartPriceScale || 1.0));
                const scaledHalfSpan = rawHalfSpan / effectivePriceScale;
                const midP = rawMidP + (chartPriceOffset || 0.0);

                minP = midP - scaledHalfSpan;
                maxP = midP + scaledHalfSpan;
                if (chartLogScale && minP <= 0) {
                    minP = 1e-8;
                }
                const finalRange = maxP - minP || (minP > 0 ? minP * 0.05 : 1.0);
                window.__lastCandleSpan = finalRange;

                // Log scale precomputation
                const useLogScale = chartLogScale && minP > 0;
                const logMinP = useLogScale ? Math.log(minP) : 0;
                const logMaxP = useLogScale ? Math.log(maxP) : 0;
                const logRange = useLogScale ? (logMaxP - logMinP || 1) : 1;

                function getY(p) {
                    if (useLogScale) {
                        return padding.top + (logMaxP - Math.log(Math.max(p, 1e-8))) / logRange * pricePanelH;
                    }
                    return padding.top + (maxP - p) / finalRange * pricePanelH;
                }

                const n = candles.length;
                const barW = Math.max(0.6, plotW / windowSize * 0.65);
                const stepX = plotW / windowSize;
                const indicatorX = index => padding.left + (index - barFraction) * stepX + stepX / 2;

                function drawChartIndicatorLine(valueForCandle, colorForCandle, lineWidth, dashPattern = []) {
                    let activeColor = null;
                    let points = [];
                    const flush = () => {
                        if (points.length > 1) {
                            ctx.save();
                            ctx.strokeStyle = activeColor;
                            ctx.lineWidth = lineWidth;
                            ctx.setLineDash(dashPattern);
                            ctx.beginPath();
                            ctx.moveTo(points[0].x, points[0].y);
                            points.slice(1).forEach(point => ctx.lineTo(point.x, point.y));
                            ctx.stroke();
                            ctx.restore();
                        }
                        points = [];
                        activeColor = null;
                    };
                    candles.forEach((candle, index) => {
                        const value = valueForCandle(candle, index);
                        const color = colorForCandle(candle, index);
                        if (!Number.isFinite(value) || !color) {
                            flush();
                            return;
                        }
                        if (activeColor !== null && color !== activeColor) flush();
                        activeColor = color;
                        points.push({ x: indicatorX(index), y: getY(value) });
                    });
                    flush();
                }

                function drawChartIndicatorMarkers(markersForCandle) {
                    ctx.save();
                    candles.forEach((candle, index) => {
                        for (const marker of markersForCandle(candle)) {
                            if (!Number.isFinite(marker.value)) continue;
                            const x = indicatorX(index);
                            const y = getY(marker.value) + (marker.direction === 'up' ? -8 : 8);
                            ctx.fillStyle = marker.color;
                            ctx.beginPath();
                            if (marker.direction === 'up') {
                                ctx.moveTo(x, y - 4);
                                ctx.lineTo(x - 4, y + 3);
                                ctx.lineTo(x + 4, y + 3);
                            } else {
                                ctx.moveTo(x, y + 4);
                                ctx.lineTo(x - 4, y - 3);
                                ctx.lineTo(x + 4, y - 3);
                            }
                            ctx.closePath();
                            ctx.fill();
                        }
                    });
                    ctx.restore();
                }

                chartViewport = { ...view, key: QuantChart.key(signal), timeframe: activeTimeframe, allCandles, candles, startIndex, endIndex, stepX, minP, maxP, finalRange, useLogScale, logMaxP, logRange, padding, pricePanelH, volumeTop, volumeBottom };

                // LuxAlgo displays two boundaries without a shaded fill. Repainting
                // mode is dashed and historical values can move as bars arrive.
                if (showNadarayaWatsonEnvelope) {
                    const envelopeFor = candle => activeNadarayaWatsonSeries(candle);
                    drawChartIndicatorLine(
                        candle => envelopeFor(candle)?.upper,
                        () => '#14b8a6',
                        1.2,
                        nadarayaWatsonRepaintingEnabled(signal) ? [4, 3] : [],
                    );
                    drawChartIndicatorLine(
                        candle => envelopeFor(candle)?.lower,
                        () => '#ef4444',
                        1.2,
                        nadarayaWatsonRepaintingEnabled(signal) ? [4, 3] : [],
                    );
                }

                for (const plot of pinePlots.overlayPlots) {
                    const valueAt = (_candle, index) => plot.values[startIndex + index];
                    if (plot.kind === 'shape' || plot.kind === 'shape_up' || plot.kind === 'shape_down') {
                        ctx.save();
                        candles.forEach((_candle, index) => {
                            const value = valueAt(null, index);
                            if (!Number.isFinite(value)) return;
                            ctx.fillStyle = plot.color || '#2962ff';
                            ctx.beginPath();
                            const x = indicatorX(index);
                            const y = getY(value);
                            if (plot.kind === 'shape_up') {
                                ctx.moveTo(x, y - 4);
                                ctx.lineTo(x - 4, y + 3);
                                ctx.lineTo(x + 4, y + 3);
                                ctx.closePath();
                            } else if (plot.kind === 'shape_down') {
                                ctx.moveTo(x, y + 4);
                                ctx.lineTo(x - 4, y - 3);
                                ctx.lineTo(x + 4, y - 3);
                                ctx.closePath();
                            } else {
                                ctx.arc(x, y, 3, 0, Math.PI * 2);
                            }
                            ctx.fill();
                        });
                        ctx.restore();
                    } else {
                        drawChartIndicatorLine(valueAt, () => plot.color || '#2962ff', 1.6, plot.kind === 'dashed' ? [4, 3] : []);
                    }
                }

                // ==========================================
                // 1. DRAW CANDLESTICK MAIN PRICE CHART (TradingView Dark Theme)
                // ==========================================
                // TradingView Price Grid Lines
                ctx.strokeStyle = '#1e222d';
                ctx.lineWidth = 1;
                const gridSteps = 5;
                const gridPrices = useLogScale
                    ? Array.from({ length: gridSteps + 1 }, (_, index) => Math.exp(logMinP + (index / gridSteps) * logRange))
                    : QuantChart.priceScale(minP, maxP, gridSteps).ticks;
                const axisTagPrices = [signal.current_price]
                    .map(Number)
                    .filter(price => Number.isFinite(price) && price > 0 && price >= minP && price <= maxP);
                for (const p of gridPrices) {
                    const y = getY(p);
                    ctx.beginPath();
                    ctx.moveTo(padding.left, y);
                    ctx.lineTo(width - padding.right, y);
                    ctx.stroke();

                    ctx.fillStyle = '#787b86';
                    ctx.font = '10px -apple-system, BlinkMacSystemFont, "Trebuchet MS", monospace';
                    ctx.textAlign = 'left';
                    if (!axisTagPrices.some(price => Math.abs(getY(price) - y) < 11)) {
                        ctx.fillText(formatPrice(p), width - padding.right + 6, y + 3.5);
                    }
                }

                // TradingView Right Price Axis Separator Line
                ctx.strokeStyle = '#2a2e39';
                ctx.lineWidth = 1;
                ctx.beginPath();
                ctx.moveTo(width - padding.right, 0);
                ctx.lineTo(width - padding.right, height);
                ctx.stroke();

                if (getDashboardBinding('showMarkovRegimeIndicator') !== false) {
                    const history = signal.markov_regimes_by_tf?.[activeTimeframe];
                    const regimeColors = {
                        BULL_TREND: '#10b981',
                        BEAR_TREND: '#f43f5e',
                        ACCUMULATION: '#06b6d4',
                        DISTRIBUTION: '#f97316',
                        PULLBACK_RESET: '#a78bfa',
                        CONSOLIDATION_CHOP: '#94a3b8',
                    };
                    if (Array.isArray(history)) {
                        ctx.save();
                        candles.forEach((_candle, index) => {
                            const state = history[startIndex + index]?.state;
                            const color = regimeColors[state];
                            if (!color) return;
                            const x = padding.left + (index - barFraction) * stepX;
                            ctx.fillStyle = color;
                            ctx.fillRect(x, padding.top + 2, Math.max(1, stepX + 0.5), 5);
                        });
                        ctx.restore();
                    }
                }

                // Price Scale Badge if modified (Placed in top-left of chart plot area so it never collides with price axis tags)
                if (Math.abs(effectivePriceScale - 1.0) > 0.02) {
                    const bX = padding.left + 8;
                    const bY = padding.top + 6;
                    const bW = 72;
                    const bH = 18;
                    ctx.fillStyle = 'rgba(15, 23, 42, 0.85)';
                    ctx.strokeStyle = '#2563eb';
                    ctx.lineWidth = 1;
                    ctx.beginPath();
                    if (ctx.roundRect) ctx.roundRect(bX, bY, bW, bH, 4);
                    else ctx.rect(bX, bY, bW, bH);
                    ctx.fill();
                    ctx.stroke();

                    ctx.fillStyle = '#60a5fa';
                    ctx.font = 'bold 9px monospace';
                    ctx.textAlign = 'center';
                    ctx.fillText(`SCALE ${effectivePriceScale.toFixed(1)}x ↺`, bX + bW / 2, bY + 12);
                }

                // Log scale indicator badge
                if (useLogScale) {
                    ctx.fillStyle = 'rgba(41, 98, 255, 0.18)';
                    ctx.fillRect(width - padding.right - 36, padding.top + pricePanelH - 18, 32, 16);
                    ctx.fillStyle = '#2962ff';
                    ctx.font = 'bold 9px monospace';
                    ctx.textAlign = 'center';
                    ctx.fillText('LOG', width - padding.right - 20, padding.top + pricePanelH - 7);
                }

                // Liquidation Shelves (10x, 25x) - Rendered only when showLiquidationZones is toggled ON
                if (showLiquidationZones && zones && zones.length > 0) {
                    zones.forEach(z => {
                        if (z.price_high < minP || z.price_low > maxP) return;
                        const isLiq = z.zone_type.includes('LIQUIDATION');
                        if (isLiq) {
                            const yLiq = getY(z.mid_price);
                            ctx.strokeStyle = 'rgba(168, 85, 247, 0.75)';
                            ctx.lineWidth = 1.2;
                            ctx.setLineDash([4, 4]);
                            ctx.beginPath();
                            ctx.moveTo(padding.left, yLiq);
                            ctx.lineTo(width - padding.right, yLiq);
                            ctx.stroke();
                            ctx.setLineDash([]);

                            ctx.fillStyle = '#c084fc';
                            ctx.font = 'bold 9px monospace';
                            ctx.textAlign = 'right';
                            ctx.fillText(`⚡ ${z.leverage_tier || 10}x LIQ (${formatPrice(z.mid_price)})`, width - padding.right - 10, yLiq - 3);
                        }
                    });
                }

                ctx.setLineDash([]);

                // Current Price Live Line
                if (signal.current_price) {
                    const y = getY(signal.current_price);
                    ctx.strokeStyle = '#f59e0b';
                    ctx.lineWidth = 1.5;
                    ctx.beginPath();
                    ctx.moveTo(padding.left, y);
                    ctx.lineTo(width - padding.right, y);
                    ctx.stroke();
                }

                // Right-Axis TradingView Pill Tags Queue with Anti-Collision / Collision Avoidance
                const axisTagsToDraw = [];
                function queuePriceAxisTag(price, bgColor, textColor, labelPrefix, priority = 1) {
                    if (price === null || price === undefined || isNaN(price)) return;
                    const numP = Number(price);
                    if (numP <= 0) return;
                    const rawY = getY(numP);
                    const txt = `${labelPrefix}${formatPrice(numP)}`;
                    ctx.font = 'bold 9px monospace';
                    const textMetrics = ctx.measureText(txt);
                    const tagW = Math.max(78, Math.min(padding.right - 4, Math.round(textMetrics.width + 8)));
                    axisTagsToDraw.push({
                        price: numP,
                        rawY,
                        y: rawY,
                        bgColor,
                        textColor,
                        txt,
                        tagW,
                        priority
                    });
                }

                if (signal.current_price) queuePriceAxisTag(signal.current_price, '#f59e0b', '#000000', 'SIGNAL ', 10);

                // Candlestick Bodies & Wicks (TradingView Pine Green #089981 & Coral Red #f23645)
                candles.forEach((c, i) => {
                    const x = padding.left + (i - barFraction) * stepX + stepX / 2;
                    const isBull = c.close >= c.open;
                    const color = isBull ? '#089981' : '#f23645';

                    // Wick
                    ctx.strokeStyle = color;
                    ctx.lineWidth = 1.2;
                    ctx.beginPath();
                    ctx.moveTo(x, getY(c.high));
                    ctx.lineTo(x, getY(c.low));
                    ctx.stroke();

                    // Candle Body
                    const yOpen = getY(c.open);
                    const yClose = getY(c.close);
                    const bodyTop = Math.min(yOpen, yClose);
                    const bodyH = Math.max(2, Math.abs(yClose - yOpen));

                    ctx.fillStyle = color;
                    ctx.fillRect(x - barW / 2, bodyTop, barW, bodyH);
                });

                // Compact volume histogram or the selected chart-bar delta
                // estimate. Both share candle geometry so panning stays aligned.
                const volumeValues = candles.map(c => Number(c.volume));
                const maxVolume = volumeValues.reduce((max, value) => Number.isFinite(value) && value > max ? value : max, 0);
                const volumePlotHeight = Math.max(8, volumeBottom - volumeTop - 12);
                ctx.strokeStyle = '#2a2e39';
                ctx.lineWidth = 1;
                ctx.beginPath();
                ctx.moveTo(padding.left, volumeTop - 1);
                ctx.lineTo(width - padding.right, volumeTop - 1);
                ctx.stroke();
                ctx.fillStyle = '#787b86';
                ctx.font = 'bold 9px monospace';
                ctx.textAlign = 'left';
                if (volumeDeltaEnabled) {
                    const visibleDeltaBars = candles.map((_candle, index) => allVolumeDeltaBars[startIndex + index]);
                    const { maxAbsDelta, hasVolume } = volumeDeltaExtent(visibleDeltaBars);
                    ctx.fillText('VOL Δ', padding.left, volumeTop + 10);
                    if (hasVolume && maxAbsDelta > 0) {
                        const centerY = volumeTop + 12 + (volumeBottom - volumeTop - 18) / 2;
                        const halfHeight = Math.max(4, (volumeBottom - volumeTop - 18) / 2);
                        ctx.strokeStyle = 'rgba(120, 123, 134, 0.45)';
                        ctx.lineWidth = 1;
                        ctx.beginPath();
                        ctx.moveTo(padding.left, centerY);
                        ctx.lineTo(width - padding.right, centerY);
                        ctx.stroke();
                        visibleDeltaBars.forEach((deltaBar, i) => {
                            if (!Number.isFinite(deltaBar?.close)) return;
                            const x = padding.left + (i - barFraction) * stepX + stepX / 2;
                            const yForDelta = value => centerY - value / maxAbsDelta * halfHeight;
                            const color = deltaBar.close >= 0 ? '#089981' : '#f23645';
                            ctx.strokeStyle = color;
                            ctx.lineWidth = Math.max(1, Math.min(1.4, barW));
                            ctx.beginPath();
                            ctx.moveTo(x, yForDelta(deltaBar.high));
                            ctx.lineTo(x, yForDelta(deltaBar.low));
                            ctx.stroke();
                            const yOpen = yForDelta(deltaBar.open);
                            const yClose = yForDelta(deltaBar.close);
                            ctx.fillStyle = color;
                            ctx.fillRect(x - Math.max(1, barW / 2), Math.min(yOpen, yClose), Math.max(1, barW), Math.max(1, Math.abs(yClose - yOpen)));
                        });
                        const latestDelta = visibleDeltaBars.at(-1)?.close;
                        const absDelta = Math.abs(latestDelta);
                        const deltaLabel = absDelta >= 1e9 ? `${(absDelta / 1e9).toFixed(1)}B`
                            : absDelta >= 1e6 ? `${(absDelta / 1e6).toFixed(1)}M`
                                : absDelta >= 1e3 ? `${(absDelta / 1e3).toFixed(1)}K`
                                    : `${Math.round(absDelta)}`;
                        ctx.textAlign = 'right';
                        ctx.fillStyle = latestDelta >= 0 ? '#5eead4' : '#fda4af';
                        ctx.fillText(`VOL Δ ${latestDelta >= 0 ? '+' : '−'}${deltaLabel}`, width - padding.right - 6, volumeTop + 10);
                    } else {
                        ctx.fillStyle = '#64748b';
                        ctx.font = '9px monospace';
                        ctx.fillText(hasVolume ? 'VOL Δ · ZERO' : 'VOLUME · UNAVAILABLE', padding.left + 42, volumeTop + 10);
                    }
                } else if (maxVolume > 0) {
                    ctx.fillText('VOL', padding.left, volumeTop + 10);
                    candles.forEach((c, i) => {
                        const volume = Number(c.volume);
                        if (!Number.isFinite(volume) || volume <= 0) return;
                        const x = padding.left + (i - barFraction) * stepX + stepX / 2;
                        const barHeight = Math.max(1, volume / maxVolume * volumePlotHeight);
                        ctx.fillStyle = c.close >= c.open ? 'rgba(8, 153, 129, 0.62)' : 'rgba(242, 54, 69, 0.62)';
                        ctx.fillRect(x - barW / 2, volumeBottom - barHeight, Math.max(1, barW), barHeight);
                    });
                    const latestVolume = volumeValues[volumeValues.length - 1];
                    const volumeLabel = latestVolume >= 1e9 ? `${(latestVolume / 1e9).toFixed(1)}B`
                        : latestVolume >= 1e6 ? `${(latestVolume / 1e6).toFixed(1)}M`
                            : latestVolume >= 1e3 ? `${(latestVolume / 1e3).toFixed(1)}K`
                                : `${Math.round(latestVolume)}`;
                    ctx.textAlign = 'right';
                    ctx.fillText(`VOL ${volumeLabel}`, width - padding.right - 6, volumeTop + 10);
                } else {
                    ctx.fillText('VOL', padding.left, volumeTop + 10);
                    ctx.fillStyle = '#64748b';
                    ctx.font = '9px monospace';
                    ctx.fillText('VOLUME · UNAVAILABLE', padding.left + 32, volumeTop + 10);
                }

                // The main price chart owns the time axis. Keep all candle dates,
                // zoom context, and cursor time information attached to price.
                ctx.strokeStyle = '#2a2e39';
                ctx.lineWidth = 1;
                ctx.beginPath();
                ctx.moveTo(padding.left, height - 22);
                ctx.lineTo(width - padding.right, height - 22);
                ctx.stroke();
                candles.forEach((c, i) => {
                    if (i % Math.max(1, Math.ceil(n / 6)) !== 0 && i !== n - 1) return;
                    const x = padding.left + (i - barFraction) * stepX + stepX / 2;
                    ctx.fillStyle = '#787b86';
                    ctx.font = '9px monospace';
                    ctx.textAlign = 'center';
                    const intraday = isIntradayChartTimeframe(activeTimeframe);
                    const rawDate = c.date || '';
                    const label = intraday ? rawDate : (rawDate.length > 5 ? rawDate.slice(5) : rawDate);
                    ctx.fillText(label, x, height - 7);
                });

                if (showNadarayaWatsonEnvelope) {
                    // Match the 4h research direction mapping: lower crosses
                    // point up for LONG and upper crosses point down for SHORT.
                    drawChartIndicatorMarkers(candle => {
                        const envelope = activeNadarayaWatsonSeries(candle);
                        const markers = [];
                        if (envelope?.crossed_below_lower) {
                            markers.push({ value: envelope.lower, direction: 'up', color: '#14b8a6' });
                        }
                        if (envelope?.crossed_above_upper) {
                            markers.push({ value: envelope.upper, direction: 'down', color: '#ef4444' });
                        }
                        return markers;
                    });
                }

                // Render All Right-Axis Price Tags with Anti-Collision Resolution
                function renderResolvedAxisTags() {
                    if (axisTagsToDraw.length === 0) return;
                    const tagH = 18;
                    const minY = padding.top + tagH / 2;
                    const maxY = padding.top + pricePanelH - tagH / 2;

                    // Sort ascending by raw Y (top to bottom on screen)
                    axisTagsToDraw.sort((a, b) => a.rawY - b.rawY);

                    // Anti-collision relaxation iterations (separate tags whose distance < tagH)
                    for (let iter = 0; iter < 6; iter++) {
                        for (let i = 0; i < axisTagsToDraw.length - 1; i++) {
                            const cur = axisTagsToDraw[i];
                            const nxt = axisTagsToDraw[i + 1];
                            const dist = nxt.y - cur.y;
                            if (dist < tagH) {
                                const overlap = tagH - dist;
                                if (cur.priority > nxt.priority) {
                                    nxt.y += overlap;
                                } else if (nxt.priority > cur.priority) {
                                    cur.y -= overlap;
                                } else {
                                    cur.y -= overlap / 2;
                                    nxt.y += overlap / 2;
                                }
                            }
                        }
                        for (let i = 0; i < axisTagsToDraw.length; i++) {
                            if (axisTagsToDraw[i].y < minY) axisTagsToDraw[i].y = minY;
                            if (axisTagsToDraw[i].y > maxY) axisTagsToDraw[i].y = maxY;
                        }
                    }

                    axisTagsToDraw.forEach(tag => {
                        const tagX = width - padding.right + 2;
                        const tagY = Math.round(tag.y - tagH / 2);

                        // Subtle connector tick if tag was vertically displaced from true price level
                        if (Math.abs(tag.y - tag.rawY) > 2.5) {
                            ctx.strokeStyle = tag.bgColor;
                            ctx.lineWidth = 1;
                            ctx.beginPath();
                            ctx.moveTo(width - padding.right, tag.rawY);
                            ctx.lineTo(tagX + 3, tag.y);
                            ctx.stroke();
                        }

                        // Solid pill container with shadow to occlude background grid lines
                        ctx.save();
                        ctx.shadowColor = 'rgba(0, 0, 0, 0.6)';
                        ctx.shadowBlur = 4;
                        ctx.fillStyle = tag.bgColor;
                        ctx.beginPath();
                        if (ctx.roundRect) {
                            ctx.roundRect(tagX, tagY, tag.tagW, tagH, 3);
                        } else {
                            ctx.rect(tagX, tagY, tag.tagW, tagH);
                        }
                        ctx.fill();
                        ctx.restore();

                        ctx.fillStyle = tag.textColor;
                        ctx.font = 'bold 9px monospace';
                        ctx.textAlign = 'left';
                        ctx.fillText(tag.txt, tagX + 4, tag.y + 3.5);
                    });
                }

                renderResolvedAxisTags();

                // TradingView Interactive Crosshair & Floating Price Badge on Candle Canvas
                if (chartCrosshair.active) {
                    ctx.save();
                    ctx.strokeStyle = 'rgba(120, 123, 134, 0.7)';
                    ctx.lineWidth = 1;
                    ctx.setLineDash([3, 3]);

                    // Vertical Crosshair
                    ctx.beginPath();
                    ctx.moveTo(chartCrosshair.x, padding.top);
                    ctx.lineTo(chartCrosshair.x, height - 22);
                    ctx.stroke();

                    // Horizontal Crosshair
                    if (chartCrosshair.y !== null && chartCrosshair.y >= padding.top && chartCrosshair.y <= height - padding.bottom) {
                        ctx.beginPath();
                        ctx.moveTo(padding.left, chartCrosshair.y);
                        ctx.lineTo(width - padding.right, chartCrosshair.y);
                        ctx.stroke();

                        // Floating Price Pill on Right Axis
                        const tagH = 18;
                        const txt = formatPrice(chartCrosshair.price);
                        ctx.font = 'bold 9px monospace';
                        const textMetrics = ctx.measureText(txt);
                        const tagW = Math.max(78, Math.min(padding.right - 4, Math.round(textMetrics.width + 8)));
                        const tagX = width - padding.right + 2;
                        const tagY = chartCrosshair.y - tagH / 2;
                        ctx.fillStyle = '#2a2e39';
                        ctx.strokeStyle = '#434651';
                        ctx.lineWidth = 1;
                        ctx.beginPath();
                        if (ctx.roundRect) {
                            ctx.roundRect(tagX, tagY, tagW, tagH, 3);
                        } else {
                            ctx.rect(tagX, tagY, tagW, tagH);
                        }
                        ctx.fill();
                        ctx.stroke();

                         ctx.fillStyle = '#ffffff';
                         ctx.textAlign = 'left';
                         ctx.fillText(txt, tagX + 4, chartCrosshair.y + 3.5);
                     }

                     // Floating time pill on the bottom axis. Keep the candle time
                     // attached to the same crosshair bar as the right-axis price.
                     const timeTxt = formatChartCandleTime(chartCrosshair.candle);
                     ctx.font = 'bold 9px monospace';
                     const timeMetrics = ctx.measureText(timeTxt);
                     const availableTimeW = Math.max(64, width - padding.left - padding.right - 4);
                     const timeTagW = Math.min(availableTimeW, Math.max(64, Math.round(timeMetrics.width + 12)));
                     const timeTagH = 18;
                     const timeTagX = Math.max(padding.left, Math.min(width - padding.right - timeTagW, chartCrosshair.x - timeTagW / 2));
                     const timeTagY = height - timeTagH - 2;
                     ctx.fillStyle = '#2a2e39';
                     ctx.strokeStyle = '#434651';
                     ctx.lineWidth = 1;
                     ctx.beginPath();
                     if (ctx.roundRect) {
                         ctx.roundRect(timeTagX, timeTagY, timeTagW, timeTagH, 3);
                     } else {
                         ctx.rect(timeTagX, timeTagY, timeTagW, timeTagH);
                     }
                     ctx.fill();
                     ctx.stroke();
                     ctx.fillStyle = '#ffffff';
                     ctx.textAlign = 'center';
                     ctx.fillText(timeTxt, timeTagX + timeTagW / 2, timeTagY + 12.5);
                     ctx.restore();
                 }

                // ==========================================
                drawMomentumPane({
                    dssCanvas,
                    pixelRatio,
                    candles,
                    allCandles,
                    startIndex,
                    barFraction,
                    stepX,
                    barW,
                    chartCrosshair,
                    registeredPaneOutput: pinePlots.paneOutput,
                    registeredPaneState: signal.pine_indicator_ui_state,
                    chartTimeframe: activeTimeframe,
                    chartIndicatorScale,
                });


            }
    publishDashboardBindings('chart-renderer', {
        drawCandleChartNow: { get: () => drawCandleChartNow },
    });
}
