import { QuantChart } from './chart-math.js';
import { NWE_MAX_OUTSIDE_DISTANCE_ID } from './pine-sources.js';

let nativeDssLegendMarkup = null;

export function drawMomentumPane({ dssCanvas, pixelRatio, candles, allCandles, startIndex, barFraction, stepX, barW, chartCrosshair, registeredPaneOutput, registeredPaneState, chartTimeframe, chartIndicatorScale = { min: 0, max: 100 } }) {
    // 2. Draw the dedicated indicator sub-chart.
    // ==========================================
    if (dssCanvas) {
        const momentumPane = document.getElementById('momentum-pane');
        const isRegisteredPaneOutput = Boolean(registeredPaneOutput);
        const isRegisteredDss = registeredPaneOutput?.name === 'DSS Bressert (Double Smoothed Stochastic)';
        const isNweOutsideDistance = String(registeredPaneOutput?.id) === NWE_MAX_OUTSIDE_DISTANCE_ID
            || String(registeredPaneState?.id) === NWE_MAX_OUTSIDE_DISTANCE_ID;
        const isRegisteredPaneSelected = isRegisteredPaneOutput || Boolean(registeredPaneState?.name);
        const wasRegisteredPaneSelected = document.body.classList.contains('pine-indicator-pane-selected');
        if (isRegisteredPaneSelected && !wasRegisteredPaneSelected && momentumPane && !momentumPane.open) momentumPane.open = true;
        document.body.classList.toggle('pine-indicator-pane-selected', isRegisteredPaneSelected);
        const dssRect = dssCanvas.getBoundingClientRect();
        if (dssCanvas.width !== Math.round(dssRect.width * pixelRatio)) dssCanvas.width = Math.round(dssRect.width * pixelRatio);
        if (dssCanvas.height !== Math.round(dssRect.height * pixelRatio)) dssCanvas.height = Math.round(dssRect.height * pixelRatio);
        const dCtx = dssCanvas.getContext('2d');
        if (!dCtx || !dssRect.width || !dssRect.height) return;
        dCtx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);

        const dWidth = dssRect.width;
        const dHeight = dssRect.height;
        const stateTimeframeChanged = registeredPaneState?.state === 'ready'
            && registeredPaneState.chart_timeframe !== chartTimeframe
            && !isRegisteredPaneOutput;
        const registeredState = stateTimeframeChanged ? 'loading' : (registeredPaneState?.state || 'loading');
        const sourceTimeframe = registeredPaneOutput?.requested_timeframes?.[0]
            || registeredPaneOutput?.timeframe
            || registeredPaneState?.requested_timeframe;
        const sourceContext = registeredPaneOutput?.requested_data_contexts?.[0]
            || registeredPaneOutput?.data_context
            || registeredPaneState?.data_context
            || `${sourceTimeframe || 'selected'} source candles`;
        const registeredName = registeredPaneOutput?.name || registeredPaneState?.name || 'Registered Pine indicator';
        const registeredDisplayName = registeredName.replace(' (Double Smoothed Stochastic)', '');
        const registeredAuthor = registeredPaneOutput?.author || registeredPaneState?.author || 'source author not supplied';
        dCtx.clearRect(0, 0, dWidth, dHeight);

        // The main price chart owns the time axis. This indicator panel
        // must not duplicate time controls or labels.
        const dssPadding = { top: 12, bottom: 12, left: 15, right: 96 };
        const dssPlotW = dWidth - dssPadding.left - dssPadding.right;
        const dssPlotH = dHeight - dssPadding.top - dssPadding.bottom;
        const scaleMin = Number.isFinite(chartIndicatorScale?.min) ? chartIndicatorScale.min : 0;
        const scaleMax = Number.isFinite(chartIndicatorScale?.max) && chartIndicatorScale.max > scaleMin ? chartIndicatorScale.max : 100;
        const scaleTicks = QuantChart.priceScale(scaleMin, scaleMax, 5);

        function getDssY(v) {
            return dssPadding.top + (scaleMax - v) / (scaleMax - scaleMin) * dssPlotH;
        }

        if (isRegisteredPaneSelected) {
            const title = momentumPane?.querySelector('.dss-summary-title');
            if (title) title.textContent = `${registeredDisplayName} · ${registeredAuthor}`;
            dssCanvas.setAttribute('aria-label', isRegisteredPaneOutput
                ? `${registeredName} by ${registeredAuthor}, calculated in browser WASM from ${sourceContext}. The indicator pane draws its Pine plots on a ${scaleMin.toFixed(1)} to ${scaleMax.toFixed(1)} value scale${isNweOutsideDistance ? ', as historical excursion percentages and not as a trading signal' : ''}. Drag the right edge or press Shift+ArrowUp/Down to zoom; Shift-drag the plot to pan vertically; use Fit to restore the plotted value range.`
                : `${registeredName} by ${registeredAuthor}. ${registeredState === 'error' ? 'Unavailable: ' : 'Updating: '}${registeredPaneState?.message || 'The registered Pine output is being calculated.'}`);
            const legend = document.getElementById('dssLegend');
            if (legend) {
                nativeDssLegendMarkup ??= legend.innerHTML;
                if (isRegisteredPaneOutput) {
                    const legendItems = document.createDocumentFragment();
                    const sourceItem = document.createElement('div');
                    sourceItem.textContent = `Browser WASM · ${String(sourceTimeframe || registeredPaneOutput.timeframe).toUpperCase()} · ${sourceContext}`;
                    legendItems.append(sourceItem);
                    for (const plot of registeredPaneOutput.plots) {
                        const item = document.createElement('div');
                        const marker = document.createElement('span');
                        marker.className = 'dss-legend-line';
                        marker.style.backgroundColor = plot.color || '#2962ff';
                        const value = plot.kind === 'hline'
                            ? plot.values.find(Number.isFinite)
                            : undefined;
                        const label = document.createElement('span');
                        label.textContent = `${plot.title}${Number.isFinite(value) ? ` · ${value}` : ''}`;
                        item.append(marker, label);
                        legendItems.append(item);
                    }
                    legend.replaceChildren(legendItems);
                } else {
                    legend.replaceChildren(document.createTextNode(registeredState === 'error'
                        ? 'Registered Pine plots are unavailable for this chart.'
                        : 'Registered Pine plots are being calculated…'));
                }
            }
        } else {
            document.body.classList.remove('pine-indicator-pane-selected');
            const title = document.querySelector('#momentum-pane .dss-summary-title');
            if (title) title.textContent = 'Momentum';
            dssCanvas.setAttribute('aria-label', `Dashboard momentum oscillator on the selected chart timeframe, on a ${scaleMin.toFixed(1)} to ${scaleMax.toFixed(1)} value scale. Drag the right edge or press Shift+ArrowUp/Down to zoom; Shift-drag the plot to pan vertically; use Fit to restore the indicator range.`);
            const legend = document.getElementById('dssLegend');
            if (legend && nativeDssLegendMarkup !== null) legend.innerHTML = nativeDssLegendMarkup;
        }

        if (isRegisteredPaneSelected && !isRegisteredPaneOutput) {
            const unavailable = registeredState === 'error';
            const message = unavailable ? 'Registered Pine indicator unavailable' : `Calculating ${registeredDisplayName}…`;
            const detail = unavailable
                ? (registeredPaneState?.message || 'Review Pine indicator diagnostics or retry chart history.')
                : `Evaluating on stored ${String(chartTimeframe || 'chart').toUpperCase()} candles in browser WASM.`;
            dCtx.fillStyle = unavailable ? '#fca5a5' : '#cbd5e1';
            dCtx.font = '600 13px sans-serif';
            dCtx.textAlign = 'center';
            dCtx.fillText(message, dWidth / 2, Math.max(30, dHeight / 2 - 8), Math.max(80, dWidth - 32));
            dCtx.fillStyle = unavailable ? '#fbbf24' : '#9ca3af';
            dCtx.font = '11px sans-serif';
            dCtx.fillText(detail, dWidth / 2, dHeight / 2 + 14, Math.max(80, dWidth - 32));

            const readout = document.getElementById('dssLiveReadout');
            if (readout) {
                readout.textContent = unavailable ? 'Unavailable' : 'Updating…';
                readout.setAttribute('aria-label', `${registeredDisplayName}: ${message}. ${detail}`);
                readout.title = `${registeredDisplayName} by ${registeredAuthor}`;
            }
            const signal = document.getElementById('dssStatusBadge');
            if (signal) {
                signal.textContent = unavailable ? 'Pine unavailable' : 'Pine WASM · calculating';
                signal.className = `dss-signal-status text-[10px] font-semibold px-2 py-0.5 rounded border ${unavailable ? 'bg-rose-950/70 text-rose-300 border-rose-800' : 'bg-amber-950/70 text-amber-300 border-amber-800'}`;
            }
            const compute = document.getElementById('dssIndicatorComputeStatus');
            if (compute) {
                compute.textContent = unavailable ? 'Unavailable' : 'Calculating';
                compute.setAttribute('aria-label', `${message} ${detail}`);
                compute.title = detail;
                compute.className = `dss-compute-status text-[10px] font-semibold px-2 py-0.5 rounded border ${unavailable ? 'bg-rose-950/70 text-rose-300 border-rose-800' : 'bg-amber-950/70 text-amber-300 border-amber-800'}`;
            }
            return;
        }

        // Native momentum uses shaded 80/30 bands. The registered
        // source instead declares plain 80/20 levels.
        if (!isRegisteredPaneOutput) {
            dCtx.save();
            dCtx.beginPath();
            dCtx.rect(dssPadding.left, dssPadding.top, dssPlotW, dssPlotH);
            dCtx.clip();
            dCtx.fillStyle = 'rgba(242, 54, 69, 0.08)';
            dCtx.fillRect(dssPadding.left, getDssY(100), dssPlotW, getDssY(80) - getDssY(100));
            dCtx.fillStyle = 'rgba(41, 98, 255, 0.02)';
            dCtx.fillRect(dssPadding.left, getDssY(80), dssPlotW, getDssY(30) - getDssY(80));
            dCtx.fillStyle = 'rgba(8, 153, 129, 0.08)';
            dCtx.fillRect(dssPadding.left, getDssY(30), dssPlotW, getDssY(0) - getDssY(30));
            dCtx.restore();
        }

        const panePlots = registeredPaneOutput?.plots || [];
        const labeledLevels = isRegisteredPaneOutput
            ? panePlots.filter(plot => plot.kind === 'hline').map(plot => plot.values.find(Number.isFinite)).filter(Number.isFinite)
            : [80, 50, 30];
        dCtx.save();
        dCtx.strokeStyle = 'rgba(120, 123, 134, 0.16)';
        dCtx.lineWidth = 1;
        dCtx.font = '9px monospace';
        for (const tick of scaleTicks.ticks.filter(value => value >= scaleMin && value <= scaleMax)) {
            const y = getDssY(tick);
            dCtx.beginPath();
            dCtx.moveTo(dssPadding.left, y);
            dCtx.lineTo(dWidth - dssPadding.right, y);
            dCtx.stroke();
            dCtx.beginPath();
            dCtx.moveTo(dWidth - dssPadding.right, y);
            dCtx.lineTo(dWidth - dssPadding.right + 4, y);
            dCtx.stroke();
            const overlapsLevelLabel = labeledLevels.some(level => Math.abs(getDssY(level) - y) < 12);
            if (!overlapsLevelLabel) {
                const digits = scaleTicks.step > 0 ? Math.max(0, Math.min(3, Math.ceil(-Math.log10(scaleTicks.step)))) : 1;
                dCtx.fillStyle = '#9ca3af';
                dCtx.textAlign = 'right';
                dCtx.fillText(tick.toFixed(digits), dWidth - 4, y + 3);
            }
        }
        dCtx.restore();

        const dssLevels = isRegisteredPaneOutput
            ? []
            : [
                { level: 80, color: 'rgba(242, 54, 69, 0.65)', label: '80 OB', dash: [3, 3] },
                { level: 50, color: 'rgba(75, 85, 99, 0.45)', label: '50', dash: [2, 2] },
                { level: 30, color: 'rgba(8, 153, 129, 0.65)', label: '30 OS', dash: [3, 3] },
            ];

        dssLevels.forEach(lvl => {
            if (lvl.level < scaleMin || lvl.level > scaleMax) return;
            const y = getDssY(lvl.level);
            dCtx.strokeStyle = lvl.color;
            dCtx.lineWidth = 1;
            dCtx.setLineDash(lvl.dash);
            dCtx.beginPath();
            dCtx.moveTo(dssPadding.left, y);
            dCtx.lineTo(dWidth - dssPadding.right, y);
            dCtx.stroke();

            dCtx.fillStyle = lvl.color;
            dCtx.font = '9px monospace';
            dCtx.textAlign = 'left';
            dCtx.fillText(lvl.label, dWidth - dssPadding.right + 6, y + 3);
        });
        dCtx.setLineDash([]);

        const customDssPlot = panePlots.find(plot => plot.title === 'DSS');
        const customTriggerPlot = panePlots.find(plot => plot.title === 'Trigger');
        const maxAbovePlot = panePlots.find(plot => plot.title === 'Max above upper band (%)');
        const maxBelowPlot = panePlots.find(plot => plot.title === 'Max below lower band (%)');
        const valueAt = (plot, index) => {
            const value = plot?.values?.[index];
            return Number.isFinite(value) ? value : null;
        };
        const lastVisibleIndex = startIndex + candles.length - 1;
        const currentDss = isRegisteredDss
            ? valueAt(customDssPlot, lastVisibleIndex)
            : ((candles[candles.length - 1]?.dss !== undefined) ? candles[candles.length - 1].dss : 50.0);
        const currentTrigger = isRegisteredDss
            ? valueAt(customTriggerPlot, lastVisibleIndex)
            : ((candles[candles.length - 1]?.trigger !== undefined) ? candles[candles.length - 1].trigger : 50.0);

        // TradingView Right Axis Separator Line
        dCtx.strokeStyle = '#2a2e39';
        dCtx.lineWidth = 1;
        dCtx.beginPath();
        dCtx.moveTo(dWidth - dssPadding.right, 0);
        dCtx.lineTo(dWidth - dssPadding.right, dHeight);
        dCtx.stroke();

        // DSS Gradient Momentum Histogram & Turning Point Markers
        const dssGradZeroY = dHeight - 34;
        if (!isRegisteredPaneOutput) candles.forEach((c, i) => {
            const x = dssPadding.left + (i - barFraction) * stepX + stepX / 2;
            const prevC = i > 0 ? candles[i - 1] : c;
            const prev2C = i > 1 ? candles[i - 2] : prevC;
            const dssVal = c.dss !== undefined ? c.dss : 50;

            const grad = c.gradient !== undefined ? c.gradient : (dssVal - (prevC.dss !== undefined ? prevC.dss : 50));
            const prevGrad = prevC.gradient !== undefined ? prevC.gradient : ((prevC.dss !== undefined ? prevC.dss : 50) - (prev2C.dss !== undefined ? prev2C.dss : 50));

            // Mini gradient bar
            const gradBarH = Math.min(10, Math.max(1.5, Math.abs(grad) * 2.2));
            if (grad >= 0) {
                dCtx.fillStyle = '#089981'; // Pine green
                dCtx.fillRect(x - barW / 2, dssGradZeroY - gradBarH, barW, gradBarH);
            } else {
                dCtx.fillStyle = '#f23645'; // Coral red
                dCtx.fillRect(x - barW / 2, dssGradZeroY, barW, gradBarH);
            }

            // Turning Point Highlight Rings
            const isBullTurn = (prevGrad <= 0 && grad > 0 && dssVal <= 35);
            const isBearTurn = (prevGrad >= 0 && grad < 0 && dssVal >= 65);

            if (isBullTurn && dssVal >= scaleMin && dssVal <= scaleMax) {
                const y = getDssY(dssVal);
                dCtx.fillStyle = '#089981';
                dCtx.strokeStyle = '#34d399';
                dCtx.lineWidth = 2;
                dCtx.beginPath();
                dCtx.arc(x, y, 4.5, 0, 2 * Math.PI);
                dCtx.fill();
                dCtx.stroke();
            } else if (isBearTurn && dssVal >= scaleMin && dssVal <= scaleMax) {
                const y = getDssY(dssVal);
                dCtx.fillStyle = '#f23645';
                dCtx.strokeStyle = '#f87171';
                dCtx.lineWidth = 2;
                dCtx.beginPath();
                dCtx.arc(x, y, 4.5, 0, 2 * Math.PI);
                dCtx.fill();
                dCtx.stroke();
            }
        });

        const lastCandle = candles[candles.length - 1];
        if (isRegisteredPaneOutput) {
            for (const plot of panePlots) {
                if (plot.kind === 'hline') {
                    const level = plot.values.find(Number.isFinite);
                    if (!Number.isFinite(level) || level < scaleMin || level > scaleMax) continue;
                    const y = getDssY(level);
                    dCtx.save();
                    dCtx.strokeStyle = plot.color || '#2962ff';
                    dCtx.lineWidth = 1;
                    dCtx.setLineDash([]);
                    dCtx.beginPath();
                    dCtx.rect(dssPadding.left, dssPadding.top, dssPlotW, dssPlotH);
                    dCtx.clip();
                    dCtx.beginPath();
                    dCtx.moveTo(dssPadding.left, y);
                    dCtx.lineTo(dWidth - dssPadding.right, y);
                    dCtx.stroke();
                    dCtx.restore();
                    dCtx.fillStyle = plot.color || '#2962ff';
                    dCtx.font = '9px monospace';
                    dCtx.textAlign = 'left';
                    dCtx.fillText(`${plot.title} ${level}`, dWidth - dssPadding.right + 6, y + 3);
                    continue;
                }
                dCtx.save();
                dCtx.strokeStyle = plot.color || '#2962ff';
                dCtx.lineWidth = plot.title === 'DSS' ? 2.0 : plot.title === 'Trigger' ? 1.4 : 1.2;
                dCtx.setLineDash([]);
                dCtx.beginPath();
                dCtx.rect(dssPadding.left, dssPadding.top, dssPlotW, dssPlotH);
                dCtx.clip();
                dCtx.beginPath();
                let started = false;
                candles.forEach((_candle, index) => {
                    const value = valueAt(plot, startIndex + index);
                    if (!Number.isFinite(value)) {
                        started = false;
                        return;
                    }
                    const x = dssPadding.left + (index - barFraction) * stepX + stepX / 2;
                    const y = getDssY(value);
                    if (!started) {
                        dCtx.moveTo(x, y);
                        started = true;
                    } else {
                        dCtx.lineTo(x, y);
                    }
                });
                dCtx.stroke();
                dCtx.restore();
            }
        } else {
            // Native momentum trigger line (amber dashed).
            dCtx.save();
            dCtx.beginPath();
            dCtx.rect(dssPadding.left, dssPadding.top, dssPlotW, dssPlotH);
            dCtx.clip();
            dCtx.strokeStyle = '#f59e0b';
            dCtx.lineWidth = 1.3;
            dCtx.setLineDash([3, 2]);
            dCtx.beginPath();
            let trigStarted = false;
            candles.forEach((c, i) => {
                if (c.trigger !== undefined && !isNaN(c.trigger)) {
                    const x = dssPadding.left + (i - barFraction) * stepX + stepX / 2;
                    const y = getDssY(c.trigger);
                    if (!trigStarted) {
                        dCtx.moveTo(x, y);
                        trigStarted = true;
                    } else {
                        dCtx.lineTo(x, y);
                    }
                }
            });
            dCtx.stroke();
            dCtx.setLineDash([]);

            // Native DSS oscillator line (cyan).
            dCtx.strokeStyle = '#00bcd4';
            dCtx.lineWidth = 2.0;
            dCtx.beginPath();
            let dssStarted = false;
            candles.forEach((c, i) => {
                if (c.dss !== undefined && !isNaN(c.dss)) {
                    const x = dssPadding.left + (i - barFraction) * stepX + stepX / 2;
                    const y = getDssY(c.dss);
                    if (!dssStarted) {
                        dCtx.moveTo(x, y);
                        dssStarted = true;
                    } else {
                        dCtx.lineTo(x, y);
                    }
                }
            });
            dCtx.stroke();
            dCtx.restore();
        }

        const hasVisiblePaneValues = panePlots.some(plot => plot.kind !== 'hline'
            && plot.values.slice(startIndex, startIndex + candles.length).some(Number.isFinite));
        if (isNweOutsideDistance && !hasVisiblePaneValues) {
            dCtx.fillStyle = '#9ca3af';
            dCtx.font = '11px sans-serif';
            dCtx.textAlign = 'center';
            dCtx.fillText('No NWE band touch in visible history', dWidth / 2, dHeight / 2);
        }

        if (candles.length > 0 && isRegisteredPaneOutput) {
            for (const plot of panePlots.filter(item => item.kind !== 'hline')) {
                const value = valueAt(plot, lastVisibleIndex);
                if (!Number.isFinite(value) || value < scaleMin || value > scaleMax) continue;
                const lastX = dssPadding.left + (candles.length - 1 - barFraction) * stepX + stepX / 2;
                const lastY = getDssY(value);
                dCtx.fillStyle = plot.color || '#2962ff';
                dCtx.beginPath();
                dCtx.arc(lastX, lastY, 3.5, 0, 2 * Math.PI);
                dCtx.fill();
            }
        } else if (candles.length > 0) {
            const lastIdx = candles.length - 1;
            const lastX = dssPadding.left + (lastIdx - barFraction) * stepX + stepX / 2;
            if (Number.isFinite(currentDss) && currentDss >= scaleMin && currentDss <= scaleMax) {
                const lastY = getDssY(currentDss);
                dCtx.fillStyle = isRegisteredDss ? (customDssPlot.color || '#2962ff') : '#00bcd4';
                dCtx.beginPath();
                dCtx.arc(lastX, lastY, 3.5, 0, 2 * Math.PI);
                dCtx.fill();
            }
        }

        // TradingView crosshair on DSS Canvas; its time label is shown
        // by the main chart OHLC/cursor header instead.
        const crosshairIndex = chartCrosshair.active && chartCrosshair.candle
            ? allCandles.findIndex(candle => candle === chartCrosshair.candle
                || (candle.time != null && candle.time === chartCrosshair.candle.time)
                || (candle.date && candle.date === chartCrosshair.candle.date))
            : lastVisibleIndex;
        const paneReadoutIndex = crosshairIndex >= 0 ? crosshairIndex : lastVisibleIndex;
        if (chartCrosshair.active) {
            dCtx.save();
            dCtx.strokeStyle = 'rgba(120, 123, 134, 0.7)';
            dCtx.lineWidth = 1;
            dCtx.setLineDash([3, 3]);

            // Vertical Crosshair through timeline
            dCtx.beginPath();
            dCtx.moveTo(chartCrosshair.x, dssPadding.top);
            dCtx.lineTo(chartCrosshair.x, dHeight);
            dCtx.stroke();

            // Horizontal Crosshair on DSS Canvas (if hovering over DSS subchart)
            if (chartCrosshair.dssY !== null && chartCrosshair.dssY >= dssPadding.top && chartCrosshair.dssY <= dHeight - 22) {
                dCtx.beginPath();
                dCtx.moveTo(dssPadding.left, chartCrosshair.dssY);
                dCtx.lineTo(dWidth - dssPadding.right, chartCrosshair.dssY);
                dCtx.stroke();

                // Floating DSS Level Pill
                const tagH = 18;
                const tagW = isNweOutsideDistance ? 60 : 48;
                const tagX = dWidth - dssPadding.right + 2;
                const tagY = chartCrosshair.dssY - tagH / 2;
                dCtx.fillStyle = '#2a2e39';
                dCtx.strokeStyle = '#434651';
                dCtx.lineWidth = 1;
                dCtx.beginPath();
                if (dCtx.roundRect) {
                    dCtx.roundRect(tagX, tagY, tagW, tagH, 3);
                } else {
                    dCtx.rect(tagX, tagY, tagW, tagH);
                }
                dCtx.fill();
                dCtx.stroke();

                const crosshairDssValue = isRegisteredDss
                    ? valueAt(customDssPlot, paneReadoutIndex)
                    : chartCrosshair.dssVal;
                dCtx.fillStyle = isRegisteredPaneOutput
                    ? (customDssPlot?.color || maxAbovePlot?.color || maxBelowPlot?.color || '#2962ff')
                    : '#00bcd4';
                dCtx.font = 'bold 9px monospace';
                dCtx.textAlign = 'left';
                const crosshairLabel = Number.isFinite(crosshairDssValue)
                    ? `${crosshairDssValue.toFixed(1)}${isNweOutsideDistance ? '%' : ''}`
                    : '—';
                dCtx.fillText(crosshairLabel, tagX + 5, chartCrosshair.dssY + 3.5);
            }
            dCtx.restore();
        }

        // Update Dedicated Indicator Header & Status Badge with Gradient
        const targetC = (chartCrosshair.active && chartCrosshair.candle) ? chartCrosshair.candle : (lastCandle || {});
        const curDss = isRegisteredDss
            ? valueAt(customDssPlot, paneReadoutIndex)
            : (targetC.dss !== undefined ? targetC.dss : currentDss);
        const curTrig = isRegisteredDss
            ? valueAt(customTriggerPlot, paneReadoutIndex)
            : (targetC.trigger !== undefined ? targetC.trigger : currentTrigger);
        const priorDss = isRegisteredDss
            ? valueAt(customDssPlot, Math.max(startIndex, paneReadoutIndex - 1))
            : null;
        const curGrad = isRegisteredDss && Number.isFinite(curDss) && Number.isFinite(priorDss)
            ? curDss - priorDss
            : (targetC.gradient !== undefined ? targetC.gradient : 0.0);
        const curAccel = targetC.accel !== undefined ? targetC.accel : 0.0;

        const dssReadoutEl = document.getElementById('dssLiveReadout');
        const dssBadgeEl = document.getElementById('dssStatusBadge');
        if (dssReadoutEl) {
            if (isNweOutsideDistance) {
                const above = valueAt(maxAbovePlot, paneReadoutIndex);
                const below = valueAt(maxBelowPlot, paneReadoutIndex);
                const hasDistance = Number.isFinite(above) || Number.isFinite(below);
                const aboveText = Number.isFinite(above) ? `${above.toFixed(2)}%` : '—';
                const belowText = Number.isFinite(below) ? `${below.toFixed(2)}%` : '—';
                const valuesText = hasDistance ? `Above ${aboveText} · Below ${belowText}` : 'No band touch';
                dssReadoutEl.textContent = valuesText;
                dssReadoutEl.title = `${registeredDisplayName} · historical maximum wick distance · ${sourceContext}`;
                dssReadoutEl.setAttribute('aria-label', `${valuesText}. Historical excursion research, not a trading signal. ${sourceContext}`);
            } else if (isRegisteredDss) {
                const dssText = Number.isFinite(curDss) ? curDss.toFixed(1) : 'warming up';
                const triggerText = Number.isFinite(curTrig) ? curTrig.toFixed(1) : 'warming up';
                dssReadoutEl.textContent = `DSS ${dssText} · Trig ${triggerText}`;
                dssReadoutEl.title = `${registeredDisplayName} by ${registeredAuthor} · Browser WASM · ${sourceContext}`;
                dssReadoutEl.setAttribute('aria-label', `${registeredDisplayName} ${dssText}; Trigger ${triggerText}; Browser WASM; ${sourceContext}`);
            } else {
                const direction = curDss >= curTrig ? 'rising' : 'falling';
                dssReadoutEl.textContent = `${curDss.toFixed(1)} · ${direction}`;
                dssReadoutEl.title = `Momentum ${curDss.toFixed(1)} · signal ${curTrig.toFixed(1)}`;
                dssReadoutEl.setAttribute('aria-label', `Momentum ${curDss.toFixed(1)}, ${direction}`);
            }
        }
        if (dssBadgeEl) {
            if (isRegisteredPaneOutput) {
                dssBadgeEl.textContent = isNweOutsideDistance
                    ? 'Research metric · not a signal'
                    : `Pine WASM · ${String(sourceTimeframe || registeredPaneOutput.timeframe).toUpperCase()}`;
                dssBadgeEl.title = sourceContext;
                dssBadgeEl.className = 'dss-signal-status text-[10px] font-semibold px-2 py-0.5 rounded border bg-gray-900 text-gray-300 border-gray-700';
            } else {
            let signalLabel = 'Momentum falling';
            let signalStyle = 'bg-rose-950/70 text-rose-300 border-rose-800';
            if (curDss <= 30 && curGrad > 0) {
                signalLabel = 'Low · turning up';
                signalStyle = 'bg-emerald-950/70 text-emerald-300 border-emerald-800';
            } else if (curDss >= 75 && curGrad < 0) {
                signalLabel = 'High · turning down';
                signalStyle = 'bg-rose-950/70 text-rose-300 border-rose-800';
            } else if (curDss <= 30) {
                signalLabel = 'Low momentum';
                signalStyle = 'bg-amber-950/70 text-amber-300 border-amber-800';
            } else if (curDss >= 80) {
                signalLabel = 'High momentum';
                signalStyle = 'bg-amber-950/70 text-amber-300 border-amber-800';
            } else if (curDss >= curTrig) {
                signalLabel = 'Momentum rising';
                signalStyle = 'bg-emerald-950/70 text-emerald-300 border-emerald-800';
            }
            dssBadgeEl.textContent = signalLabel;
            dssBadgeEl.className = `dss-signal-status text-[10px] font-semibold px-2 py-0.5 rounded border ${signalStyle}`;
            }
        }
        const computeBadge = document.getElementById('dssIndicatorComputeStatus');
        if (isRegisteredPaneOutput && computeBadge) {
            const staleSource = registeredPaneOutput.requested_data_stale?.some(Boolean) || /(?:^| · )stale(?: ·|$)/i.test(sourceContext);
            const sourceStatus = staleSource ? 'Stored · stale' : 'Stored';
            computeBadge.textContent = sourceStatus;
            computeBadge.setAttribute('aria-label', `Registered Pine output uses ${staleSource ? 'stale ' : ''}historical candles: ${sourceContext}`);
            computeBadge.title = sourceContext;
            computeBadge.className = `dss-compute-status text-[10px] font-semibold px-2 py-0.5 rounded border ${staleSource ? 'bg-amber-950/70 text-amber-300 border-amber-800' : 'bg-gray-800 text-gray-300 border-gray-700'}`;
        }
    }
}
