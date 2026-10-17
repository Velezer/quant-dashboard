import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { readApiResponse } from './api.js';

/** Initializes the markov modal dashboard feature. */
export function installMarkovModal() {
            let activeMainTab = 'signals';
            // 1-CLICK IN-PAGE MARKOV MODAL & TICKER BACKTEST
            // ==========================================
            let currentMarkovModalSymbol = '';
            let currentMarkovModalTimeframe = '1d';
            let currentMarkovModalExchange = '';
            let currentMarkovModalCategory = '';
            let loadedMarkovIdentity = '';

            // Default jumpToMarkov now opens the in-page modal directly without navigating away!
            function jumpToMarkov(symbol, event) {
                openMarkovModal(symbol, event);
            }

            async function openMarkovModal(symbol, event, timeframe = '1d', exchangeId = '', assetCategoryId = '') {
                if (event) event.stopPropagation();
                if (!symbol) return;
                currentMarkovModalSymbol = symbol;
                currentMarkovModalTimeframe = timeframe;
                currentMarkovModalExchange = exchangeId;
                currentMarkovModalCategory = assetCategoryId;
                const dataStatus = document.getElementById('mdmDataStatus');
                const retryButton = document.getElementById('mdmRetryLoad');
                if (dataStatus) dataStatus.textContent = `Loading causal DSS Bressert + NWE regime model for ${timeframe.toUpperCase()}…`;
                if (retryButton) retryButton.classList.add('hidden');

                const modal = document.getElementById('markovDetailModal');
                if (!modal) return;
                modal.classList.remove('hidden');

                // Reset placeholders while loading
                document.getElementById('mdmModalSymbolBadge').innerText = symbol;
                document.getElementById('mdmModalClassBadge').innerText = '...';
                document.getElementById('mdmKpiRegime').innerText = 'Loading...';
                document.getElementById('mdmKpiDwell').innerText = '--';
                document.getElementById('mdmKpiExpDuration').innerText = '--';
                document.getElementById('mdmKpiPersistence').innerText = '--';
                document.getElementById('mdmKpiNextShift').innerText = 'Loading...';
                document.getElementById('mdmTransitionBars').innerHTML = '<div class="text-xs text-gray-500 py-3 text-center">Loading historical transition frequencies...</div>';
                document.getElementById('mdmMatrixTable').innerHTML = '<tr><td class="p-3 text-center text-xs text-gray-500">Loading matrix...</td></tr>';
                document.getElementById('mdmTimelineRibbon').innerHTML = '<div class="text-[10px] text-gray-500 w-full text-center">Loading timeline...</div>';

                const requestedIdentity = `${timeframe}|${symbol}|${exchangeId}|${assetCategoryId}`;
                if (!MARKOV_DATA || loadedMarkovIdentity !== requestedIdentity) {
                    try {
                        const query = new URLSearchParams({ timeframe, symbol });
                        if (exchangeId) query.set('exchange_id', exchangeId);
                        if (assetCategoryId) query.set('asset_category_id', assetCategoryId);
                        const res = await fetch(`/api/markov-regimes?${query.toString()}`);
                        if (!res.ok) throw new Error(`Markov model unavailable (HTTP ${res.status}).`);
                        if (res.ok) {
                            MARKOV_DATA = await readApiResponse(res);
                        }
                        if (MARKOV_DATA?.timeframe !== timeframe) throw new Error('Markov model response timeframe did not match the selected signal.');
                        loadedMarkovIdentity = requestedIdentity;
                        if (dataStatus) dataStatus.textContent = 'Causal completed-bar state labels · smoothed historical transition frequencies · no trade probabilities.';
                        if (retryButton) retryButton.classList.add('hidden');
                    } catch (e) {
                        console.error('Failed to load markov data for modal:', e);
                        MARKOV_DATA = null;
                        loadedMarkovIdentity = '';
                        if (dataStatus) dataStatus.textContent = 'Regime data could not be loaded for this timeframe. Retry the model request or select another signal.';
                        if (retryButton) retryButton.classList.remove('hidden');
                    }
                }

                renderMarkovModalData(symbol);
            }

            function openSelectedSignalMarkov() {
                const signal = getDashboardBinding('selectedSignal');
                if (!signal) return;
                openMarkovModal(signal.storage_symbol || signal.symbol, null, signal.timeframe || '1d', signal.exchange_id || '', signal.asset_category_id || '');
            }

            function retryMarkovLoad() {
                if (currentMarkovModalSymbol) {
                    MARKOV_DATA = null;
                    loadedMarkovIdentity = '';
                    openMarkovModal(currentMarkovModalSymbol, null, currentMarkovModalTimeframe, currentMarkovModalExchange, currentMarkovModalCategory);
                }
            }

            function closeMarkovModal() {
                const modal = document.getElementById('markovDetailModal');
                if (modal) modal.classList.add('hidden');
            }

            function onMarkovModalBackdropClick(event) {
                if (event.target && event.target.id === 'markovDetailModal') {
                    closeMarkovModal();
                }
            }

            function openBacktestFromMarkovModal() {
                const sym = currentMarkovModalSymbol;
                closeMarkovModal();
                // ticker backtest removed
            }

            function openMarkovModalFromBacktestModal() {
                const sym = currentModalSymbol;
                closeTickerBacktestModal();
                if (sym) openMarkovModal(sym);
            }

            function renderMarkovModalData(symbol) {
                if (!MARKOV_DATA) return;

                const regimes = MARKOV_DATA.regimes || [];
                const regimeMap = {};
                regimes.forEach(r => { regimeMap[r.id] = r; });

                const evaluationElement = document.getElementById('mdmWalkForwardEvaluation');
                const configElement = document.getElementById('mdmClassifierConfig');
                if (configElement) {
                    configElement.textContent = MARKOV_DATA.classification_config
                        ? JSON.stringify(MARKOV_DATA.classification_config, null, 2)
                        : 'Classifier settings unavailable.';
                }

                const clean = symbol.replace('.JK', '').replace('-USD', '').toUpperCase();
                let asset = MARKOV_DATA.assets?.find(a =>
                    (!currentMarkovModalExchange || a.exchange_id === currentMarkovModalExchange) &&
                    (!currentMarkovModalCategory || a.asset_category_id === currentMarkovModalCategory) &&
                    (a.symbol.toUpperCase() === symbol.toUpperCase())
                ) || ((!currentMarkovModalExchange && !currentMarkovModalCategory) ? MARKOV_DATA.assets?.find(a =>
                    a.symbol.toUpperCase() === symbol.toUpperCase() || 
                    a.clean_symbol.toUpperCase() === clean ||
                    a.symbol.toUpperCase().includes(clean)
                ) : undefined);

                // Fallback to the current screener snapshot if Markov has no row.
                if (!asset) {
                    const status = document.getElementById('mdmDataStatus');
                    if (status) status.textContent = `No classified Markov history is available for ${symbol} at ${MARKOV_DATA.timeframe}. The causal NWE endpoint requires a full warm-up window and additional transition observations.`;
                    document.getElementById('mdmModalClassBadge').innerText = 'INSUFFICIENT HISTORY';
                    document.getElementById('mdmKpiRegime').innerText = 'Unavailable';
                    document.getElementById('mdmKpiRegimeShare').innerText = 'No classified observations';
                    document.getElementById('mdmKpiDwell').innerText = '--';
                    document.getElementById('mdmKpiDwellStatus').innerText = 'No classified bars';
                    document.getElementById('mdmKpiExpDuration').innerText = '--';
                    document.getElementById('mdmKpiPersistence').innerText = '--';
                    document.getElementById('mdmKpiNextShift').innerText = 'Unavailable';
                    document.getElementById('mdmKpiNextProb').innerText = 'No observed transitions for this instrument and timeframe.';
                    document.getElementById('mdmTransitionBars').innerHTML = '<div class="text-xs text-gray-500 py-3 text-center">No classified transitions are available.</div>';
                    document.getElementById('mdmMatrixTable').innerHTML = '<tr><td class="p-3 text-center text-xs text-gray-500">No classified transition history is available.</td></tr>';
                    document.getElementById('mdmTimelineRibbon').innerHTML = '<div class="text-[10px] text-gray-500 w-full text-center">No classified bars are available.</div>';
                    if (evaluationElement) evaluationElement.innerText = 'No classified sequence is available for a walk-forward check.';
                    return;
                }

                // Use the ticker's own learned model; aggregate models are only for macro views.
                const tickerModel = MARKOV_DATA.ticker_models?.[asset.model_key];
                const uModel = MARKOV_DATA.universe_models?.[asset.class];
                const model = tickerModel || uModel || MARKOV_DATA.global_model;

                if (evaluationElement) {
                    const evaluation = tickerModel?.walk_forward_evaluation;
                    evaluationElement.innerText = evaluation?.status === 'evaluated'
                        ? `Expanding window · ${evaluation.evaluated_transitions} later transitions · model accuracy ${evaluation.accuracy_pct}% · stay-current baseline ${evaluation.stay_in_current_state_baseline_accuracy_pct}% · multiclass Brier ${evaluation.multiclass_brier_score}. ${evaluation.interpretation}`
                        : `Walk-forward status: ${evaluation?.status || 'unavailable'}. At least ${evaluation?.minimum_training_transitions ?? 30} prior transitions are required before evaluation.`;
                }
                const currR = regimeMap[asset.current_regime] || { id: asset.current_regime, short_name: asset.current_regime, name: asset.current_regime, color: '#9ca3af', icon: '●' };
                const nextR = regimeMap[asset.most_frequent_next_regime] || { id: asset.most_frequent_next_regime, short_name: asset.most_frequent_next_regime, name: asset.most_frequent_next_regime, color: '#9ca3af', icon: '➔' };

                // Update Header Badges
                document.getElementById('mdmModalSymbolBadge').innerText = asset.symbol;
                document.getElementById('mdmModalClassBadge').innerText = (asset.class || '').replace('_', ' ').toUpperCase();
            
                const iconBadge = document.getElementById('mdmModalIconBadge');
                if (iconBadge) {
                    iconBadge.innerText = currR.icon;
                    iconBadge.style.color = currR.color;
                }

                const statusBadge = document.getElementById('mdmModalStatusBadge');
                if (statusBadge) {
                    statusBadge.classList.remove('hidden');
                    if (asset.is_overdue) {
                        statusBadge.className = 'text-xs px-2.5 py-1 rounded font-bold font-mono bg-amber-950 text-amber-300 border border-amber-700';
                        statusBadge.innerText = 'LONGER THAN HISTORICAL AVERAGE';
                    } else {
                        statusBadge.className = 'text-xs px-2.5 py-1 rounded font-bold font-mono bg-emerald-950 text-emerald-300 border border-emerald-800';
                        statusBadge.innerText = 'WITHIN HISTORICAL AVERAGE';
                    }
                }

                const overdueBanner = document.getElementById('mdmOverdueBanner');
                if (overdueBanner) {
                    if (asset.is_overdue) {
                        overdueBanner.classList.remove('hidden');
                        document.getElementById('mdmOverdueDays').innerText = `${asset.dwell_bars} bars`;
                        document.getElementById('mdmOverdueAvg').innerText = `${asset.expected_duration_bars} bars (${MARKOV_DATA?.timeframe || '1d'})`;
                    } else {
                        overdueBanner.classList.add('hidden');
                    }
                }

                // Top KPIs
                const currIdx = regimes.findIndex(r => r.id === asset.current_regime);
                const statShare = (currIdx >= 0 && model?.stationary_distribution_pct?.[currIdx]) ? model.stationary_distribution_pct[currIdx] : '--';

                const kpiRegime = document.getElementById('mdmKpiRegime');
                kpiRegime.innerText = `${currR.icon} ${currR.short_name}`;
                kpiRegime.style.color = currR.color;
                document.getElementById('mdmKpiRegimeShare').innerText = `Stationary Share: ${statShare}%`;

                const kpiDwell = document.getElementById('mdmKpiDwell');
                kpiDwell.innerText = `${asset.dwell_bars} bars`;
                kpiDwell.className = `text-xl font-bold font-mono mt-1 ${asset.is_overdue ? 'text-amber-400' : 'text-white'}`;
                document.getElementById('mdmKpiDwellStatus').innerText = asset.is_overdue ? 'Above empirical mean' : 'Current dwell';

                document.getElementById('mdmKpiExpDuration').innerText = `${asset.expected_duration_bars} bars`;
                document.getElementById('mdmKpiPersistence').innerText = `${asset.stability_pct}%`;

                const kpiNext = document.getElementById('mdmKpiNextShift');
                kpiNext.innerText = asset.current_transition_support > 0
                    ? `${nextR.icon} ${nextR.short_name}`
                    : 'Insufficient observations';
                kpiNext.style.color = nextR.color;
                document.getElementById('mdmKpiNextProb').innerText = `Smoothed historical frequency: ${asset.next_transition_frequency_pct}% · n=${asset.current_transition_support ?? 0}`;
                const transitionSupport = document.getElementById('mdmKpiNextProb');
                if (transitionSupport && asset.current_transition_support === 0) {
                    transitionSupport.innerText = 'No observed transitions; prior-only estimate';
                }

                // Outgoing smoothed historical transition frequency bars.
                const forecastBadge = document.getElementById('mdmForecastOriginBadge');
                if (forecastBadge) forecastBadge.innerText = `From: ${currR.short_name} (${currR.icon})`;

                const matrix = model?.transition_matrix || [];
                const rowProbs = (currIdx >= 0 && matrix[currIdx]) ? matrix[currIdx] : [];

                const barsContainer = document.getElementById('mdmTransitionBars');
                if (barsContainer && rowProbs.length > 0) {
                    barsContainer.innerHTML = regimes.map((r, j) => {
                        const prob = rowProbs[j] ?? 0;
                        const isStay = (currIdx === j);
                        const isTopNext = (r.id === asset.most_frequent_next_regime);
                        const barColor = isStay ? '#2962ff' : (isTopNext ? '#f59e0b' : r.color);

                        return `
                            <div class="flex flex-col gap-1 p-2 rounded-lg bg-gray-900/60 border ${isStay ? 'border-blue-800/80 bg-blue-950/20' : (isTopNext ? 'border-amber-700/80 bg-amber-950/20' : 'border-gray-800/60')}">
                                <div class="flex items-center justify-between text-xs font-mono">
                                    <div class="flex items-center gap-1.5">
                                        <span style="color: ${r.color}">${r.icon}</span>
                                        <span class="font-bold text-gray-200">${r.short_name}</span>
                                        ${isStay ? '<span class="text-[9px] px-1.5 py-0.2 rounded bg-blue-900/80 text-blue-300 font-bold">STAY (P_ii)</span>' : ''}
                                        ${isTopNext ? '<span class="text-[9px] px-1.5 py-0.2 rounded bg-amber-900/80 text-amber-300 font-bold">MOST FREQUENT NEXT STATE</span>' : ''}
                                    </div>
                                    <span class="font-bold ${prob >= 50 ? 'text-emerald-400' : (prob >= 20 ? 'text-cyan-300' : 'text-gray-400')}">${prob.toFixed(1)}%</span>
                                </div>
                                <div class="w-full bg-gray-800 h-1.5 rounded-full overflow-hidden">
                                    <div class="h-full rounded-full transition-all duration-500" style="width: ${Math.min(prob, 100)}%; background-color: ${barColor}"></div>
                                </div>
                            </div>
                        `;
                    }).join('');
                }

                // Transition Matrix Table (Highlight Current Row)
                const matrixTable = document.getElementById('mdmMatrixTable');
                if (matrixTable && matrix.length > 0) {
                    let mHtml = `<thead><tr class="text-gray-400 border-b border-gray-800 text-[10px]"><th class="p-2 text-left">From \\ To</th>`;
                    regimes.forEach(r => {
                        mHtml += `<th class="p-2 text-center" style="color: ${r.color}">${r.icon} ${r.short_name.split(' ')[0]}</th>`;
                    });
                    mHtml += `</tr></thead><tbody class="divide-y divide-gray-800/60">`;

                    regimes.forEach((fromR, i) => {
                        const isCurrentRow = (i === currIdx);
                        const rowClass = isCurrentRow ? 'bg-blue-950/40 font-bold border-y border-blue-600/50' : '';
                        mHtml += `<tr class="${rowClass}"><td class="p-2 text-left" style="color: ${fromR.color}">${fromR.icon} ${fromR.short_name} ${isCurrentRow ? '<span class="text-[9px] text-blue-300 font-bold">◀ NOW</span>' : ''}</td>`;
                        regimes.forEach((toR, j) => {
                            const prob = matrix[i]?.[j] ?? 0;
                            const isDiag = (i === j);
                            let bgStyle = '';
                            if (prob >= 60) bgStyle = 'background-color: rgba(16, 185, 129, 0.30); color: #34d399; font-weight: bold;';
                            else if (prob >= 30) bgStyle = 'background-color: rgba(16, 185, 129, 0.18); color: #6ee7b7;';
                            else if (prob >= 10) bgStyle = 'background-color: rgba(6, 182, 212, 0.15); color: #67e8f9;';
                            else if (prob >= 3) bgStyle = 'color: #9ca3af;';
                            else bgStyle = 'color: #4b5563;';

                            if (isDiag) bgStyle += ' border: 1px solid rgba(255,255,255,0.15);';

                            mHtml += `<td class="p-2 font-mono text-[11px]" style="${bgStyle}">${prob.toFixed(1)}%</td>`;
                        });
                        mHtml += `</tr>`;
                    });
                    mHtml += `</tbody>`;
                    matrixTable.innerHTML = mHtml;
                }

                // 30-Day Timeline Ribbon
                const timelineRibbon = document.getElementById('mdmTimelineRibbon');
                const timeline = asset.recent_timeline || [asset.current_regime];
                if (timelineRibbon) {
                    timelineRibbon.innerHTML = timeline.map((rId, idx) => {
                        const rInfo = regimeMap[rId] || { color: '#6b7280', short_name: rId };
                        const isLast = (idx === timeline.length - 1);
                        return `<div class="flex-1 h-full rounded-sm transition hover:opacity-80 ${isLast ? 'border border-white' : ''}" style="background-color: ${rInfo.color}" title="${rInfo.short_name} (Bar ${idx + 1})"></div>`;
                    }).join('');
                }

                // Draw Directed Graph on Modal Canvas
                setTimeout(() => {
                    drawMarkovModalGraph(model, asset.current_regime);
                }, 60);
            }

            function drawMarkovModalGraph(model, currentRegimeId) {
                const canvas = document.getElementById('mdmGraphCanvas');
                if (!canvas || !MARKOV_DATA?.regimes || !model) return;
                const rect = canvas.getBoundingClientRect();
                if (rect.width <= 0 || rect.height <= 0) return;

                const dpr = Math.min(window.devicePixelRatio || 1, 2);
                const targetW = Math.floor(rect.width * dpr);
                const targetH = Math.floor(rect.height * dpr);

                if (canvas.width !== targetW || canvas.height !== targetH) {
                    canvas.width = targetW;
                    canvas.height = targetH;
                }

                const ctx = canvas.getContext('2d');
                ctx.resetTransform();
                ctx.scale(dpr, dpr);

                const w = rect.width;
                const h = rect.height;
                ctx.clearRect(0, 0, w, h);

                const regimes = MARKOV_DATA.regimes;
                const matrix = model.transition_matrix || [];
                const K = regimes.length;

                const cx = w / 2;
                const cy = h / 2;
                const radius = Math.min(w, h) * 0.36;

                const nodeCoords = regimes.map((r, i) => {
                    const angle = (i / K) * 2 * Math.PI - Math.PI / 2;
                    return {
                        id: r.id,
                        x: cx + radius * Math.cos(angle),
                        y: cy + radius * Math.sin(angle),
                        regime: r,
                        index: i
                    };
                });

                // 1. Draw Edges
                for (let i = 0; i < K; i++) {
                    for (let j = 0; j < K; j++) {
                        const prob = matrix[i]?.[j] ?? 0;
                        if (prob < 2.5) continue;

                        const fromNode = nodeCoords[i];
                        const toNode = nodeCoords[j];
                        const isFromActive = (fromNode.id === currentRegimeId);

                        if (i === j) {
                            // Self loop
                            const loopAngle = (i / K) * 2 * Math.PI - Math.PI / 2;
                            const loopRadius = 18;
                            const loopCenterX = fromNode.x + Math.cos(loopAngle) * (loopRadius + 14);
                            const loopCenterY = fromNode.y + Math.sin(loopAngle) * (loopRadius + 14);

                            ctx.beginPath();
                            ctx.arc(loopCenterX, loopCenterY, loopRadius, 0, 2 * Math.PI);
                            ctx.strokeStyle = isFromActive ? fromNode.regime.color : 'rgba(107, 114, 128, 0.35)';
                            ctx.lineWidth = isFromActive ? 2.5 : 1.2;
                            ctx.stroke();

                            ctx.fillStyle = isFromActive ? '#ffffff' : '#9ca3af';
                            ctx.font = 'bold 9px monospace';
                            ctx.textAlign = 'center';
                            ctx.fillText(`${prob.toFixed(0)}%`, loopCenterX, loopCenterY + 3);
                        } else {
                            // Directed Curved Edge
                            const dx = toNode.x - fromNode.x;
                            const dy = toNode.y - fromNode.y;
                            const dist = Math.sqrt(dx * dx + dy * dy);
                            const normX = dx / dist;
                            const normY = dy / dist;

                            const perpX = -normY * 14;
                            const perpY = normX * 14;

                            const startX = fromNode.x + normX * 20;
                            const startY = fromNode.y + normY * 20;
                            const endX = toNode.x - normX * 22;
                            const endY = toNode.y - normY * 22;
                            const midX = (startX + endX) / 2 + perpX;
                            const midY = (startY + endY) / 2 + perpY;

                            ctx.beginPath();
                            ctx.moveTo(startX, startY);
                            ctx.quadraticCurveTo(midX, midY, endX, endY);
                            ctx.strokeStyle = isFromActive ? fromNode.regime.color : 'rgba(75, 85, 99, 0.25)';
                            ctx.lineWidth = isFromActive ? (prob >= 15 ? 2.5 : 1.8) : 0.8;
                            ctx.stroke();

                            // Arrowhead
                            const angle = Math.atan2(endY - midY, endX - midX);
                            const arrowSize = isFromActive ? 6 : 4;
                            ctx.beginPath();
                            ctx.moveTo(endX, endY);
                            ctx.lineTo(endX - arrowSize * Math.cos(angle - Math.PI / 6), endY - arrowSize * Math.sin(angle - Math.PI / 6));
                            ctx.lineTo(endX - arrowSize * Math.cos(angle + Math.PI / 6), endY - arrowSize * Math.sin(angle + Math.PI / 6));
                            ctx.closePath();
                            ctx.fillStyle = ctx.strokeStyle;
                            ctx.fill();

                            if (isFromActive || prob >= 15) {
                                ctx.fillStyle = isFromActive ? '#ffffff' : '#9ca3af';
                                ctx.font = isFromActive ? 'bold 9px monospace' : '8px monospace';
                                ctx.textAlign = 'center';
                                ctx.fillText(`${prob.toFixed(0)}%`, midX, midY - 2);
                            }
                        }
                    }
                }

                // 2. Draw Nodes
                nodeCoords.forEach(node => {
                    const r = node.regime;
                    const isActive = (node.id === currentRegimeId);

                    ctx.beginPath();
                    ctx.arc(node.x, node.y, 20, 0, 2 * Math.PI);
                    ctx.fillStyle = isActive ? '#1f2937' : '#111827';
                    ctx.fill();
                    ctx.lineWidth = isActive ? 3 : 1.5;
                    ctx.strokeStyle = isActive ? r.color : 'rgba(107, 114, 128, 0.4)';
                    ctx.stroke();

                    if (isActive) {
                        ctx.beginPath();
                        ctx.arc(node.x, node.y, 24, 0, 2 * Math.PI);
                        ctx.strokeStyle = r.color;
                        ctx.lineWidth = 1.5;
                        ctx.setLineDash([3, 3]);
                        ctx.stroke();
                        ctx.setLineDash([]);
                    }

                    ctx.font = '12px sans-serif';
                    ctx.textAlign = 'center';
                    ctx.fillText(r.icon, node.x, node.y - 1);

                    ctx.fillStyle = isActive ? '#ffffff' : '#9ca3af';
                    ctx.font = 'bold 8px sans-serif';
                    ctx.fillText(r.short_name.split(' ')[0], node.x, node.y + 10);
                });
            }

            // Global Keyboard Shortcut: ESC closes any open modal
            document.addEventListener('keydown', function(e) {
                if (e.key === 'Escape' || e.keyCode === 27) {
                    closeMarkovModal();
                    closeTickerBacktestModal();
                    closeDaemonDiagnosticModal();
                }
            });

    function switchMainTab(_tab) {
        activeMainTab = 'signals';
        document.body.classList.add('signals-workspace-active');
        const viewScreener = document.getElementById('viewScreener');
        if (viewScreener) viewScreener.classList.remove('hidden');
        const signalsTab = document.getElementById('navTabSignals');
        if (signalsTab) signalsTab.setAttribute('aria-pressed', 'true');
        requestAnimationFrame(() => { renderSelectedSignalSummary(); });
        if (window.history && window.history.replaceState) {
            const url = new URL(window.location);
            url.searchParams.delete('tab');
            window.history.replaceState(null, '', url);
        }
    }

            // Shared escaping helper for evidence and signal annotations.

    publishDashboardBindings('markov-modal', {
        activeMainTab: { get: () => activeMainTab, set: value => { activeMainTab = value; } },
        currentMarkovModalSymbol: { get: () => currentMarkovModalSymbol, set: value => { currentMarkovModalSymbol = value; } },
        jumpToMarkov: { get: () => jumpToMarkov },
        openSelectedSignalMarkov: { get: () => openSelectedSignalMarkov },
        retryMarkovLoad: { get: () => retryMarkovLoad },
        openMarkovModal: { get: () => openMarkovModal },
        closeMarkovModal: { get: () => closeMarkovModal },
        onMarkovModalBackdropClick: { get: () => onMarkovModalBackdropClick },
        openBacktestFromMarkovModal: { get: () => openBacktestFromMarkovModal },
        openMarkovModalFromBacktestModal: { get: () => openMarkovModalFromBacktestModal },
        renderMarkovModalData: { get: () => renderMarkovModalData },
        drawMarkovModalGraph: { get: () => drawMarkovModalGraph },
        switchMainTab: { get: () => switchMainTab },
    });
}
