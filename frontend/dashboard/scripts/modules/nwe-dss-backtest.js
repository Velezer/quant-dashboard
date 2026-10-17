import { requestAllInstruments, requestJson } from './api.js';

const SUPPORTED_APPS = new Set(['stockbit', 'pintu', 'pintu_futures', 'tokocrypto']);
const SUPPORTED_CATEGORIES = new Set(['idx_stock', 'crypto_perpetual', 'crypto_spot', 'tokenized_stock']);
const REQUEST_SYMBOL_LIMIT = 25;

function cell(text, classes = '') {
    const element = document.createElement('td');
    element.className = `border-t border-gray-800 px-2 py-1.5 ${classes}`;
    element.textContent = text;
    return element;
}

function displayNumber(value, suffix = '') {
    const number = value == null ? NaN : Number(value);
    return Number.isFinite(number) ? `${number.toFixed(2)}${suffix}` : '—';
}

export function displayProfitFactor(value, unbounded = false) {
    return unbounded === true ? '∞' : displayNumber(value);
}

export function nonEligibleResearchRow(instrument, exchange, category) {
    if (instrument?.research_eligibility === 'eligible' || instrument?.research_eligibility == null) return null;
    const excluded = instrument.research_eligibility === 'excluded';
    const status = excluded ? 'OUT_OF_SCOPE' : 'PENDING_VERIFICATION';
    return {
        symbol: instrument.storage_symbol || instrument.symbol,
        displaySymbol: instrument.symbol || instrument.storage_symbol,
        exchange,
        asset_category_id: category,
        freshness: excluded ? 'NOT_APPLICABLE' : 'PENDING',
        source_label: excluded ? 'Excluded from current research universe' : 'Venue verification pending',
        error: excluded
            ? 'This registered ticker is excluded from the research universe.'
            : 'Research is waiting for venue verification; no market-history request was made.',
        backtest: { status, rule_candidate: null },
    };
}

function renderRows(body, rows) {
    if (!body) return;
    if (!rows.length) {
        const row = document.createElement('tr');
        const empty = cell('No registered tickers matched this app/category scope.', 'text-center text-gray-500');
        empty.colSpan = 8;
        row.append(empty);
        body.replaceChildren(row);
        return;
    }
    const fragment = document.createDocumentFragment();
    const ordered = [...rows].sort((left, right) => String(left.symbol).localeCompare(String(right.symbol)));
    for (const item of ordered) {
        const holdout = item.backtest?.holdout || {};
        const status = item.backtest?.status || 'UNAVAILABLE';
        const rule = item.backtest?.rule_candidate || 'No selected rule · insufficient validation evidence';
        const basis = holdout.return_basis || 'gross · fees unknown';
        const row = document.createElement('tr');
        row.append(
            cell(`${item.displaySymbol || item.symbol} · ${item.exchange || 'app'} / ${item.asset_category_id || ''}`, 'whitespace-nowrap font-semibold text-gray-200'),
            cell(item.backtest?.selected_exit_bars == null ? '—' : `${item.backtest.selected_exit_bars} bars`, 'whitespace-nowrap font-mono text-cyan-200'),
            cell(`${holdout.trades ?? 0} · ${item.backtest?.holdout_window_days ?? 0}d window`, 'whitespace-nowrap font-mono text-gray-300'),
            cell(displayNumber(holdout.win_rate_pct, '%'), 'whitespace-nowrap font-mono text-gray-300'),
            cell(`${displayNumber(holdout.mean_return_pct, '%')} · ${basis}`, 'whitespace-nowrap font-mono text-gray-300'),
            cell(displayProfitFactor(holdout.profit_factor, holdout.profit_factor_unbounded), 'whitespace-nowrap font-mono text-gray-300'),
            cell(`${rule} · ${status}`, 'min-w-[260px] text-gray-300'),
            cell(`${item.source_label || item.source || 'Unavailable'} · ${item.history_bars ?? 0} bars · ${item.freshness || 'UNKNOWN'}`, 'min-w-[150px] text-gray-400'),
        );
        row.title = item.error || item.backtest?.validation_note || item.execution_scope || '';
        fragment.append(row);
    }
    body.replaceChildren(fragment);
}

