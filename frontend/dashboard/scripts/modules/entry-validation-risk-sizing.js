export function renderAutocorrelationRiskSizing({ riskSizing, excludedCosts, costs, metric, metricCard, dateText, numeric, doc = document }) {
    const node = id => doc.getElementById(id);
    const validation = riskSizing?.validation;
    const holdout = riskSizing?.holdout;
    const state = node('risk-sizing-state');
    const statusText = node('risk-sizing-status');
    if (!state || !statusText) return;
    const labels = {
        RESEARCH_ONLY: 'Research only',
        EXPLORATORY_ONLY: 'Exploratory only',
        INSUFFICIENT_EVIDENCE: 'More evidence needed',
        UNAVAILABLE: 'Unavailable',
    };
    state.textContent = labels[riskSizing?.status] || 'Rerun 4h backtest';
    state.className = `state-badge ${riskSizing?.status === 'RESEARCH_ONLY' ? 'valid' : 'pending'}`;
    const renderSample = (target, label, sample) => {
        const metrics = node(target);
        if (!metrics) return;
        if (!sample) {
            metrics.replaceChildren(metricCard(`${label} risk sizing`, 'Unavailable', riskSizing?.reason || 'Rerun the 4h report to calculate this sample.'));
            return;
        }
        const median = numeric(sample.median_size_multiplier);
        const baseRiskPct = numeric(sample.base_risk_per_trade_pct);
        const adjustedRiskPct = median === null || baseRiskPct === null ? null : baseRiskPct * median;
        const adjustedReturn = numeric(sample.autocorrelation_adjusted_equity_return_pct);
        const adjustedDrawdown = numeric(sample.autocorrelation_adjusted_max_drawdown_pct);
        const adjustedPath = adjustedReturn === null
            ? 'Unavailable'
            : `${metric(sample.fixed_risk_equity_return_pct, '%')} → ${metric(adjustedReturn, '%')}`;
        const drawdowns = adjustedDrawdown === null
            ? 'Autocorrelation path unavailable'
            : `Max drawdown ${metric(sample.fixed_risk_max_drawdown_pct, '%')} → ${metric(adjustedDrawdown, '%')}`;
        metrics.replaceChildren(
            metricCard(`${label} coverage`, `${metric(sample.coverage_pct, '%', 1)}`, `${metric(sample.autocorrelation_estimates, '', 0)} / ${metric(sample.trades, ' trades', 0)} had a causal estimate`),
            metricCard(`${label} median multiplier`, median === null ? 'Unavailable' : metric(median, '×', 3), `Mean ${metric(sample.mean_size_multiplier, '×', 3)} · never above 1.000×`),
            metricCard(`${label} risk budget`, adjustedRiskPct === null ? 'Unavailable' : metric(adjustedRiskPct, '%', 3), `${metric(sample.base_risk_per_trade_pct, '%', 2)} base risk × median multiplier`),
            metricCard(`${label} simulated return`, adjustedPath, drawdowns),
        );
    };
    const message = riskSizing?.status === 'RESEARCH_ONLY'
        ? `${riskSizing.method || 'Causal return autocorrelation is used to adjust hypothetical risk.'} The fixed-risk and adjusted-risk paths use non-overlapping trades and configured transaction fees.`
        : (riskSizing?.reason || 'Run the saved report at 4h with enough contiguous completed candles and known fees to inspect autocorrelation-based sizing.');
    statusText.textContent = message;
    statusText.dataset.tone = riskSizing?.status === 'RESEARCH_ONLY' ? '' : 'pending';
    renderCalculator(riskSizing, costs, metric, metricCard, dateText, numeric, node);
    renderSample('risk-sizing-validation-metrics', 'Validation', validation);
    renderSample('risk-sizing-holdout-metrics', 'Untouched holdout', holdout);
    node('risk-sizing-limits').textContent = `The comparison compounds a hypothetical ${metric(riskSizing?.base_risk_per_trade_pct, '%', 2)} risk budget per planned stop-loss unit from normalized equity. It requires ${metric(riskSizing?.minimum_return_samples, ' contiguous 4h return samples', 0)}; up to ${metric(riskSizing?.lookback_return_bars, ' 4h return bars', 0)} are used. Any unavailable estimate makes the adjusted path incomplete. This is per-ticker research, not live account sizing; omitted costs include ${Array.isArray(excludedCosts) ? excludedCosts.join(', ') : 'spread, slippage, funding, liquidity and market impact'}.`;
}

