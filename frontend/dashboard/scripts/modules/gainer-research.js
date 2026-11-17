import { requestJson } from './api.js';

const PROTOCOL = 'tokocrypto_dss_equal_weight_next_4h_rank_v1';

export function installGainerResearch() {
    const run = document.getElementById('gainer-run');
    if (!run) return;
    const stop = document.getElementById('gainer-stop');
    const status = document.getElementById('gainer-status');
    const result = document.getElementById('gainer-results');
    let stopping = false;
    stop.addEventListener('click', () => { stopping = true; stop.disabled = true; });
    run.addEventListener('click', async () => {
        const costInputs = ['gainer-buy', 'gainer-sell', 'gainer-spread', 'gainer-slippage'];
        const costs = costInputs.map(id => document.getElementById(id).value.trim());
        if (costs.some(value => !value || !Number.isFinite(Number(value)) || Number(value) < 0 || Number(value) > 10)) {
            status.textContent = 'Enter each cost from 0 to 10 percent. These are research assumptions; verify your actual fee tier.';
            return;
        }
        const [buy, sell, spread, slippage] = costs.map(value => Number(value) / 100);
        // Half the quoted spread on each side; slippage applies to each fill.
        const costFactor = (1 - sell - spread / 2 - slippage) / (1 + buy + spread / 2 + slippage);
        stopping = false; run.disabled = true; stop.disabled = false;
        result.replaceChildren();
        for (const id of costInputs) document.getElementById(id).disabled = true;
        const cohorts = new Map();
        let attempted = 0, failed = 0, missing = 0, proxies = 0, short = 0;
        try {
            const universe = [];
            let offset = 0;
            do {
                const page = await requestJson(`/api/entry-validation?exchange=tokocrypto&product=tokocrypto_spot&timeframe=4h&offset=${offset}&limit=100`);
                if (!Array.isArray(page.rows)) throw new Error('Registry unavailable. Retry the run.');
                universe.push(...page.rows.filter(row => row.research_eligibility === 'eligible'));
                if (!page.has_more) break;
                if (!Number.isInteger(page.next_offset) || page.next_offset <= offset) throw new Error('Registry pagination did not advance.');
                offset = page.next_offset;
            } while (!stopping);
            for (const item of universe) {
                if (stopping) break;
                status.textContent = `${attempted}/${universe.length} tickers processed · ${failed} failed · reading ${item.symbol}. Stop finishes the current ticker.`;
                try {
                    const payload = await requestJson(`/api/entry-validation/gainer?instrument_id=${encodeURIComponent(item.instrument_id)}`, { signal: AbortSignal.timeout(120000) });
                    if (payload.protocol_id !== PROTOCOL || !Array.isArray(payload.observations)) throw new Error('Ranking protocol unavailable');
                    if (!payload.observations.length) missing++;
                    if (payload.source?.is_proxy) proxies++;
                    if (payload.history?.validation_span_sufficient !== true) short++;
                    for (const observation of payload.observations) {
                        if (![observation.time, observation.score, observation.momentum, observation.gross_return].every(Number.isFinite)) continue;
                        const cohort = cohorts.get(observation.time) || [];
                        cohort.push({ ...observation, symbol: item.symbol });
                        cohorts.set(observation.time, cohort);
                    }
                } catch { failed++; }
                attempted++;
            }
            const dates = [...cohorts.keys()].sort((a, b) => a - b);
            const holdout = dates.slice(Math.floor(dates.length * 0.8));
            const samples = [];
            for (const time of holdout) {
                const rows = cohorts.get(time);
                // Do not treat a small subset as evidence for the complete universe.
                if (rows.length < Math.max(10, Math.ceil(universe.length * 0.8))) continue;
                const ranked = [...rows].sort((a, b) => b.score - a.score || a.symbol.localeCompare(b.symbol)).filter(row => row.score > 0);
                if (!ranked.length) continue;
                const momentum = [...rows].sort((a, b) => b.momentum - a.momentum || a.symbol.localeCompare(b.symbol));
                const winners = new Set([...rows].sort((a, b) => b.gross_return - a.gross_return).slice(0, Math.max(1, Math.ceil(rows.length / 10))).map(row => row.symbol));
                const net = row => ((1 + row.gross_return) * costFactor - 1) * 100;
                const mean = list => list.reduce((sum, row) => sum + net(row), 0) / list.length;
                samples.push({ top1: net(ranked[0]), top5: mean(ranked.slice(0, 5)), universe: mean(rows), momentum: mean(momentum.slice(0, 5)), hit: winners.has(ranked[0].symbol) ? 100 : 0 });
            }
            const add = text => { const p = document.createElement('p'); p.textContent = text; result.append(p); };
            add(`${attempted}/${universe.length} attempted; ${failed} failures; ${missing} without usable samples; ${short} below 1,000 days; ${proxies} proxy sources. ${stopping ? 'Stopped: partial universe.' : 'Scope finished.'}`);
            add('Unvalidated research. Fixed DSS-led equal weights, causal 512-bar lookback, next-open entry and next-4h-open exit. Latest 64 completed bars per ticker; final chronological 20% is reported. No weight or rule fitting. Current listings introduce survivorship bias. Assumed costs are applied to both sides; they are not verified venue fees. Results are transient and must not be used as a win probability.');
            if (!samples.length || stopping || failed) {
                add('Predictive evidence unavailable: incomplete run or insufficient synchronized holdout cohorts at 80% universe coverage. Recover missing history using Sync Market on the screener and retry.');
            } else {
                const average = key => samples.reduce((sum, row) => sum + row[key], 0) / samples.length;
                add(`${samples.length} holdout cohorts · top-1 mean net ${average('top1').toFixed(3)}% · top-5 ${average('top5').toFixed(3)}% · equal universe ${average('universe').toFixed(3)}% · prior-4h momentum top-5 ${average('momentum').toFixed(3)}% · top-1 next-gainer-decile frequency ${average('hit').toFixed(1)}%.`);
                add('This bounded recent sample does not establish statistical significance or future predictive ability. Longer untouched history and uncertainty estimates are required.');
            }
            status.textContent = stopping ? 'Stopped after current ticker. Retry starts a new snapshot.' : 'Research run finished. Review coverage and limitations below.';
        } catch (error) { status.textContent = `Research unavailable: ${error.message} Retry after recovery.`; }
        finally {
            run.disabled = false; stop.disabled = true;
            for (const id of costInputs) document.getElementById(id).disabled = false;
        }
    });
}