function instrumentInScope(instrument) {
    const activeApp = String(globalThis.activeExchangeFilter || 'all').toLowerCase();
    const activeProduct = String(globalThis.activeProductFilter || 'all').toLowerCase();
    const app = String(instrument.exchange_id || '').toLowerCase();
    const appMatches = activeApp === 'all' || activeApp === app
        || (app === 'pintu' && ['pintu_futures', 'pintu_spot'].includes(activeApp));
    const productMatches = activeProduct === 'all'
        || activeProduct === String(instrument.product_id || '').toLowerCase();
    return appMatches && productMatches && SUPPORTED_APPS.has(app)
        && SUPPORTED_CATEGORIES.has(String(instrument.asset_category_id || '').toLowerCase());
}

function groupedInstruments(instruments) {
    const requestedSymbols = new Set(
        String(document.getElementById('nweDssBacktestSymbols')?.value || '')
            .split(',').map(value => value.trim().toUpperCase()).filter(Boolean),
    );
    const groups = new Map();
    for (const instrument of instruments.filter(instrumentInScope)) {
        if (requestedSymbols.size) {
            const identities = [instrument.symbol, instrument.storage_symbol, ...(instrument.aliases || [])]
                .map(value => String(value || '').toUpperCase());
            if (![...requestedSymbols].some(symbol => identities.includes(symbol))) continue;
        }
        const exchange = String(instrument.exchange_id || '').toLowerCase();
        const category = String(instrument.asset_category_id || '').toLowerCase();
        const key = `${exchange}\u0000${category}`;
        if (!groups.has(key)) groups.set(key, { exchange, category, instruments: [] });
        groups.get(key).instruments.push(instrument);
    }
    return [...groups.values()];
}

