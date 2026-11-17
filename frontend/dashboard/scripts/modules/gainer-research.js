import { requestJson } from './api.js';
import { GAINER_PROTOCOL, summarizeCohorts } from './gainer-statistics.js';

const LIMITATIONS = 'Unvalidated research: current eligible listings only, so historical universe membership and delisted assets are unavailable (survivorship bias). Registration and earliest stored candle dates are not listing dates. Equal factor weights and ranking rules are fixed before evaluation; no fitting or promotion. Observations sample the next 4h return on a fixed 4-day epoch grid across available history, capped at 100,000 candles per ticker. Studies reset after gaps. Training/validation/holdout use chronological 60/20/20 date partitions with crossing outcomes purged. At least 80% of the frozen current universe and 10 instruments are required per cohort. Approximate Newey–West 95% mean intervals use four cohort lags (16 days); fewer than 30 cohorts have no interval. These intervals do not eliminate selection bias or guarantee future returns. Published fees are a current standard-rate scenario, not historical fee reconstruction; discounts, changing taxes and execution impact are excluded. No next-gainer prediction, win probability, live profile promotion, account state or orders.';

export function installGainerResearch() {
    const $ = id => document.getElementById(id);
    if (!$('gainer-run')) return;
    const status = $('gainer-status'), result = $('gainer-results');
    const inputs = ['gainer-fee-mode', 'gainer-buy', 'gainer-sell', 'gainer-spread', 'gainer-slippage'];
    let busy = false, stopping = false, state = null, saved = null;
    const paragraph = (host, text) => { const node = document.createElement('p'); node.textContent = text; host.append(node); };
    function setControls() {
        $('gainer-run').disabled = busy;
        $('gainer-stop').disabled = !busy || stopping;
        $('gainer-retry').disabled = busy || !state?.failures.size;
        $('gainer-save').disabled = busy || !state?.report || state.report.coverage.minimumHistoryDays === null;
        $('gainer-load').disabled = busy;
        for (const id of inputs) $(id).disabled = busy || (['gainer-buy', 'gainer-sell'].includes(id) && $('gainer-fee-mode').value === 'published');
    }
    function render(report, savedAt = null) {
        result.replaceChildren();
        paragraph(result, savedAt ? `Saved research snapshot: ${new Date(savedAt * 1000).toLocaleString('en-GB', { timeZone: 'Asia/Jakarta' })} WIB. Saved values may be stale; rerun for new history.` : 'Current research snapshot · unvalidated');
        paragraph(result, `Protocol ${report.protocol_id}. Frozen cutoff ${new Date(report.cutoff * 1000).toLocaleString('en-GB', { timeZone: 'Asia/Jakarta' })} WIB.`);
        const c = report.coverage;
        if (report.provenance) paragraph(result, `Input fingerprint ${report.provenance.fingerprint}; source counts ${JSON.stringify(report.provenance.sources)}. Fee policies captured for this snapshot: ${JSON.stringify(report.provenance.policies)}.`);
        paragraph(result, `${c.attempted}/${c.universe} tickers attempted; ${c.failed} failed; ${c.missing} without usable samples; ${c.short} below ${c.minimumHistoryDays} days; ${c.proxies} proxy histories; ${c.listingAgeUnknown} listing dates unknown. ${c.excludedCohorts} cohorts excluded; ${c.purged} observations purged at split boundaries. ${c.stopped ? 'Partial run: stopped.' : 'Scope processed.'}`);
        paragraph(result, `Cost scenario: ${report.costs.mode}. Spread ${report.costs.spread}% quoted; slippage ${report.costs.slippage}% per fill. ${report.costs.mode === 'manual' ? `Assumed buy ${report.costs.buy}%, sell ${report.costs.sell}% for every instrument; unverified.` : 'Published standard rates applied per instrument quote; source and publication date appear above.'}`);
        if (c.failed || c.stopped || c.short || !report.splits.holdout.count) paragraph(result, 'Predictive evidence unavailable: incomplete run or insufficient synchronized holdout coverage. Review coverage, recover history on the screener and retry failed tickers.');
        const scroll = document.createElement('div'); scroll.className = 'table-scroll'; scroll.tabIndex = 0;
        scroll.setAttribute('role', 'region'); scroll.setAttribute('aria-label', 'Chronological ranking evidence and uncertainty');
        const table = document.createElement('table');
        const head = document.createElement('thead'), heading = document.createElement('tr');
        for (const name of ['Sample', 'Metric', 'Mean', '95% interval', 'Cohorts']) { const cell = document.createElement('th'); cell.scope = 'col'; cell.textContent = name; heading.append(cell); }
        head.append(heading); table.append(head);
        const body = document.createElement('tbody');
        const labels = { top1: 'Top-1 net %', top5: 'Top-5 net %', grossTop5: 'Top-5 gross %', universe: 'Equal universe net %', momentum: 'Momentum top-5 net %', excessUniverse: 'Paired excess vs universe, pp', excessMomentum: 'Paired excess vs momentum, pp', positiveFrequency: 'Top-1 positive net frequency %', decilePrecision: 'Top-5 gainer-decile precision %', rankCorrelation: 'Spearman rank correlation' };
        const format = value => Number.isFinite(value) ? value.toFixed(3) : 'Unavailable';
        for (const [partition, metrics] of Object.entries(report.metrics)) {
            const split = report.splits[partition];
            paragraph(result, `${partition}: ${split.count} cohorts${split.first ? ` · ${new Date(split.first * 1000).toISOString().slice(0, 10)} to ${new Date(split.last * 1000).toISOString().slice(0, 10)}` : ''}`);
            for (const [key, metric] of Object.entries(metrics)) {
                const row = document.createElement('tr');
                for (const value of [partition, labels[key] || key, format(metric.mean), metric.lower === null ? 'Unavailable (<30 cohorts)' : `${format(metric.lower)} to ${format(metric.upper)}`, metric.count]) {
                    const cell = document.createElement('td'); cell.textContent = String(value); row.append(cell);
                }
                body.append(row);
            }
        }
        table.append(body); scroll.append(table); result.append(scroll);
        paragraph(result, report.limitations);
    }
    function showCoverage(payloads) {
        const body = $('gainer-coverage'); body.replaceChildren();
        for (const payload of payloads.values()) {
            const row = document.createElement('tr'), h = payload.history;
            const date = value => value ? new Date(value / 1e6).toISOString().slice(0, 10) : 'Unavailable';
            for (const value of [payload.instrument.symbol, payload.source?.id || 'Unavailable', `${h.bars || 0} bars / ${h.coverage_days || 0} days`, `${date(h.first_ts)} – ${date(h.last_ts)}`, `${h.gaps || 0} gaps${h.read_limit_reached ? ' · read cap reached' : ''} · native backfill ${h.native_backfill?.status || 'not attempted'} · ${h.native_backfill?.attempted_at || 'unknown attempt'}${h.native_backfill?.error ? ` · ${h.native_backfill.error}` : ''}`, h.registered_at ? new Date(h.registered_at * 1000).toISOString().slice(0, 10) : 'Unknown', 'Unknown · not inferred from candles']) {
                const cell = document.createElement('td'); cell.textContent = String(value); row.append(cell);
            }
            body.append(row);
        }
    }
    function finishReport() {
        const cohorts = state.cohorts;
        let missing = 0, short = 0, proxies = 0, minimumHistoryDays = saved?.minimum_history_days ?? null;
        for (const payload of state.payloads.values()) {
            if (!payload.observation_count) missing++;
            if (!payload.history.validation_span_sufficient) short++;
            if (payload.source?.is_proxy) proxies++;
            minimumHistoryDays = payload.minimum_history_days;
        }
        const sources = {}, policies = {};
        let hash = 2166136261;
        const manifest = [...state.payloads.values()].sort((a, b) => a.instrument.instrument_id.localeCompare(b.instrument.instrument_id));
        for (const payload of manifest) {
            const source = payload.source?.id || 'unavailable';
            sources[source] = (sources[source] || 0) + 1;
            if (state.costs.mode === 'published') policies[payload.costs.quote] = { buy: payload.costs.buy_fee_pct, sell: payload.costs.sell_fee_pct, label: payload.costs.label, source: payload.costs.source_url, published: payload.costs.published_date };
            const identity = `${payload.instrument.instrument_id}|${source}|${payload.history.data_fingerprint}|${state.cutoff}`;
            for (let index = 0; index < identity.length; index++) hash = Math.imul(hash ^ identity.charCodeAt(index), 16777619) >>> 0;
        }
        const provenance = { fingerprint: `fnv1a32:${hash.toString(16).padStart(8, '0')}`, sources, policies };
        const summary = summarizeCohorts(cohorts, state.universe.length);
        state.report = { protocol_id: GAINER_PROTOCOL, cutoff: state.cutoff, status: 'UNVALIDATED_RESEARCH',
            coverage: { universe: state.universe.length, attempted: state.payloads.size + state.failures.size, failed: state.failures.size, missing, short, proxies, minimumHistoryDays,
                listingAgeUnknown: state.payloads.size, stopped: stopping, excludedCohorts: summary.excluded, purged: summary.purged },
            costs: state.costs, provenance, splits: summary.splits, metrics: summary.metrics, limitations: LIMITATIONS };
        render(state.report); showCoverage(state.payloads);
        $('gainer-errors').textContent = [...state.failures.values()].map(item => `${item.symbol}: ${item.error}`).join(' · ');
    }
    async function runQueue(queue) {
        busy = true; stopping = false; setControls();
        for (const item of queue) {
            if (stopping) break;
            status.textContent = `${state.payloads.size}/${state.universe.length} read · ${state.failures.size} failed · reading ${item.symbol}. Stop finishes this request.`;
            try {
                const payload = await requestJson(`/api/entry-validation/gainer?instrument_id=${encodeURIComponent(item.instrument_id)}&cutoff=${state.cutoff}`, { signal: AbortSignal.timeout(120000) });
                if (payload.protocol_id !== GAINER_PROTOCOL || !Array.isArray(payload.observations) || payload.instrument?.instrument_id !== item.instrument_id) throw new Error('Protocol or exact instrument mismatch.');
                if (state.costs.mode === 'published' && payload.costs?.fees_known !== true) throw new Error('Published fees unavailable for this quote. Choose a manual scenario and start a new run.');
                if (payload.observations.some(row => !['time', 'signal_time', 'exit_time', 'score', 'momentum', 'gross_return'].every(key => Number.isFinite(row[key])))) throw new Error('Malformed observation.');
                if (state.observationsTotal + payload.observations.length > 1_000_000) throw new Error('Browser research limit of 1,000,000 observations reached. Full-scope evidence is unavailable.');
                let buy = state.costs.buy / 100, sell = state.costs.sell / 100;
                if (state.costs.mode === 'published') { buy = payload.costs.buy_fee_pct / 100; sell = payload.costs.sell_fee_pct / 100; }
                const halfSpread = state.costs.spread / 200, slippage = state.costs.slippage / 100;
                const factor = (1 - sell - halfSpread - slippage) / (1 + buy + halfSpread + slippage);
                for (const observation of payload.observations) {
                    const rows = state.cohorts.get(observation.time) || [];
                    rows.push({ ...observation, symbol: item.instrument_id, net_return: ((1 + observation.gross_return) * factor - 1) * 100 });
                    state.cohorts.set(observation.time, rows);
                }
                state.observationsTotal += payload.observations.length;
                payload.observation_count = payload.observations.length;
                delete payload.observations;
                state.payloads.set(item.instrument_id, payload); state.failures.delete(item.instrument_id);
            } catch (error) { state.failures.set(item.instrument_id, { ...item, error: String(error.message).slice(0, 200) }); }
        }
        finishReport(); busy = false; setControls();
        status.textContent = stopping ? 'Stopped. Start a new run to collect the full universe; completed research can be saved as partial.' : 'Scope processed. Review failures, coverage, uncertainty and limitations. Save this aggregate snapshot to keep it after reload.';
    }
    $('gainer-fee-mode').addEventListener('change', setControls);
    $('gainer-stop').addEventListener('click', () => { stopping = true; setControls(); });
    $('gainer-retry').addEventListener('click', async () => { if (state && !busy) await runQueue([...state.failures.values()]); });
    $('gainer-run').addEventListener('click', async () => {
        if (busy) return;
        const mode = $('gainer-fee-mode').value;
        const parse = id => $(id).value.trim() === '' ? NaN : Number($(id).value);
        const costs = { mode, buy: mode === 'manual' ? parse('gainer-buy') : null, sell: mode === 'manual' ? parse('gainer-sell') : null, spread: parse('gainer-spread'), slippage: parse('gainer-slippage') };
        if ([costs.spread, costs.slippage, ...(mode === 'manual' ? [costs.buy, costs.sell] : [])].some(value => !Number.isFinite(value) || value < 0 || value > 10)) {
            status.textContent = 'Enter each required cost from 0 to 10 percent. Blank means unknown; enter zero explicitly.'; return;
        }
        busy = true; stopping = false; state = null; setControls();
        try {
            const universe = [], seen = new Set(); let offset = 0;
            do {
                status.textContent = `Collecting complete Tokocrypto Spot registry · ${universe.length} eligible tickers.`;
                const page = await requestJson(`/api/entry-validation?exchange=tokocrypto&product=tokocrypto_spot&timeframe=4h&offset=${offset}&limit=100`);
                if (!Array.isArray(page.rows)) throw new Error('Registry unavailable.');
                for (const item of page.rows) if (item.research_eligibility === 'eligible' && !seen.has(item.instrument_id)) { seen.add(item.instrument_id); universe.push(item); }
                if (universe.length > 1000) throw new Error('Universe exceeds the 1,000-instrument browser research limit. No partial universe was evaluated.');
                if (!page.has_more) break;
                if (!Number.isInteger(page.next_offset) || page.next_offset <= offset) throw new Error('Registry pagination did not advance.');
                offset = page.next_offset;
            } while (!stopping);
            if (stopping || !universe.length) throw new Error(stopping ? 'Stopped before a complete registry snapshot.' : 'No eligible verified Spot instruments.');
            state = { universe, cutoff: Math.floor(Date.now() / 1000) - 1, payloads: new Map(), cohorts: new Map(), observationsTotal: 0, failures: new Map(), costs, report: null };
            await runQueue(universe);
        } catch (error) { status.textContent = `${error.message} Retry after recovery.`; busy = false; setControls(); }
    });
    $('gainer-save').addEventListener('click', async () => {
        if (!state?.report || busy) return;
        busy = true; setControls();
        try {
            saved = await requestJson('/api/entry-validation/gainer-report', { method: 'POST', body: JSON.stringify(state.report) });
            status.textContent = 'Aggregate research saved to SQLite. Individual candle outcomes are not saved. Load saved report restores this snapshot.';
        } catch (error) { status.textContent = `Save failed: ${error.message}. The result is still here; retry Save report.`; }
        finally { busy = false; setControls(); }
    });
    async function load() {
        if (busy) return;
        busy = true; setControls();
        try {
            saved = await requestJson('/api/entry-validation/gainer-report');
            const fees = $('gainer-fee-reference'); fees.replaceChildren();
            for (const policy of saved.fee_policies || []) {
                paragraph(fees, `${policy.quote}: buy ${policy.buy_fee_pct}%, sell ${policy.sell_fee_pct}% · ${policy.label}`);
                if (policy.source_url?.startsWith('https://support.tokocrypto.com/')) { const link = document.createElement('a'); link.href = policy.source_url; link.textContent = 'Official published fee source'; link.target = '_blank'; link.rel = 'noopener noreferrer'; fees.append(link); }
            }
            if (saved.report) {
                render(saved.report, saved.computed_at);
                status.textContent = saved.report.protocol_id === GAINER_PROTOCOL ? 'Saved browser-computed aggregate research loaded. Rerun for changed history; snapshots do not imply current validation.' : 'Saved protocol is outdated. Start a new run before comparing results.';
            } else status.textContent = 'No saved ranking report. Choose cost assumptions and run the complete Spot universe.';
        } catch (error) { status.textContent = `Saved research unavailable: ${error.message}. Use Load saved report to retry.`; }
        finally { busy = false; setControls(); }
    }
    $('gainer-load').addEventListener('click', load);
    setControls(); void load();
}
