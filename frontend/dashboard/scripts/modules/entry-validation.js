import { requestJson } from './api.js';

const PAGE_SIZE = 50;
const APP_LABELS = { stockbit: 'Stockbit', pintu: 'Pintu', pluang: 'Pluang', tokocrypto: 'Tokocrypto' };

function numeric(value) {
    if (!['number', 'string'].includes(typeof value) || (typeof value === 'string' && !value.trim())) return null;
    const number = Number(value);
    return Number.isFinite(number) ? number : null;
}

export function metric(value, suffix = '', digits = 2) {
    const number = numeric(value);
    return number === null ? '—' : `${number.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits })}${suffix}`;
}

export function reportModelState(model, currentProtocol, reportProtocol) {
    if (reportProtocol && currentProtocol && reportProtocol !== currentProtocol) return { label: 'Outdated protocol', tone: 'pending' };
    if (model?.entry_validation_status === 'VALIDATED_HOLDOUT') return { label: 'Validated holdout evidence', tone: 'valid' };
    const status = model?.backtest?.status || model?.status;
    const labels = {
        INVALID_HISTORY: 'Invalid history', INSUFFICIENT_HISTORY: 'Insufficient history',
        NO_VALIDATION_TRADES: 'Insufficient validation trades', HOLDOUT_RESEARCH_COMPLETE: 'Research complete · not validated',
        NO_HISTORY: 'No stored history', UNSUPPORTED_MODEL: 'Entry model not configured',
    };
    return { label: labels[status] || 'Validation pending', tone: 'pending' };
}

export function normalizeList(payload) {
    if (!payload || !Array.isArray(payload.rows) || numeric(payload.total) === null || payload.total < 0) {
        throw new Error('The service returned an incomplete ticker list. Refresh the list to retry.');
    }
    if (payload.rows.some(row => !row || typeof row.instrument_id !== 'string' || !row.instrument_id.trim())) throw new Error('The service returned a ticker without its registered identity.');
    if (payload.has_more && (!payload.rows.length || !Number.isSafeInteger(payload.next_offset) || payload.next_offset <= Number(payload.offset || 0))) {
        throw new Error('Ticker pagination did not advance. No partial universe was treated as complete.');
    }
    return payload;
}

export async function collectScope(fetchPage, { onPage = () => {}, shouldStop = () => false } = {}) {
    const eligible = [];
    const seen = new Set();
    let skipped = 0;
    let offset = 0;
    let loaded = 0;
    while (!shouldStop()) {
        const page = normalizeList(await fetchPage(offset));
        if (page.has_more && page.next_offset <= offset) throw new Error('Ticker pagination repeated a page. Refresh the list to retry the complete scope.');
        for (const row of page.rows) {
            if (!row?.instrument_id || seen.has(row.instrument_id)) continue;
            seen.add(row.instrument_id);
            loaded++;
            if (row.research_eligibility === 'eligible') eligible.push({ instrument_id: row.instrument_id, symbol: row.symbol || row.storage_symbol });
            else skipped++;
        }
        onPage({ loaded, total: page.total, eligible: eligible.length, skipped });
        if (!page.has_more) return { eligible, skipped, stopped: false };
        offset = page.next_offset;
    }
    return { eligible, skipped, stopped: true };
}

export async function processQueue(rows, { run, shouldStop = () => false, onProgress = () => {}, onResult = () => {} }) {
    const failures = [];
    let processed = 0;
    for (const row of rows) {
        if (shouldStop()) break;
        onProgress({ processed, total: rows.length, failed: failures.length, current: row });
        try {
            const report = await run(row);
            onResult(row, report);
        } catch (error) {
            failures.push({ ...row, error: String(error?.message || error).slice(0, 500) });
        }
        processed++;
        onProgress({ processed, total: rows.length, failed: failures.length, current: null });
    }
    return { processed, failures, stopped: processed < rows.length };
}