export function installNweDssBacktestReport() {
    const status = document.getElementById('nweDssBacktestStatus');
    const body = document.getElementById('nweDssBacktestRows');
    const timeframeSelect = document.getElementById('nweDssBacktestTimeframe');
    const symbolsInput = document.getElementById('nweDssBacktestSymbols');
    const runButton = document.getElementById('runNweDssBacktest');
    const cancelButton = document.getElementById('cancelNweDssBacktest');
    let runId = 0;
    let cancelled = false;
    let controller;

    async function runReport() {
        if (!status || !body || !runButton) return;
        const currentRun = ++runId;
        cancelled = false;
        controller = new AbortController();
        runButton.disabled = true;
        if (timeframeSelect) timeframeSelect.disabled = true;
        if (cancelButton) {
            cancelButton.disabled = false;
            cancelButton.classList.remove('hidden');
        }
        body.replaceChildren();
        status.textContent = 'Loading registered tickers for the selected app/category scope…';
        const rows = [];
        try {
            const registry = await requestAllInstruments({
                signal: controller.signal,
                onPage: ({ loaded, total }) => {
                    if (currentRun === runId) {
                        status.textContent = `Loading registered tickers… ${loaded} of ${total}.`;
                    }
                },
            });
            const groups = groupedInstruments(Array.isArray(registry?.instruments) ? registry.instruments : []);
            const totalSymbols = groups.reduce((sum, group) => sum + group.instruments.length, 0);
            const pendingRows = groups.flatMap(group => group.instruments
                .map(instrument => nonEligibleResearchRow(instrument, group.exchange, group.category))
                .filter(Boolean));
            rows.push(...pendingRows);
            if (!totalSymbols) {
                status.textContent = 'No active registered NWE/DSS tickers match this app/category scope. Choose another app or product and retry.';
                if (symbolsInput?.value.trim()) status.textContent = 'No registered tickers in the active app/category scope matched that ticker filter. Check the symbol and retry.';
                renderRows(body, []);
                return;
            }
            const eligibleSymbols = totalSymbols - pendingRows.length;
            if (!eligibleSymbols) {
                renderRows(body, rows);
                const pendingCount = pendingRows.filter(row => row.backtest.status === 'PENDING_VERIFICATION').length;
                const excludedCount = pendingRows.length - pendingCount;
                status.textContent = `Research not run · ${pendingCount} registered ticker${pendingCount === 1 ? '' : 's'} pending venue verification; ${excludedCount} excluded from the research universe.`;
                return;
            }
            const timeframe = String(timeframeSelect?.value || '4h').toLowerCase();
            const eligibleGroups = groups.map(group => ({
                ...group,
                instruments: group.instruments.filter(instrument =>
                    !nonEligibleResearchRow(instrument, group.exchange, group.category)),
            }));
            const totalRequests = eligibleGroups.reduce((sum, group) => sum + Math.ceil(group.instruments.length / REQUEST_SYMBOL_LIMIT), 0);
            let completedRequests = 0;
            for (const group of eligibleGroups) {
                for (let offset = 0; offset < group.instruments.length; offset += REQUEST_SYMBOL_LIMIT) {
                    if (cancelled || currentRun !== runId) break;
                    const chunk = group.instruments.slice(offset, offset + REQUEST_SYMBOL_LIMIT);
                    const byStorageSymbol = new Map(chunk.map(item => [String(item.storage_symbol || item.symbol || ''), item]));
                    const query = new URLSearchParams({
                        exchange: group.exchange,
                        asset_class: group.category,
                        timeframe,
                        symbols: [...byStorageSymbol.keys()].filter(Boolean).join(','),
                    });
                    try {
                        const result = await requestJson(`/api/research/nwe-dss?${query}`, { signal: controller.signal });
                        for (const item of result?.rows || []) {
                            const instrument = byStorageSymbol.get(String(item.symbol || ''));
                            rows.push({ ...item, displaySymbol: instrument?.symbol || item.symbol });
                        }
                    } catch (error) {
                        if (cancelled || currentRun !== runId) break;
                        for (const instrument of chunk) rows.push({
                            symbol: instrument.storage_symbol || instrument.symbol,
                            displaySymbol: instrument.symbol,
                            exchange: group.exchange,
                            asset_category_id: group.category,
                            freshness: 'UNAVAILABLE',
                            error: String(error?.message || error),
                            backtest: { status: 'UNAVAILABLE' },
                        });
                    }
                    completedRequests += 1;
                    renderRows(body, rows);
                    status.textContent = `Research progress · ${completedRequests} of ${totalRequests} requests · ${eligibleSymbols} eligible tickers · ${pendingRows.length} pending or excluded · ${timeframe.toUpperCase()}${cancelled ? ' · cancelling…' : ''}.`;
                if (cancelled) break;
                }
                if (cancelled || currentRun !== runId) break;
            }
            if (cancelled || currentRun !== runId) {
                status.textContent = `Research cancelled after ${completedRequests} of ${totalRequests} requests; ${pendingRows.length} scoped tickers remain pending or excluded. Run again to retry the eligible scope.`;
            } else {
                const validated = rows.filter(item => item.backtest?.status === 'VALIDATED_CANDIDATE').length;
                status.textContent = `Research complete · ${eligibleSymbols} eligible tickers examined · ${pendingRows.length} pending or excluded · ${timeframe.toUpperCase()} · ${validated} validated candidates. Pending, proxy, unknown-cost, and insufficient results remain unvalidated.`;
            }
        } catch (error) {
            status.textContent = cancelled
                ? `Research cancelled after ${rows.length} ticker${rows.length === 1 ? '' : 's'}. Run again to retry.`
                : `Research unavailable: ${error.message}. Retry after the registry or local market-history store recovers.`;
        } finally {
            if (currentRun === runId) {
                runButton.disabled = false;
                if (timeframeSelect) timeframeSelect.disabled = false;
                if (cancelButton) {
                    cancelButton.disabled = true;
                    cancelButton.classList.add('hidden');
                }
                controller = null;
            }
        }
    }

    runButton?.addEventListener('click', () => { void runReport(); });
    cancelButton?.addEventListener('click', () => {
        cancelled = true;
        controller?.abort();
        if (status) status.textContent = 'Cancelling after the current ticker batch…';
    });
}