function renderCalculator(riskSizing, costs, metric, metricCard, dateText, numeric, node) {
    const metrics = node('risk-sizing-calculator-metrics');
    const statusText = node('risk-sizing-calculator-status');
    if (!metrics || !statusText) return;
    const estimate = riskSizing?.latest_causal_estimate;
    const controls = ['risk-sizing-equity', 'risk-sizing-entry', 'risk-sizing-stop', 'risk-sizing-direction', 'risk-sizing-base-risk']
        .map(node).filter(Boolean);
    const baseRiskInput = node('risk-sizing-base-risk');
    const maximumBaseRisk = numeric(riskSizing?.maximum_base_risk_per_trade_pct);
    if (baseRiskInput && maximumBaseRisk !== null) baseRiskInput.max = String(maximumBaseRisk);
    const render = () => {
        metrics.replaceChildren();
        if (riskSizing?.status !== 'RESEARCH_ONLY' || !estimate || costs?.fees_known !== true) {
            statusText.dataset.tone = 'pending';
            statusText.textContent = costs?.fees_known !== true
                ? 'Known buy and sell fees are required for a fee-aware size. No estimate is shown.'
                : (riskSizing?.reason || 'A complete 4h validation and holdout risk study with a latest causal estimate is required.');
            return;
        }
        const multiplier = numeric(estimate.multiplier);
        const equity = numeric(node('risk-sizing-equity')?.value);
        const entry = numeric(node('risk-sizing-entry')?.value);
        const stop = numeric(node('risk-sizing-stop')?.value);
        const baseRisk = numeric(node('risk-sizing-base-risk')?.value);
        const direction = node('risk-sizing-direction')?.value;
        const exposureIsValid = multiplier !== null && equity !== null && equity > 0
            && entry !== null && entry > 0 && stop !== null && stop > 0
            && baseRisk !== null && baseRisk > 0 && maximumBaseRisk !== null && baseRisk <= maximumBaseRisk
            && ((direction === 'LONG' && stop < entry) || (direction === 'SHORT' && stop > entry));
        if (!exposureIsValid) {
            statusText.dataset.tone = 'pending';
            statusText.textContent = `Latest completed report bar ${dateText(estimate.bar_time, true)}: multiplier ${metric(multiplier, '×', 3)}, horizon variance ratio ${metric(estimate.horizon_variance_ratio, '', 3)} from ${metric(estimate.return_samples, ' contiguous returns', 0)}. Enter positive equity, entry and stop values; the stop must be on the loss side, and base risk must be positive and no more than ${metric(maximumBaseRisk, '%', 1)}.`;
            metrics.append(metricCard('Latest causal size multiplier', metric(multiplier, '×', 3), `4h report bar · ${dateText(estimate.bar_time, true)}`));
            return;
        }
        const entryFeePct = direction === 'LONG' ? numeric(costs.buy_fee_pct) : numeric(costs.sell_fee_pct);
        const exitFeePct = direction === 'LONG' ? numeric(costs.sell_fee_pct) : numeric(costs.buy_fee_pct);
        const priceRisk = direction === 'LONG' ? entry - stop : stop - entry;
        if (entryFeePct === null || exitFeePct === null) {
            statusText.dataset.tone = 'error';
            statusText.textContent = 'Fee values are missing from this saved report. Rerun it before calculating a fee-aware size.';
            return;
        }
        const riskPerUnit = priceRisk + entry * entryFeePct / 100 + stop * exitFeePct / 100;
        if (!Number.isFinite(riskPerUnit) || riskPerUnit <= 0) {
            statusText.dataset.tone = 'error';
            statusText.textContent = 'Fee-adjusted stop risk is invalid for these inputs. Check the entry, stop and fee values.';
            return;
        }
        const riskBudget = equity * baseRisk / 100 * multiplier;
        const quantity = riskBudget / riskPerUnit;
        if (!Number.isFinite(riskBudget) || riskBudget <= 0 || !Number.isFinite(quantity)) {
            statusText.dataset.tone = 'error';
            statusText.textContent = 'These inputs exceed the calculator’s numeric range. Use smaller hypothetical equity or price values.';
            return;
        }
        statusText.dataset.tone = '';
        statusText.textContent = `Uses the latest completed 4h report bar from ${dateText(estimate.bar_time, true)} and a ${metric(riskSizing.selected_maximum_hold_bars, ' bar', 0)} horizon. This local scenario is not a live price or an order.`;
        metrics.replaceChildren(
            metricCard('Autocorrelation-adjusted risk budget', metric(riskBudget, '', 2), `${metric(baseRisk, '%', 2)} base × ${metric(multiplier, '×', 3)} multiplier`),
            metricCard('Theoretical quantity', metric(quantity, '', 6), 'Before lot sizes, liquidity and capital limits'),
            metricCard('Fee-adjusted risk per unit', metric(riskPerUnit, '', 8), `${metric(entryFeePct, '%', 3)} entry · ${metric(exitFeePct, '%', 3)} stop exit fee`),
            metricCard('Estimated loss at stop', metric(quantity * riskPerUnit, '', 2), `Entry notional ${metric(quantity * entry, '', 2)} · ${direction}`),
        );
    };
    for (const input of controls) {
        input.oninput = render;
        input.onchange = render;
    }
    render();
}