function human(value) { return String(value || '').replaceAll('_', ' ').toLowerCase(); }
function sourceName(source) { return typeof source === 'string' ? source : source?.id || source?.source || 'Source unavailable'; }
function dateText(timestamp, detailed = false) {
    const value = numeric(timestamp);
    if (value === null || value <= 0) return 'Unavailable';
    const ms = value > 1e15 ? value / 1e6 : value > 1e12 ? value : value * 1000;
    const date = new Date(ms);
    if (!Number.isFinite(date.getTime())) return 'Unavailable';
    return new Intl.DateTimeFormat('en-GB', { timeZone: 'Asia/Jakarta', year: 'numeric', month: 'short', day: '2-digit', ...(detailed ? { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' } : {}) }).format(date) + (detailed ? ' WIB' : '');
}

export function initializeEntryValidation(doc = document) {
    const $ = id => doc.getElementById(id);
    if (!$('scope-form')) return;
    const scopeControls = ['validation-app', 'validation-product', 'validation-timeframe', 'validation-search', 'apply-scope'];
    let scope = { exchange: 'stockbit', product: 'all', timeframe: '15m', search: '' };
    let list = null;
    let offset = 0;
    let selected = null;
    let report = null;
    let modelKind = 'nwe';
    let listSerial = 0;
    let reportSerial = 0;
    let busy = false;
    let stopping = false;
    let failures = [];
    let protocol = '';

    const element = (tag, text, className = '') => {
        const node = doc.createElement(tag);
        if (text != null) node.textContent = String(text);
        if (className) node.className = className;
        return node;
    };
    const text = (id, value) => { if ($(id)) $(id).textContent = String(value ?? '—'); };
    const status = (id, message, tone = '') => { text(id, message); if ($(id)) $(id).dataset.tone = tone; };
    function query(snapshot = scope, pageOffset = offset) {
        return new URLSearchParams({ ...snapshot, offset: String(pageOffset), limit: String(PAGE_SIZE) });
    }
    async function request(url, options = {}) {
        const timeout = globalThis.AbortSignal?.timeout?.(options.method === 'POST' ? 120_000 : 30_000);
        try { return await requestJson(url, { ...options, ...(timeout ? { signal: timeout } : {}) }); }
        catch (error) {
            if (error?.name === 'TimeoutError' || error?.name === 'AbortError') throw new Error('The request timed out. The current ticker may still finish on the server; refresh saved evidence before retrying.');
            throw error;
        }
    }
    function setBusy(value) {
        busy = value;
        for (const id of scopeControls) $(id).disabled = value;
        $('reload-list').disabled = value;
        $('run-scope').disabled = value || !list || list.total === 0;
        $('stop-run').hidden = !value;
        $('stop-run').disabled = stopping;
        $('retry-failed').hidden = failures.length === 0;
        $('retry-failed').disabled = value;
        $('run-selected').disabled = value || selected?.research_eligibility !== 'eligible';
        $('previous-page').disabled = value || !list || offset === 0;
        $('next-page').disabled = value || !list?.has_more;
    }
    function resetSelection() {
        selected = null; report = null; reportSerial++;
        text('report-title', 'Select a ticker');
        text('report-identity', 'Review exact model versions, source, held-out outcomes, and validation blockers.');
        status('report-status', 'Choose Review on a ticker above. Saved reports are snapshots; rerun to refresh the evidence.');
        $('report-content').hidden = true; $('retry-report').hidden = true;
        setBusy(busy);
    }
    function miniCell(models, formatter) {
        const cell = element('td');
        if (!models.length) cell.append(element('span', '—', 'muted'));
        models.forEach(model => cell.append(element('span', `${model.kind === 'dss' ? 'DSS' : 'NWE'} · ${formatter(model)}`, 'mini-model')));
        return cell;
    }
    function renderTickerRows() {
        const body = $('ticker-rows'); body.replaceChildren();
        const rows = list?.rows || [];
        if (!rows.length) {
            const emptyMessage = !list
                ? $('list-status').dataset.tone === 'error' ? 'Ticker registry unavailable. Use Refresh list to retry.' : 'Loading the scoped ticker registry…'
                : 'No registered tickers match this scope. Choose another app or product, clear the search, or use app readiness on the screener.';
            const row = element('tr'); const cell = element('td', emptyMessage, 'empty-cell');
            cell.colSpan = 6; row.append(cell); body.append(row); return;
        }
        for (const item of rows) {
            const row = element('tr');
            if (selected?.instrument_id === item.instrument_id) row.className = 'is-selected';
            const identity = element('td');
            identity.append(element('strong', item.symbol || item.storage_symbol, 'ticker-name'), element('span', `${APP_LABELS[item.exchange_id] || item.exchange_id} · ${human(item.product_id)}`, 'ticker-context'));
            const evidence = item.evidence;
            const models = Array.isArray(evidence?.models) ? evidence.models : [];
            const stateCell = element('td');
            if (item.research_eligibility !== 'eligible') stateCell.append(element('span', human(item.research_eligibility) || 'Verification unavailable', 'state-badge pending'));
            else if (!evidence) stateCell.append(element('span', 'Not backtested', 'state-badge'));
            else if (!models.length) stateCell.append(element('span', human(evidence.status) || 'No model evidence', 'state-badge pending'));
            else for (const model of models) {
                const modelState = reportModelState(model, protocol, evidence.protocol_id);
                const line = element('span', '', 'mini-model');
                line.append(element('b', `${model.kind === 'dss' ? 'DSS' : 'NWE'} `), element('span', modelState.label, `state-badge ${modelState.tone}`));
                stateCell.append(line);
            }
            const snapshot = element('td');
            if (evidence) {
                snapshot.append(element('span', dateText(evidence.computed_at), 'metric-value'), element('span', sourceName(evidence.source), 'subline'));
                if (evidence.source_is_proxy === true) snapshot.append(element('span', 'Research proxy', 'state-badge proxy'));
                snapshot.append(element('span', `${metric(evidence.history_bars, '', 0)} sampled bars`, 'subline'));
            }
            else snapshot.append(element('span', 'No saved snapshot', 'muted'));
            const action = element('td'); const button = element('button', 'Review', 'secondary-button');
            button.type = 'button'; button.setAttribute('aria-label', `Review ${item.symbol || item.storage_symbol} entry-model evidence`);
            button.addEventListener('click', () => { void selectTicker(item); }); action.append(button);
            row.append(identity, stateCell, miniCell(models, model => `${metric(model.holdout_trades, '', 0)} trades · ${metric(model.holdout_win_rate_pct, '%', 1)}`), miniCell(models, model => metric(model.holdout_mean_return_pct, '%')), snapshot, action);
            body.append(row);
        }
    }
    function replaceOptions(select, rows, selectedValue) {
        select.replaceChildren(...rows.map(row => { const option = element('option', row.label || row.id); option.value = row.id; return option; }));
        if (rows.some(row => row.id === selectedValue)) select.value = selectedValue;
    }
    async function loadList({ clearSelection = false } = {}) {
        const serial = ++listSerial;
        if (clearSelection) { list = null; resetSelection(); }
        status('list-status', 'Loading registered tickers and saved evidence…', 'loading');
        if (clearSelection) renderTickerRows();
        $('run-scope').disabled = true;
        try {
            const payload = normalizeList(await request(`/api/entry-validation?${query()}`));
            if (serial !== listSerial) return;
            if (payload.total > 0 && offset >= payload.total) {
                offset = Math.floor((payload.total - 1) / PAGE_SIZE) * PAGE_SIZE;
                return await loadList();
            }
            list = payload;
            protocol = String(payload.protocol_id || '');
            text('protocol-label', protocol || 'Protocol identity unavailable');
            const products = [{ id: 'all', label: 'All products' }, ...(Array.isArray(payload.products) ? payload.products.filter(product => product.id !== 'all') : [])];
            replaceOptions($('validation-product'), products, scope.product);
            if (Array.isArray(payload.timeframes) && payload.timeframes.length) replaceOptions($('validation-timeframe'), payload.timeframes.map(id => ({ id, label: id.toUpperCase() })), scope.timeframe);
            for (const key of ['registered', 'backtested', 'validated']) text(`summary-${key}`, metric(payload.summary?.[key], '', 0));
            const first = payload.total ? offset + 1 : 0;
            const last = Math.min(offset + payload.rows.length, payload.total);
            text('page-label', `${first}–${last} of ${metric(payload.total, '', 0)} registered tickers`);
            status('list-status', payload.total ? `${payload.total.toLocaleString()} registered tickers in scope. Review a snapshot or backtest both current entry models. Saved results may need a rerun after history or protocol changes.` : 'No tickers match this scope. Clear the ticker search, change product, or inspect the selected app’s listing readiness on the screener.');
            renderTickerRows();
        } catch (error) {
            if (serial === listSerial) {
                status('list-status', `${error.message} Use Refresh list to recover.`, 'error');
                if (!list) renderTickerRows();
            }
        } finally { if (serial === listSerial) setBusy(busy); }
    }
    function metadata(id, entries) {
        $(id).replaceChildren(...entries.map(([name, value]) => {
            const row = element('div'); row.append(element('dt', name), element('dd', value)); return row;
        }));
    }
    function metricCard(label, value, detail) {
        const card = element('div', '', 'evidence-metric');
        card.append(element('span', label), element('strong', value), element('small', detail)); return card;
    }
    function renderBands(body, bands) {
        body.replaceChildren();
        if (!Array.isArray(bands) || !bands.length) {
            const row = element('tr'); const cell = element('td', 'No held-out score-band outcomes are available. More valid history and qualifying entries are needed.', 'empty-cell'); cell.colSpan = 6; row.append(cell); body.append(row); return;
        }
        for (const band of bands) {
            const row = element('tr'); if (numeric(band.max_score) === 10 && !band.max_inclusive) row.className = 'score-band-low';
            const interval = Array.isArray(band.win_rate_wilson_95_pct) ? band.win_rate_wilson_95_pct : [];
            const win = element('td', metric(band.win_rate_pct, '%'), 'metric-value');
            win.append(element('span', interval.length === 2 ? `${metric(interval[0], '%', 1)}–${metric(interval[1], '%', 1)}` : 'Interval unavailable', 'subline'));
            row.append(element('td', band.label || `${metric(band.min_score, '', 0)}–${metric(band.max_score, '', 0)}`), element('td', metric(band.trades, '', 0), 'metric-value'), win, element('td', metric(band.mean_return_pct, '%'), 'metric-value'), element('td', `${metric(band.average_win_pct, '%')} / ${metric(band.average_loss_pct, '%')}`, 'metric-value'), element('td', band.profit_factor_unbounded === true ? '∞ · no observed losses' : metric(band.profit_factor), 'metric-value'));
            body.append(row);
        }
    }
    function renderChecks(model) {
        const checks = model.checks || model.backtest?.checks || {};
        const labels = {
            registered_identity: 'Registered ticker and model identity match',
            holdout_sample_sufficient: 'Enough completed holdout trades', holdout_span_sufficient: 'Enough held-out calendar history',
            profitable_holdout: 'Positive held-out return and profit factor', score_discrimination_supported: 'Held-out score discrimination supported',
            venue_native_history: 'Venue-native history', fees_known: 'Product transaction fees are known', execution_costs_complete: 'Execution costs are fully covered',
        };
        $('model-checks').replaceChildren(...Object.entries(labels).map(([key, label]) => {
            const value = checks[key];
            const row = element('li', '', value === true ? 'check-pass' : value === false ? 'check-fail' : 'check-unknown');
            const marker = element('span', value === true ? '✓' : value === false ? '○' : '—', 'check-symbol'); marker.setAttribute('aria-hidden', 'true');
            const threshold = key === 'holdout_sample_sufficient' && numeric(model.backtest?.minimum_holdout_trades) !== null
                ? ` · minimum ${metric(model.backtest.minimum_holdout_trades, ' trades', 0)}`
                : key === 'holdout_span_sufficient' && numeric(model.backtest?.minimum_holdout_days) !== null
                    ? ` · minimum ${metric(model.backtest.minimum_holdout_days, ' days', 0)}` : '';
            const copy = element('span', label); copy.append(element('small', `${value === true ? 'Passed' : value === false ? 'Not passed' : 'Not reported'}${threshold}`));
            row.append(marker, copy); return row;
        }));
        $('model-blockers').replaceChildren();
        const reasons = Array.isArray(model.blocking_reasons) ? model.blocking_reasons : [];
        if (reasons.length) {
            $('model-blockers').append(element('p', 'What still blocks validation:'));
            const list = element('ul'); reasons.forEach(reason => list.append(element('li', reason))); $('model-blockers').append(list);
        }
    }
    function renderExitCandidates(backtest) {
        const body = $('exit-candidate-rows');
        if (!body) return;
        body.replaceChildren();
        const candidates = Array.isArray(backtest.validation_exit_candidates) ? backtest.validation_exit_candidates : [];
        if (!candidates.length) {
            const row = element('tr');
            const cell = element('td', 'No validation exit candidates are available in this snapshot.', 'empty-cell');
            cell.colSpan = 4; row.append(cell); body.append(row); return;
        }
        for (const candidate of candidates) {
            const row = element('tr');
            const selected = candidate.exit_rule === backtest.selected_exit_rule
                && numeric(candidate.exit_bars) === numeric(backtest.selected_exit_bars);
            if (selected) row.className = 'selected-exit';
            const rule = candidate.exit_rule === 'opposite_entry_signal_or_timeout'
                ? 'Opposite entry signal, with timeout' : 'Fixed timeout';
            row.append(
                element('td', `${selected ? 'Selected · ' : ''}${rule}`),
                element('td', metric(candidate.exit_bars, '', 0)),
                element('td', metric(candidate.trades, '', 0)),
                element('td', metric(candidate.mean_return_pct, '%')),
            );
            body.append(row);
        }
    }
    function renderModel() {
        if (!report) return;
        const models = Array.isArray(report.models) ? report.models : [];
        const model = models.find(item => item.kind === modelKind) || models[0];
        const tabs = doc.querySelector('.model-tabs'); tabs.hidden = !model; $('model-evidence').hidden = !model;
        if (!model) return;
        modelKind = model.kind;
        for (const button of tabs.querySelectorAll('[data-model]')) {
            const active = button.dataset.model === modelKind;
            button.setAttribute('aria-selected', String(active)); button.tabIndex = active ? 0 : -1;
            button.disabled = !models.some(item => item.kind === button.dataset.model);
        }
        $('model-evidence').setAttribute('aria-labelledby', `model-tab-${modelKind}`);
        const backtest = model.backtest || {};
        const holdout = backtest.holdout || {};
        const modelState = reportModelState(model, protocol, report.protocol_id);
        text('model-title', model.label || (modelKind === 'dss' ? 'DSS-led entry' : 'NWE-led entry'));
        text('model-version', `${model.score_model_id || 'Score model identity unavailable'} · ${model.signal_strategy_id || 'Entry strategy unavailable'}`);
        text('model-state', modelState.label); $('model-state').className = `state-badge ${modelState.tone}`;
        const interval = Array.isArray(holdout.win_rate_wilson_95_pct) ? holdout.win_rate_wilson_95_pct : [];
        $('evidence-metrics').replaceChildren(
            metricCard('Holdout trades', metric(holdout.trades, '', 0), `${metric(backtest.holdout_window_days, ' calendar days', 0)} in held-out window`),
            metricCard('Win rate', metric(holdout.win_rate_pct, '%'), interval.length === 2 ? `95% interval ${metric(interval[0], '%', 1)}–${metric(interval[1], '%', 1)}` : 'Uncertainty interval unavailable'),
            metricCard('Mean return', metric(holdout.mean_return_pct, '%'), holdout.return_basis || 'Return basis unavailable'),
            metricCard('Profit factor', holdout.profit_factor_unbounded === true ? '∞' : metric(holdout.profit_factor), holdout.profit_factor_unbounded === true ? 'No observed holdout losses' : 'Observed wins / observed losses'),
            metricCard('Observed payoff', metric(holdout.payoff_ratio, '×'), `Avg win ${metric(holdout.average_win_pct, '%')} · avg loss ${metric(holdout.average_loss_pct, '%')}`),
        );
        renderChecks(model);
        metadata('sample-dates', ['training', 'validation', 'holdout'].map(key => {
            const segment = backtest.splits?.[key] || {};
            return [key === 'validation' ? 'Exit selection' : key[0].toUpperCase() + key.slice(1), `${dateText(segment.start_ts)} – ${dateText(segment.end_ts)} · ${metric(segment.bars, ' bars', 0)}`];
        }));
        if (backtest.selected_exit_bars == null) {
            text('exit-protocol', 'No exit could be selected from the validation sample. The holdout is not used to choose one.');
        } else if (backtest.selected_exit_rule === 'opposite_entry_signal_or_timeout') {
            text('exit-protocol', `Enter at the next bar open. Close at the close of the first qualifying opposite-direction entry signal, or after ${backtest.selected_exit_bars} bars if none appears. The opposite signal closes the position; it does not open a reverse position. Selected using validation only.`);
        } else {
            text('exit-protocol', `Enter at next bar open; exit at the close after ${backtest.selected_exit_bars} bars. Selected using validation only. No overlapping positions or trades crossing the split boundaries.`);
        }
        renderExitCandidates(backtest);
        renderBands($('score-band-rows'), backtest.holdout_score_bands);
        text('calibration-note', backtest.discrimination_note || 'Score discrimination has not been established without adequate held-out samples and uncertainty separation.');
        const factors = Array.isArray(backtest.holdout_factor_bands) ? backtest.holdout_factor_bands : [];
        $('factor-band-details').hidden = !factors.length;
        $('factor-band-content').replaceChildren();
        for (const factor of factors) {
            const section = element('section', '', 'factor-table'); section.append(element('h4', factor.indicator || factor.label || factor.factor || 'Indicator'));
            const scroll = element('div', '', 'table-scroll'); scroll.tabIndex = 0;
            const table = element('table'); const head = element('thead'); const header = element('tr');
            ['Indicator points', 'Trades', 'Win rate / 95% interval', 'Mean return', 'Average win / loss', 'Profit factor'].forEach(title => { const th = element('th', title); th.scope = 'col'; header.append(th); });
            head.append(header); const body = element('tbody'); renderBands(body, factor.bands); table.append(head, body); scroll.append(table); section.append(scroll); $('factor-band-content').append(section);
        }
    }
    function renderReport() {
        if (!report) return;
        const instrument = report.instrument || selected || {};
        const history = report.history || {};
        const costs = report.costs || {};
        $('report-content').hidden = false; $('retry-report').hidden = true;
        text('report-title', `${instrument.symbol || instrument.storage_symbol || 'Ticker'} · ${String(report.timeframe || scope.timeframe).toUpperCase()}`);
        text('report-identity', `${APP_LABELS[instrument.exchange_id] || instrument.exchange_id || 'App unavailable'} · ${human(instrument.product_id)} · ${human(instrument.asset_category_id)} · ${instrument.storage_symbol || 'Storage identity unavailable'}`);
        status('report-status', `Saved ${dateText(report.computed_at, true)}. This is a reproducible history snapshot; rerun after new data or protocol changes.`, '');
        const notices = $('report-notices'); notices.replaceChildren();
        if (report.protocol_id !== protocol) notices.append(element('p', 'This snapshot uses a different protocol version. Rerun it before comparing it with current entry-model evidence.', 'notice'));
        if (report.source?.is_proxy === true) notices.append(element('p', `Research-proxy history · ${sourceName(report.source)}. It cannot establish venue-native execution evidence.`, 'notice proxy'));
        else if (report.source?.venue_native !== true) notices.append(element('p', 'Venue-native history has not been confirmed for this snapshot.', 'notice'));
        if (report.eligibility?.reason) notices.append(element('p', report.eligibility.reason, 'notice'));
        if (selected?.research_eligibility !== 'eligible') notices.append(element('p', 'The current registry does not mark this ticker eligible. Saved results are historical; a new run is blocked until verification or scope recovers.', 'notice'));
        if (report.status === 'NO_HISTORY') notices.append(element('p', 'No matching stored history was available. Use Sync Market on the screener, then backtest this ticker again.', 'notice'));
        if (report.status === 'UNSUPPORTED_MODEL') notices.append(element('p', 'No current four-indicator entry model is configured for this ticker’s exact app/product/category. Another model has not been substituted.', 'notice'));
        if (history.read_limit_reached) notices.append(element('p', `The ${metric(history.read_limit, '', 0)}-bar read limit was reached. Earlier candles may remain stored; validation uses only the sample dates shown here.`, 'notice'));
        metadata('history-metadata', [['Source', sourceName(report.source)], ['Provider', report.source?.data_provider || 'Unavailable'], ['Source basis', report.source?.is_proxy ? 'Research proxy' : report.source?.venue_native ? 'Venue-native history' : 'Unverified'], ['Timezone', report.source?.timezone || 'Unavailable'], ['Sampled bars', metric(history.bars, '', 0)], ['Sample dates', `${dateText(history.first_ts)} – ${dateText(history.last_ts)}`], ['Sample span', metric(history.coverage_days, ' days', 0)], ['Stored span', metric(history.stored_coverage_days, ' days', 0)], ['Protocol', report.protocol_id || 'Unavailable'], ['Data fingerprint', history.data_fingerprint || 'Unavailable']]);
        metadata('cost-metadata', [['Fee model', costs.fee_model || 'Unavailable'], ['Buy / sell fees', costs.fees_known === true ? `${metric(costs.buy_fee_pct, '%', 3)} / ${metric(costs.sell_fee_pct, '%', 3)}` : 'Unknown · gross returns only'], ['Execution-cost coverage', costs.execution_costs_complete === true ? 'Complete for this protocol' : 'Incomplete · validation blocked']]);
        text('cost-omissions', Array.isArray(costs.omissions) && costs.omissions.length ? `Not modeled: ${costs.omissions.join(', ')}.` : costs.execution_costs_complete ? 'Review the supplied cost policy and source assumptions.' : 'Complete execution-cost coverage has not been established.');
        text('sizing-reason', report.sizing?.reason || 'Entry evidence must pass all required checks before a separate sizing model can use it. No sizing model is built here.');
        const inputs = report.sizing?.required_inputs;
        text('sizing-inputs', Array.isArray(inputs) && inputs.length ? `Future sizing needs: ${inputs.join(', ')}.` : 'Future sizing needs conditional return and loss distributions, uncertainty, complete costs, and validated ticker/model/source identity.');
        renderModel();
    }
    async function selectTicker(item, { focus = true } = {}) {
        selected = item; report = null; const serial = ++reportSerial;
        text('report-title', `${item.symbol || item.storage_symbol} · ${scope.timeframe.toUpperCase()}`);
        text('report-identity', `${APP_LABELS[item.exchange_id] || item.exchange_id} · ${human(item.product_id)} · ${item.storage_symbol || item.symbol}`);
        $('report-content').hidden = true; $('retry-report').hidden = true;
        status('report-status', 'Loading saved entry-model evidence…', 'loading');
        renderTickerRows(); setBusy(busy);
        if (focus) $('report-panel').focus({ preventScroll: false });
        try {
            const payload = await request(`/api/entry-validation/report?${new URLSearchParams({ instrument_id: item.instrument_id, timeframe: scope.timeframe })}`);
            if (serial !== reportSerial) return;
            if (!payload?.report || !Array.isArray(payload.report.models)) throw new Error('The saved report was incomplete. Rerun this ticker to recover.');
            report = payload.report; renderReport();
        } catch (error) {
            if (serial !== reportSerial) return;
            const noReport = /not found|no saved|404/i.test(error.message);
            status('report-status', noReport ? 'No saved backtest for this ticker and timeframe. Run Backtest selected ticker to generate its entry evidence.' : `Saved report unavailable: ${error.message}`, noReport ? '' : 'error');
            $('retry-report').hidden = noReport;
        }
    }
    function updateProgress({ processed, total, failed, current }, skipped = 0) {
        text('run-progress-title', stopping ? 'Stopping after the current ticker…' : current ? `Backtesting ${current.symbol || current.instrument_id}` : 'Processing ticker evidence');
        text('run-progress-count', `${processed} / ${total} processed`);
        $('run-progress').max = Math.max(total, 1); $('run-progress').value = processed;
        text('run-progress-detail', `${failed} request failures · ${skipped} pending or excluded tickers skipped. Each completed request saves its snapshot; evidence checks still determine validation.`);
    }
    async function runRows(mode) {
        if (busy || (mode === 'selected' && selected?.research_eligibility !== 'eligible')) return;
        const snapshot = { ...scope };
        stopping = false; setBusy(true); $('run-progress-panel').hidden = false;
        $('failed-status').hidden = true;
        let skipped = 0;
        try {
            let queue;
            if (mode === 'selected') queue = [{ instrument_id: selected.instrument_id, symbol: selected.symbol || selected.storage_symbol }];
            else if (mode === 'retry') queue = failures.map(({ instrument_id, symbol }) => ({ instrument_id, symbol }));
            else {
                text('run-progress-title', 'Collecting the complete registered scope…');
                text('run-progress-count', '0 tickers loaded'); $('run-progress').max = 1; $('run-progress').value = 0;
                const collected = await collectScope(pageOffset => request(`/api/entry-validation?${query(snapshot, pageOffset)}`), { shouldStop: () => stopping, onPage: state => {
                    text('run-progress-count', `${state.loaded} / ${state.total} tickers loaded`);
                    text('run-progress-detail', `${state.eligible} eligible · ${state.skipped} pending or excluded. All pages are checked before the batch begins.`);
                } });
                queue = collected.eligible; skipped = collected.skipped;
            }
            if (!queue.length || stopping) {
                text('run-progress-title', stopping ? 'Stopped before starting another ticker.' : 'No eligible tickers in this scope.');
                text('run-progress-detail', `${skipped} pending or excluded tickers. Check app readiness or choose another scope.`); return;
            }
            const result = await processQueue(queue, {
                shouldStop: () => stopping,
                run: async row => {
                    const payload = await request('/api/entry-validation/run', { method: 'POST', body: JSON.stringify({ instrument_id: row.instrument_id, timeframe: snapshot.timeframe }) });
                    if (!payload?.report || !Array.isArray(payload.report.models)) throw new Error('The service returned an incomplete report.');
                    return payload.report;
                },
                onProgress: progress => updateProgress(progress, skipped),
                onResult: (row, nextReport) => { if (selected?.instrument_id === row.instrument_id) { reportSerial++; report = nextReport; renderReport(); } },
            });
            const attempted = new Set(queue.slice(0, result.processed).map(row => row.instrument_id));
            failures = [...failures.filter(row => !attempted.has(row.instrument_id)), ...result.failures];
            text('run-progress-title', result.stopped ? 'Stopped after the current ticker.' : 'Scope processing finished.');
            text('run-progress-detail', `${result.processed} of ${queue.length} ticker requests processed · ${failures.length} failed · ${skipped} pending or excluded skipped. Saved snapshots may still be unvalidated or lack matching history.`);
            if (failures.length) {
                $('failed-status').hidden = false;
                text('failed-status', `${failures.length} ticker requests failed. ${failures.slice(0, 3).map(row => `${row.symbol || row.instrument_id}: ${row.error}`).join(' · ')} Use Retry failed tickers after the service recovers.`);
            }
            await loadList();
        } catch (error) {
            text('run-progress-title', 'The batch could not continue.');
            text('run-progress-detail', `${error.message} Refresh the list and retry. Completed snapshots remain saved.`);
        } finally { stopping = false; setBusy(false); }
    }
    function applyScope() {
        if (busy) return;
        scope = { exchange: $('validation-app').value, product: $('validation-product').value, timeframe: $('validation-timeframe').value, search: $('validation-search').value.trim() };
        offset = 0; failures = []; $('failed-status').hidden = true;
        $('run-progress-panel').hidden = true;
        void loadList({ clearSelection: true });
    }
    $('scope-form').addEventListener('submit', event => { event.preventDefault(); applyScope(); });
    $('validation-app').addEventListener('change', () => { replaceOptions($('validation-product'), [{ id: 'all', label: 'All products' }], 'all'); applyScope(); });
    for (const id of ['validation-product', 'validation-timeframe']) $(id).addEventListener('change', applyScope);
    $('reload-list').addEventListener('click', () => { void loadList(); });
    $('previous-page').addEventListener('click', () => { if (!busy && offset > 0) { offset = Math.max(0, offset - PAGE_SIZE); void loadList(); } });
    $('next-page').addEventListener('click', () => { if (!busy && list?.has_more) { offset = list.next_offset; void loadList(); } });
    $('run-scope').addEventListener('click', () => { void runRows('scope'); });
    $('run-selected').addEventListener('click', () => { void runRows('selected'); });
    $('retry-failed').addEventListener('click', () => { void runRows('retry'); });
    $('stop-run').addEventListener('click', () => { stopping = true; $('stop-run').disabled = true; text('run-progress-title', 'Stopping after the current ticker…'); });
    $('retry-report').addEventListener('click', () => { if (selected) void selectTicker(selected); });
    for (const button of doc.querySelectorAll('[data-model]')) {
        button.addEventListener('click', () => { modelKind = button.dataset.model; renderModel(); });
        button.addEventListener('keydown', event => {
            if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
            event.preventDefault();
            const buttons = [...doc.querySelectorAll('[data-model]')].filter(item => !item.disabled);
            const index = buttons.indexOf(button);
            const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + buttons.length) % buttons.length;
            buttons[next]?.click(); buttons[next]?.focus();
        });
    }
    void loadList();
}

if (typeof document !== 'undefined') initializeEntryValidation(document);
