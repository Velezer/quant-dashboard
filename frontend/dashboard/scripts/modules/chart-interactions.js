/** Binds chart gestures to the current chart state. */
import { QuantChart } from './chart-math.js';
export function installChartCanvasInteractions(canvas, dssCanvas) {
                    const indicatorFitButton = document.getElementById('dssScaleFitBtn');
                    if (indicatorFitButton && !indicatorFitButton.hasScaleFitListener) {
                        indicatorFitButton.hasScaleFitListener = true;
                        indicatorFitButton.addEventListener('click', event => {
                            event.preventDefault();
                            event.stopPropagation();
                            resetIndicatorScale();
                        });
                    }
                    let drag = null;
                    const surfaces = [canvas, dssCanvas].filter(Boolean);
                    surfaces.forEach(surface => {
                        surface.style.touchAction = 'none';
                        surface.tabIndex = 0;
                        surface.setAttribute('aria-keyshortcuts', surface === dssCanvas
                            ? '+ - ArrowLeft ArrowRight Home Shift+ArrowUp Shift+ArrowDown'
                            : '+ - ArrowLeft ArrowRight Home');
                        surface.addEventListener('wheel', event => handleChartWheel(event, surface), { passive: false });
                        surface.addEventListener('dblclick', event => {
                            const bounds = surface.getBoundingClientRect();
                            if (surface === canvas && event.clientX - bounds.left >= bounds.width - 96) resetPriceScale();
                            else if (surface === dssCanvas && event.clientX - bounds.left >= bounds.width - 96) resetIndicatorScale();
                            else resetChartAll();
                        });
                        surface.addEventListener('keydown', event => {
                            if (!selectedSignal || event.altKey || event.metaKey || event.ctrlKey) return;
                            if (surface === dssCanvas && event.shiftKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
                                const factor = event.key === 'ArrowUp' ? 1.15 : 1 / 1.15;
                                const anchor = (chartIndicatorScale.min + chartIndicatorScale.max) / 2;
                                setIndicatorScale(QuantChart.zoomValueScale(chartIndicatorScale.min, chartIndicatorScale.max, anchor, factor));
                            } else if (event.shiftKey) return;
                            else if (event.key === '+' || event.key === '=') setChartWindowSize(activeWindowSize / 1.15);
                            else if (event.key === '-') setChartWindowSize(activeWindowSize * 1.15);
                            else if (event.key === 'Home' || event.key === '0') resetChartAll();
                            else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                                const direction = event.key === 'ArrowLeft' ? 1 : -1;
                                const view = QuantChart.viewport(getChartCandles().length, activeWindowSize, windowPanOffset + direction * activeWindowSize * 0.1);
                                windowPanOffset = view.offset;
                                chartCrosshair.active = false;
                                drawCandleChart(selectedSignal);
                            } else return;
                            event.preventDefault();
                        });
                        surface.addEventListener('pointerdown', event => {
                            if (event.button !== 0 || !chartViewport) return;
                            const bounds = surface.getBoundingClientRect();
                            const x = event.clientX - bounds.left;
                            const y = event.clientY - bounds.top;
                            const onIndicatorAxis = surface === dssCanvas && x >= bounds.width - 96;
                            const onIndicatorPlot = surface === dssCanvas && !onIndicatorAxis;
                            const mode = surface === canvas && x >= bounds.width - 96
                                ? 'price'
                                : onIndicatorAxis ? 'indicator-scale'
                                    : onIndicatorPlot && event.shiftKey ? 'indicator-pan' : 'pan';
                            drag = {
                                surface, id: event.pointerId, mode, x: event.clientX, y: event.clientY,
                                size: activeWindowSize, offset: windowPanOffset, priceScale: chartPriceScale,
                                indicatorScale: { ...chartIndicatorScale },
                                indicatorAnchor: onIndicatorAxis
                                    ? QuantChart.valueAtScaleY(y, 12, Math.max(1, bounds.height - 24), chartIndicatorScale.min, chartIndicatorScale.max)
                                    : null,
                                indicatorPlotHeight: Math.max(1, bounds.height - 24),
                                priceOffset: chartPriceOffset, span: chartViewport.finalRange,
                                plotWidth: Math.max(1, bounds.width - 111), plotHeight: Math.max(1, bounds.height - 40),
                            };
                            surface.setPointerCapture(event.pointerId);
                            surface.focus({ preventScroll: true });
                            chartCrosshair.active = false;
                            surface.style.cursor = mode === 'price' || mode === 'indicator-scale' ? 'ns-resize' : 'grabbing';
                            event.preventDefault();
                        });
                        surface.addEventListener('pointermove', event => {
                            const view = chartViewport;
                            if (!view || !selectedSignal) return;
                            if (drag && event.pointerId === drag.id) {
                                const dx = event.clientX - drag.x;
                                const dy = event.clientY - drag.y;
                                if (drag.mode === 'price') {
                                    setPriceScale(drag.priceScale * Math.exp(-dy * 0.008));
                                } else if (drag.mode === 'indicator-scale' && Number.isFinite(drag.indicatorAnchor)) {
                                    setIndicatorScale(QuantChart.zoomValueScale(
                                        drag.indicatorScale.min,
                                        drag.indicatorScale.max,
                                        drag.indicatorAnchor,
                                        Math.exp(-dy * 0.008),
                                    ));
                                } else if (drag.mode === 'indicator-pan') {
                                    const delta = dy / drag.indicatorPlotHeight * (drag.indicatorScale.max - drag.indicatorScale.min);
                                    setIndicatorScale(QuantChart.panValueScale(
                                        drag.indicatorScale.min,
                                        drag.indicatorScale.max,
                                        delta,
                                    ));
                                } else {
                                    windowPanOffset = QuantChart.viewport(view.allCandles.length, drag.size, drag.offset + dx / drag.plotWidth * drag.size).offset;
                                    if (surface === canvas) chartPriceOffset = drag.priceOffset + dy / drag.plotHeight * drag.span;
                                }
                                drawCandleChart(selectedSignal);
                                return;
                            }
                            const bounds = surface.getBoundingClientRect();
                            const x = event.clientX - bounds.left;
                            const y = event.clientY - bounds.top;
                            const onPriceAxis = surface === canvas && x >= bounds.width - 96;
                            const onIndicatorAxis = surface === dssCanvas && x >= bounds.width - 96;
                            if (onPriceAxis || onIndicatorAxis || x < 15) {
                                surface.style.cursor = onPriceAxis || onIndicatorAxis ? 'ns-resize' : 'ew-resize';
                                chartCrosshair.active = false;
                                drawCandleChart(selectedSignal);
                                return;
                            }
                            surface.style.cursor = 'crosshair';
                            const plotWidth = Math.max(1, bounds.width - 111);
                            const absoluteIndex = Math.floor(view.left + (x - 15) / plotWidth * view.size);
                            const index = QuantChart.clamp(absoluteIndex, 0, view.allCandles.length - 1);
                            const candle = view.allCandles[index];
                            const centerX = 15 + (index + 0.5 - view.left) / view.size * plotWidth;
                            const inPricePanel = surface === canvas
                                && y >= view.padding.top
                                && y <= view.padding.top + view.pricePanelH;
                            const priceFraction = inPricePanel ? (y - view.padding.top) / view.pricePanelH : 0.5;
                            chartCrosshair = {
                                active: true, x: centerX, y: inPricePanel ? y : null,
                                dssY: surface === dssCanvas ? y : null, barIdx: index - view.startIndex,
                                price: view.useLogScale ? Math.exp(view.logMaxP - priceFraction * view.logRange) : view.maxP - priceFraction * view.finalRange,
                                dssVal: surface === dssCanvas
                                    ? QuantChart.valueAtScaleY(y, 12, Math.max(1, bounds.height - 24), chartIndicatorScale.min, chartIndicatorScale.max)
                                    : QuantChart.clamp(100 - (y - 12) / Math.max(1, bounds.height - 38) * 100, 0, 100),
                                candle,
                            };
                            updateOHLCBar(candle, selectedSignal);
                            drawCandleChart(selectedSignal);
                        });
                        const finishDrag = event => {
                            if (drag && drag.id === event.pointerId) {
                                if (surface.hasPointerCapture(event.pointerId)) surface.releasePointerCapture(event.pointerId);
                                drag = null;
                                surface.style.cursor = 'crosshair';
                            }
                        };
                        surface.addEventListener('pointerup', finishDrag);
                        surface.addEventListener('pointercancel', finishDrag);
                        surface.addEventListener('lostpointercapture', finishDrag);
                        surface.addEventListener('pointerleave', () => {
                            if (drag) return;
                            chartCrosshair.active = false;
                            const candles = getChartCandles();
                            if (candles.length) updateOHLCBar(candles[candles.length - 1], selectedSignal);
                            drawCandleChart(selectedSignal);
                        });
                    });
}
