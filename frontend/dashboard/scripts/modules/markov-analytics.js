import { publishDashboardBindings } from './runtime-context.js';

/** Initializes the markov analytics dashboard feature. */
export function installMarkovAnalytics() {
            function researchEscape(value) {
                return String(value ?? '').replace(/[&<>"']/g, character => ({
                    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
                }[character]));
            }

            // =========================================================================
            // MARKOV CHAIN REGIME VISUALIZER RUNTIME SCRIPT (PER-TICKER & MACRO)
            // =========================================================================
            let MARKOV_DATA = /* MARKOV_DATA_PLACEHOLDER */ null;
            let currentMarkovUniverse = 'all';
            let selectedMarkovFocus = '__macro__';
            let markovSearchQuery = '';
            let markovRegimeFilter = 'all';
            let markovOverdueOnly = false;
            let selectedMarkovSymbol = 'BTC-USD';
            let highlightedMarkovNode = null;

            function markovBarLabel() {
                return MARKOV_DATA?.timeframe || '1d';
            }

            function getActiveMarkovModel() {
                if (!MARKOV_DATA) return null;
                if (selectedMarkovFocus && selectedMarkovFocus !== '__macro__' && MARKOV_DATA.ticker_models?.[selectedMarkovFocus]) {
                    return MARKOV_DATA.ticker_models[selectedMarkovFocus];
                }
                if (currentMarkovUniverse === 'all' || !MARKOV_DATA.universe_models?.[currentMarkovUniverse]) {
                    return MARKOV_DATA.global_model;
                }
                return MARKOV_DATA.universe_models[currentMarkovUniverse];
            }

            function updateMarkovModelBadge() {
                const badge = document.getElementById('markov-model-badge');
                const subtitle = document.getElementById('markov-graph-subtitle');
                const matrixSub = document.getElementById('markov-matrix-subtitle');
                if (!badge) return;

                if (selectedMarkovFocus && selectedMarkovFocus !== '__macro__' && MARKOV_DATA?.ticker_models?.[selectedMarkovFocus]) {
                    const tm = MARKOV_DATA.ticker_models[selectedMarkovFocus];
                    badge.className = 'text-[10px] font-mono font-bold px-2 py-0.5 rounded bg-emerald-900/60 text-emerald-300 border border-emerald-700/50';
                    badge.innerText = `TICKER: ${tm.clean_symbol} (${tm.total_bars} bars)`;
                    if (subtitle) subtitle.innerText = `| Specific to ${tm.symbol} historical transitions (${tm.total_bars} bars)`;
                    if (matrixSub) matrixSub.innerText = `| Smoothed empirical transition frequencies for ${tm.symbol}`;
                } else {
                    badge.className = 'text-[10px] font-mono font-bold px-2 py-0.5 rounded bg-blue-900/60 text-blue-300 border border-blue-700/50';
                    badge.innerText = currentMarkovUniverse === 'all' ? 'MACRO (GLOBAL)' : `MACRO (${currentMarkovUniverse.toUpperCase()})`;
                    if (subtitle) subtitle.innerText = `| Aggregate universe transition frequencies`;
                    if (matrixSub) matrixSub.innerText = `| Macro universe transition frequencies`;
                }
            }

            function initMarkovTickerOptions() {
                const sel = document.getElementById('markovTickerSelect');
                if (!sel || !MARKOV_DATA?.assets) return;

                const currVal = sel.value || selectedMarkovFocus;
                let html = `<option value="__macro__">🌐 Macro Aggregate (${currentMarkovUniverse === 'all' ? 'All Assets' : currentMarkovUniverse.toUpperCase()})</option>`;

                let filteredAssets = MARKOV_DATA.assets;
                if (currentMarkovUniverse !== 'all') {
                    filteredAssets = filteredAssets.filter(a => a.class === currentMarkovUniverse);
                }

                const sorted = [...filteredAssets].sort((a, b) => a.clean_symbol.localeCompare(b.clean_symbol));
                sorted.forEach(a => {
                    const uLabel = a.class === 'idx' ? 'IDX' : (a.class === 'crypto_futures' ? 'Futures' : (a.class === 'tokenized_stock' ? 'Tokenized stock' : 'Spot'));
                    html += `<option value="${a.model_key}">${a.clean_symbol} · ${uLabel} · ${a.exchange_id} - ${a.current_regime.split('_')[0]}</option>`;
                });

                sel.innerHTML = html;
                if (currVal && Array.from(sel.options).some(o => o.value === currVal)) {
                    sel.value = currVal;
                } else {
                    sel.value = '__macro__';
                    selectedMarkovFocus = '__macro__';
                }
                updateMarkovModelBadge();
            }

            function onMarkovTickerSelect(val) {
                selectedMarkovFocus = val;
                if (val !== '__macro__') {
                    selectedMarkovSymbol = val;
                    const asset = MARKOV_DATA.assets?.find(a => a.model_key === val);
                    if (asset) renderMarkovTimeline(asset);
                }
                updateMarkovModelBadge();
                renderMarkovKPIs();
                renderMarkovMatrix();
                drawMarkovGraph();
                renderMarkovRadar();
            }

            function setMarkovUniverse(u) {
                currentMarkovUniverse = u;
                document.querySelectorAll('.markov-u-btn').forEach(btn => {
                    btn.classList.remove('bg-blue-600', 'text-white');
                    btn.classList.add('bg-gray-800', 'text-gray-300');
                });
                const activeBtn = document.getElementById(`m-btn-${u}`);
                if (activeBtn) {
                    activeBtn.classList.remove('bg-gray-800', 'text-gray-300');
                    activeBtn.classList.add('bg-blue-600', 'text-white');
                }
                initMarkovTickerOptions();
                renderMarkovKPIs();
                renderMarkovMatrix();
                drawMarkovGraph();
                renderMarkovRadar();
            }

            function renderMarkovKPIs() {
                const grid = document.getElementById('markov-kpi-grid');
                if (!grid || !MARKOV_DATA?.regimes) return;
                const model = getActiveMarkovModel();
                if (!model) return;

                const regimes = MARKOV_DATA.regimes;
                grid.innerHTML = regimes.map((r, i) => {
                    const persist = model.persistence_pct?.[i] ?? 0;
                    const dwell = model.expected_duration_bars?.[i] ?? 0;
                    const statShare = model.stationary_distribution_pct?.[i] ?? 0;

                    return `
                        <div class="bg-gray-900 border border-gray-800 rounded-xl p-3 flex flex-col justify-between shadow transition hover:border-gray-700 cursor-pointer" onclick="highlightMarkovNodeFromKPI('${r.id}')">
                            <div class="flex items-center justify-between">
                                <span class="text-xs font-bold uppercase tracking-wider" style="color: ${r.color}">${r.icon} ${r.short_name}</span>
                                <span class="text-[10px] font-mono font-bold px-1.5 py-0.2 rounded bg-gray-800 text-gray-300">${statShare}% share</span>
                            </div>
                            <div class="mt-2 flex items-baseline justify-between">
                                <div class="text-lg font-bold font-mono text-white">${persist}%</div>
                                <span class="text-[10px] text-gray-400 font-mono">P_ii (stay)</span>
                            </div>
                            <div class="mt-1 pt-1.5 border-t border-gray-800/80 flex items-center justify-between text-[10px] text-gray-400">
                                <span>Mean duration:</span>
                                <span class="font-mono font-bold text-gray-200">${dwell} ${markovBarLabel()} bars</span>
                                <span class="text-gray-500">${model.transition_row_support?.[i] ?? 0} transitions</span>
                            </div>
                        </div>
                    `;
                }).join('');
            }

            function highlightMarkovNodeFromKPI(regimeId) {
                highlightedMarkovNode = (highlightedMarkovNode === regimeId) ? null : regimeId;
                drawMarkovGraph();
                markovRegimeFilter = highlightedMarkovNode ? highlightedMarkovNode : 'all';
                const sel = document.getElementById('markovRegimeFilterSelect');
                if (sel) sel.value = markovRegimeFilter;
                renderMarkovRadar();
            }

            function renderMarkovMatrix() {
                const table = document.getElementById('markovMatrixTable');
                if (!table || !MARKOV_DATA?.regimes) return;
                const model = getActiveMarkovModel();
                if (!model) return;

                const regimes = MARKOV_DATA.regimes;
                const matrix = model.transition_matrix || [];

                let html = `<thead><tr class="text-gray-400 border-b border-gray-800 text-[10px]"><th class="p-2 text-left">From \\ To</th>`;
                regimes.forEach(r => {
                    html += `<th class="p-2 text-center" style="color: ${r.color}">${r.icon} ${r.short_name.split(' ')[0]}</th>`;
                });
                html += `</tr></thead><tbody class="divide-y divide-gray-800/60">`;

                regimes.forEach((fromR, i) => {
                    html += `<tr><td class="p-2 text-left font-bold" style="color: ${fromR.color}">${fromR.icon} ${fromR.short_name}</td>`;
                    regimes.forEach((toR, j) => {
                        const prob = matrix[i]?.[j] ?? 0;
                        const isDiag = (i === j);
                        let bgStyle = '';
                        if (prob >= 60) bgStyle = 'background-color: rgba(16, 185, 129, 0.35); font-weight: bold; color: #34d399;';
                        else if (prob >= 30) bgStyle = 'background-color: rgba(16, 185, 129, 0.20); color: #6ee7b7;';
                        else if (prob >= 10) bgStyle = 'background-color: rgba(6, 182, 212, 0.15); color: #67e8f9;';
                        else if (prob >= 3) bgStyle = 'color: #9ca3af;';
                        else bgStyle = 'color: #4b5563;';

                        if (isDiag) bgStyle += ' border: 1px solid rgba(255,255,255,0.15);';

                        html += `<td class="p-2 font-mono text-[11px]" style="${bgStyle}">${prob.toFixed(1)}%</td>`;
                    });
                    html += `</tr>`;
                });
                html += `</tbody>`;
                table.innerHTML = html;
            }

            function drawMarkovGraph() {
                const canvas = document.getElementById('markovGraphCanvas');
                if (!canvas) return;
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

                const model = getActiveMarkovModel();
                if (!model || !MARKOV_DATA?.regimes) return;

                const regimes = MARKOV_DATA.regimes;
                const matrix = model.transition_matrix || [];
                const K = regimes.length;

                const cx = w / 2;
                const cy = h / 2;
                const radius = Math.min(w, h) * 0.38;

                // Compute Node Positions
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

                canvas._nodeCoords = nodeCoords;

                // 1. Draw Edges between nodes
                for (let i = 0; i < K; i++) {
                    for (let j = 0; j < K; j++) {
                        const prob = matrix[i]?.[j] ?? 0;
                        if (prob < 3.0) continue;

                        const fromNode = nodeCoords[i];
                        const toNode = nodeCoords[j];
                        const isHighlighted = highlightedMarkovNode === fromNode.id;
                        const isDimmed = highlightedMarkovNode && !isHighlighted;

                        if (i === j) {
                            // Draw Self-Loop (Persistence)
                            const loopAngle = (i / K) * 2 * Math.PI - Math.PI / 2;
                            const loopRadius = 24;
                            const loopCenterX = fromNode.x + Math.cos(loopAngle) * (loopRadius + 18);
                            const loopCenterY = fromNode.y + Math.sin(loopAngle) * (loopRadius + 18);

                            ctx.beginPath();
                            ctx.arc(loopCenterX, loopCenterY, loopRadius, 0, 2 * Math.PI);
                            ctx.strokeStyle = isDimmed ? 'rgba(75, 85, 99, 0.2)' : (isHighlighted ? fromNode.regime.color : 'rgba(156, 163, 175, 0.4)');
                            ctx.lineWidth = isHighlighted ? 2.5 : 1.5;
                            ctx.stroke();

                            ctx.fillStyle = isDimmed ? '#4b5563' : '#e5e7eb';
                            ctx.font = 'bold 9px monospace';
                            ctx.textAlign = 'center';
                            ctx.fillText(`${prob.toFixed(0)}%`, loopCenterX, loopCenterY + 3);
                        } else {
                            // Draw Directed Curved Edge
                            const dx = toNode.x - fromNode.x;
                            const dy = toNode.y - fromNode.y;
                            const dist = Math.sqrt(dx * dx + dy * dy);
                            const normX = dx / dist;
                            const normY = dy / dist;

                            const perpX = -normY * 18;
                            const perpY = normX * 18;

                            const startX = fromNode.x + normX * 24;
                            const startY = fromNode.y + normY * 24;
                            const endX = toNode.x - normX * 26;
                            const endY = toNode.y - normY * 26;
                            const midX = (startX + endX) / 2 + perpX;
                            const midY = (startY + endY) / 2 + perpY;

                            ctx.beginPath();
                            ctx.moveTo(startX, startY);
                            ctx.quadraticCurveTo(midX, midY, endX, endY);
                            ctx.strokeStyle = isDimmed ? 'rgba(75, 85, 99, 0.15)' : (isHighlighted ? fromNode.regime.color : 'rgba(107, 114, 128, 0.35)');
                            ctx.lineWidth = isHighlighted ? 2.2 : (prob > 20 ? 1.8 : 1.0);
                            ctx.stroke();

                            // Arrowhead
                            const angle = Math.atan2(endY - midY, endX - midX);
                            const arrowSize = 6;
                            ctx.beginPath();
                            ctx.moveTo(endX, endY);
                            ctx.lineTo(endX - arrowSize * Math.cos(angle - Math.PI / 6), endY - arrowSize * Math.sin(angle - Math.PI / 6));
                            ctx.lineTo(endX - arrowSize * Math.cos(angle + Math.PI / 6), endY - arrowSize * Math.sin(angle + Math.PI / 6));
                            ctx.closePath();
                            ctx.fillStyle = ctx.strokeStyle;
                            ctx.fill();

                            if (isHighlighted || prob >= 12) {
                                ctx.fillStyle = isDimmed ? '#4b5563' : (isHighlighted ? '#ffffff' : '#9ca3af');
                                ctx.font = isHighlighted ? 'bold 10px monospace' : '9px monospace';
                                ctx.textAlign = 'center';
                                ctx.fillText(`${prob.toFixed(0)}%`, midX, midY - 2);
                            }
                        }
                    }
                }

                // 2. Draw Nodes
                nodeCoords.forEach(node => {
                    const r = node.regime;
                    const isSelected = highlightedMarkovNode === node.id;
                    const isDimmed = highlightedMarkovNode && !isSelected;

                    ctx.beginPath();
                    ctx.arc(node.x, node.y, 24, 0, 2 * Math.PI);
                    ctx.fillStyle = isDimmed ? '#111827' : '#1f2937';
                    ctx.fill();
                    ctx.lineWidth = isSelected ? 3.5 : 2;
                    ctx.strokeStyle = isDimmed ? '#374151' : r.color;
                    ctx.stroke();

                    if (isSelected) {
                        ctx.beginPath();
                        ctx.arc(node.x, node.y, 28, 0, 2 * Math.PI);
                        ctx.strokeStyle = r.color;
                        ctx.lineWidth = 1;
                        ctx.setLineDash([3, 3]);
                        ctx.stroke();
                        ctx.setLineDash([]);
                    }

                    ctx.font = '14px sans-serif';
                    ctx.textAlign = 'center';
                    ctx.fillText(r.icon, node.x, node.y - 2);

                    ctx.fillStyle = isDimmed ? '#6b7280' : '#ffffff';
                    ctx.font = 'bold 9px sans-serif';
                    ctx.fillText(r.short_name.split(' ')[0], node.x, node.y + 11);
                });
            }

            function initMarkovGraphClickListener() {
                const canvas = document.getElementById('markovGraphCanvas');
                if (!canvas || canvas._listenerAdded) return;
                canvas._listenerAdded = true;

                canvas.addEventListener('click', (e) => {
                    const rect = canvas.getBoundingClientRect();
                    const clickX = e.clientX - rect.left;
                    const clickY = e.clientY - rect.top;

                    if (!canvas._nodeCoords) return;
                    const clicked = canvas._nodeCoords.find(n => {
                        const dx = n.x - clickX;
                        const dy = n.y - clickY;
                        return (dx * dx + dy * dy) <= 26 * 26;
                    });

                    if (clicked) {
                        highlightMarkovNodeFromKPI(clicked.id);
                    } else {
                        highlightedMarkovNode = null;
                        drawMarkovGraph();
                        markovRegimeFilter = 'all';
                        const sel = document.getElementById('markovRegimeFilterSelect');
                        if (sel) sel.value = 'all';
                        renderMarkovRadar();
                    }
                });
            }

            function renderMarkovRadar() {
                const tbody = document.getElementById('markovRadarTableBody');
                const countBadge = document.getElementById('markov-radar-count');
                if (!tbody || !MARKOV_DATA?.assets) return;

                let assets = MARKOV_DATA.assets;

                if (currentMarkovUniverse !== 'all') {
                    assets = assets.filter(a => a.class === currentMarkovUniverse);
                }
                if (markovRegimeFilter !== 'all') {
                    assets = assets.filter(a => a.current_regime === markovRegimeFilter);
                }
                if (markovOverdueOnly) {
                    assets = assets.filter(a => a.is_overdue);
                }
                if (markovSearchQuery) {
                    const q = markovSearchQuery.toLowerCase();
                    assets = assets.filter(a => a.symbol.toLowerCase().includes(q) || a.clean_symbol.toLowerCase().includes(q) || a.exchange_id.toLowerCase().includes(q) || a.asset_category_id.toLowerCase().includes(q));
                }

                if (countBadge) countBadge.innerText = `${assets.length} assets`;

                if (assets.length === 0) {
                    tbody.innerHTML = `<tr><td colspan="5" class="p-4 text-center text-xs text-gray-500">No assets match the current regime filter.</td></tr>`;
                    return;
                }

                const regimeMap = {};
                MARKOV_DATA.regimes.forEach(r => { regimeMap[r.id] = r; });

                tbody.innerHTML = assets.slice(0, 100).map(a => {
                    const r = regimeMap[a.current_regime] || { short_name: a.current_regime, color: '#9ca3af', icon: '●' };
                    const nextR = regimeMap[a.most_frequent_next_regime] || { short_name: a.most_frequent_next_regime, color: '#9ca3af' };
                    const isSelected = (a.model_key === selectedMarkovSymbol);
                    const rowBg = isSelected ? 'bg-blue-950/40 border-l-2 border-blue-500' : 'hover:bg-gray-800/40';

                    return `
                        <tr class="${rowBg} cursor-pointer transition" onclick="selectMarkovAsset('${researchEscape(a.model_key)}')">
                            <td class="py-2.5 px-3">
                                <div class="flex items-center gap-1.5 font-bold font-mono text-white text-xs">
                                    <span>${a.clean_symbol}</span>
                                    <span class="text-[9px] px-1 py-0.2 rounded bg-gray-800 text-gray-400 uppercase">${a.exchange_id} · ${a.asset_category_id}</span>
                                </div>
                                <div class="text-[10px] text-gray-400 font-mono mt-0.5">$${a.current_price}</div>
                            </td>
                            <td class="py-2.5 px-3">
                                <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-semibold border" style="background-color: ${r.bg_color || 'rgba(255,255,255,0.05)'}; color: ${r.color}; border-color: ${r.border_color || r.color}">
                                    ${r.icon} ${r.short_name}
                                </span>
                            </td>
                            <td class="py-2.5 px-3 text-center font-mono">
                                <span class="text-xs font-bold ${a.is_overdue ? 'text-amber-400' : 'text-gray-300'}">${a.dwell_bars} bars</span>
                                <span class="text-[10px] text-gray-500"> / ${a.expected_duration_bars} mean bars</span>
                                ${a.is_overdue ? '<span class="ml-1 text-[9px] px-1 py-0.2 rounded bg-amber-950 text-amber-300 border border-amber-800 font-bold">ABOVE HISTORICAL MEAN</span>' : ''}
                            </td>
                            <td class="py-2.5 px-3 text-center font-mono text-xs">
                                <div class="font-bold text-gray-200">${a.stability_pct}%</div>
                                <div class="w-12 bg-gray-800 rounded-full h-1 mx-auto mt-1 overflow-hidden">
                                    <div class="bg-emerald-400 h-1 rounded-full" style="width: ${a.stability_pct}%"></div>
                                </div>
                            </td>
                            <td class="py-2.5 px-3 text-right font-mono">
                                <div class="text-xs font-bold" style="color: ${nextR.color}">➔ ${a.current_transition_support > 0 ? nextR.short_name : 'Insufficient observations'}</div>
                                <div class="text-[10px] text-gray-400">${a.next_transition_frequency_pct}% smoothed · n=${a.current_transition_support ?? 0}</div>
                            </td>
                        </tr>
                    `;
                }).join('');

                const selectedItem = assets.find(a => a.model_key === selectedMarkovSymbol) || assets[0];
                if (selectedItem) {
                    renderMarkovTimeline(selectedItem);
                }
            }

            function selectMarkovAsset(modelKey) {
                const asset = MARKOV_DATA.assets?.find(a => a.model_key === modelKey);
                if (!asset) return;
                selectedMarkovSymbol = modelKey;
                selectedMarkovFocus = modelKey;
                const sel = document.getElementById('markovTickerSelect');
                if (sel) sel.value = modelKey;
                updateMarkovModelBadge();
                renderMarkovTimeline(asset);
                renderMarkovKPIs();
                renderMarkovMatrix();
                drawMarkovGraph();
                renderMarkovRadar();
            }

            function renderMarkovTimeline(asset) {
                const card = document.getElementById('markov-selected-asset-card');
                if (!card || !asset) return;

                const regimeMap = {};
                MARKOV_DATA.regimes.forEach(r => { regimeMap[r.id] = r; });
                const currR = regimeMap[asset.current_regime] || { short_name: asset.current_regime, color: '#9ca3af', icon: '●' };
                const nextR = regimeMap[asset.most_frequent_next_regime] || { short_name: asset.most_frequent_next_regime, color: '#9ca3af', icon: '➔' };

                const timeline = asset.recent_timeline || [];

                card.innerHTML = `
                    <div class="flex items-center justify-between">
                        <div class="flex items-center gap-2">
                            <span class="text-lg font-bold font-mono text-white">${asset.symbol}</span>
                            <span class="px-2 py-0.5 rounded text-xs font-semibold border" style="background-color: ${currR.bg_color}; color: ${currR.color}; border-color: ${currR.border_color}">
                                ${currR.icon} ${currR.name}
                            </span>
                        </div>
                        <div class="text-right font-mono">
                            <span class="text-base font-bold text-white">$${asset.current_price}</span>
                            <span class="text-xs text-gray-400 block">${asset.last_date}</span>
                        </div>
                    </div>

                    <div class="grid grid-cols-3 gap-2 pt-2 border-t border-gray-800 text-center font-mono">
                        <div class="bg-gray-800/70 p-2 rounded-lg">
                            <div class="text-[10px] text-gray-400 uppercase">Dwell Time</div>
                            <div class="text-sm font-bold ${asset.is_overdue ? 'text-amber-400' : 'text-white'}">${asset.dwell_bars} bars</div>
                        </div>
                        <div class="bg-gray-800/70 p-2 rounded-lg">
                            <div class="text-[10px] text-gray-400 uppercase">Empirical Mean Duration</div>
                            <div class="text-sm font-bold text-gray-200">${asset.expected_duration_bars} bars (${markovBarLabel()})</div>
                        </div>
                        <div class="bg-gray-800/70 p-2 rounded-lg">
                            <div class="text-[10px] text-gray-400 uppercase">Historical Persistence</div>
                            <div class="text-sm font-bold text-emerald-400">${asset.stability_pct}%</div>
                        </div>
                    </div>

                    <div class="bg-gray-800/90 border border-gray-700/80 rounded-lg p-3 flex items-center justify-between gap-2">
                        <div>
                            <div class="text-[10px] text-gray-400 uppercase tracking-wider font-semibold">Most Frequent Historical Next State</div>
                            <div class="text-sm font-bold mt-0.5" style="color: ${nextR.color}">${asset.current_transition_support > 0 ? `${nextR.icon} ${nextR.name} (${asset.next_transition_frequency_pct}% smoothed historical frequency)` : 'Insufficient observed transitions to estimate the next state.'}</div>
                            <div class="text-[10px] text-gray-400">${asset.current_transition_support > 0 ? `Observed transitions from this state: ${asset.current_transition_support}` : 'No observed transitions from this state; estimate is prior-only.'}</div>
                        </div>
                        ${asset.is_overdue ? '<span class="text-[10px] px-2 py-1 rounded bg-amber-950 text-amber-300 border border-amber-700 font-bold">ABOVE EMPIRICAL MEAN</span>' : '<span class="text-[10px] px-2 py-1 rounded bg-emerald-950 text-emerald-300 border border-emerald-800 font-bold">WITHIN EMPIRICAL MEAN</span>'}
                    </div>

                    <div class="pt-1">
                        <div class="flex items-center justify-between text-[10px] text-gray-400 mb-1 font-mono">
                            <span>30-Day Historical Regime Evolution</span>
                            <span>Latest ➔</span>
                        </div>
                        <div class="flex items-center gap-0.5 h-6 rounded-lg overflow-hidden bg-gray-950 p-0.5 border border-gray-800">
                            ${timeline.map(rId => {
                                const rInfo = regimeMap[rId] || { color: '#6b7280', short_name: rId };
                                return `<div class="flex-1 h-full rounded-sm transition hover:opacity-80" style="background-color: ${rInfo.color}" title="${rInfo.short_name}"></div>`;
                            }).join('')}
                        </div>
                    </div>
                `;
            }

            function onMarkovSearch(val) {
                markovSearchQuery = val;
                renderMarkovRadar();
            }

            function onMarkovRegimeFilter(regime) {
                markovRegimeFilter = regime;
                renderMarkovRadar();
            }

            function toggleMarkovOverdueFilter() {
                markovOverdueOnly = !markovOverdueOnly;
                const btn = document.getElementById('markovOverdueBtn');
                if (btn) {
                    if (markovOverdueOnly) {
                        btn.classList.remove('bg-gray-800', 'text-gray-300');
                        btn.classList.add('bg-amber-600', 'text-white');
                    } else {
                        btn.classList.remove('bg-amber-600', 'text-white');
                        btn.classList.add('bg-gray-800', 'text-gray-300');
                    }
                }
                renderMarkovRadar();
            }

            function renderAll() {
                if (!DATA) {
                    switchMainTab('signals');
                    return;
                }
                const signalCountBadge = document.getElementById('tabSignalsCountBadge');
                if (signalCountBadge) {
                    signalCountBadge.innerText = Array.isArray(DATA.signals) ? DATA.signals.length : 0;
                }
                renderKPIs();
                renderSignalsList();
                switchMainTab('signals');
            }

            // Read diagnostics for canonical SQLite market history.

    publishDashboardBindings('markov-analytics', {
        researchEscape: { get: () => researchEscape },
        MARKOV_DATA: { get: () => MARKOV_DATA, set: value => { MARKOV_DATA = value; } },
        currentMarkovUniverse: { get: () => currentMarkovUniverse, set: value => { currentMarkovUniverse = value; } },
        selectedMarkovFocus: { get: () => selectedMarkovFocus, set: value => { selectedMarkovFocus = value; } },
        markovSearchQuery: { get: () => markovSearchQuery, set: value => { markovSearchQuery = value; } },
        markovRegimeFilter: { get: () => markovRegimeFilter, set: value => { markovRegimeFilter = value; } },
        markovOverdueOnly: { get: () => markovOverdueOnly, set: value => { markovOverdueOnly = value; } },
        selectedMarkovSymbol: { get: () => selectedMarkovSymbol, set: value => { selectedMarkovSymbol = value; } },
        highlightedMarkovNode: { get: () => highlightedMarkovNode, set: value => { highlightedMarkovNode = value; } },
        getActiveMarkovModel: { get: () => getActiveMarkovModel },
        updateMarkovModelBadge: { get: () => updateMarkovModelBadge },
        initMarkovTickerOptions: { get: () => initMarkovTickerOptions },
        onMarkovTickerSelect: { get: () => onMarkovTickerSelect },
        setMarkovUniverse: { get: () => setMarkovUniverse },
        renderMarkovKPIs: { get: () => renderMarkovKPIs },
        highlightMarkovNodeFromKPI: { get: () => highlightMarkovNodeFromKPI },
        renderMarkovMatrix: { get: () => renderMarkovMatrix },
        drawMarkovGraph: { get: () => drawMarkovGraph },
        initMarkovGraphClickListener: { get: () => initMarkovGraphClickListener },
        renderMarkovRadar: { get: () => renderMarkovRadar },
        selectMarkovAsset: { get: () => selectMarkovAsset },
        renderMarkovTimeline: { get: () => renderMarkovTimeline },
        onMarkovSearch: { get: () => onMarkovSearch },
        onMarkovRegimeFilter: { get: () => onMarkovRegimeFilter },
        toggleMarkovOverdueFilter: { get: () => toggleMarkovOverdueFilter },
        renderAll: { get: () => renderAll },
    });
}
