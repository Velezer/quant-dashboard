import { hasFiveIndicatorScores, oppositeEntryExitCue, signalIndicatorScores, signalIndicatorSummary, signalTotalStrength } from './signal-screener-model.js';
/** Builds one accessible signal row from normalized screener data. */
export function createSignalScreenerCard({
    escape,
    appLabel,
    scoreModelFor,
    signalScoreState,
    signalScorePresentation,
    signalDisplayScore,
    signalScoreWeightSummary,
    signalPriceBarAgeLabel,
    formatPrice,
    ageLabel,
    renderScoreBasis,
}) {
    return function renderSignalCard(signal, rank, selected) {
        const scoreState = signalScoreState(signal);
        const exitCue = oppositeEntryExitCue(signal);
        const exitText = exitCue
            ? `If holding ${exitCue.positionSide}: ${exitCue.provisional ? 'exit watch · wait for bar close' : 'exit cue'} · opposite ${exitCue.signalDirection}`
            : '';
        const exitExplanation = 'Applies to a position using the same app, product, model and timeframe. No order is placed.';
        const directionClass = signal.direction === 'SHORT' ? 'signal-direction-short' : 'signal-direction-long';
        const directionLabel = scoreState && !scoreState.current ? `LAST ${signal.direction}` : signal.direction;
        const evidence = String(signal.evidence_status || 'VALIDATION_PENDING').replaceAll('_', ' ');
        const scoreModel = scoreModelFor(signal);
        const presentation = signalScorePresentation(signal, scoreModel);
        const scoreScaleMax = Number.isFinite(presentation.scoreScaleMax)
            ? presentation.scoreScaleMax
            : 10;
        const fiveIndicators = hasFiveIndicatorScores(signal, scoreModel);
        const afterFeeGate = signal.score_basis?.after_fee_gate;
        const gateHoldout = afterFeeGate?.holdout || {};
        const gateWinRate = Number(gateHoldout.win_rate_after_fees_pct);
        const gateLowerBound = Number(gateHoldout.win_rate_after_fees_wilson_95_lower_pct);
        const gateTrades = Number(gateHoldout.trades);
        const gateMinimumScore = Number(afterFeeGate?.minimum_score_out_of_50);
        const targetAfterFeeWinRate = Number(afterFeeGate?.target_after_fee_win_rate_pct ?? 50);
        const hasAfterFeeMetric = Number.isFinite(gateWinRate) && Number.isFinite(gateLowerBound)
            && Number.isFinite(gateTrades) && Number.isFinite(gateMinimumScore);
        const afterFeeMetricText = fiveIndicators && (hasAfterFeeMetric || signal.persistent_score !== true)
            ? hasAfterFeeMetric
                ? `Historical holdout ${gateWinRate.toFixed(1)}% after fees vs ${targetAfterFeeWinRate.toFixed(0)}% target · ${Math.trunc(gateTrades)} trades · 95% lower bound ${gateLowerBound.toFixed(1)}% · profile cutoff ${gateMinimumScore.toFixed(0)}/50 is informational, not a signal filter`
                : 'No promoted after-fee profile · equal weights used · signal shown without a profile filter'
            : '';
        const afterFeeAccessibleText = hasAfterFeeMetric
            ? `${afterFeeMetricText}. Historical cohort evidence is not this signal's win probability.`
            : afterFeeMetricText;
        const totalStrength = signalTotalStrength(signal, scoreModel);
        const displayScore = signalDisplayScore(signal);
        const scoreLabel = scoreState?.tone === 'stale' ? 'Last recorded score' : 'Market-entry score';
        const scoreValue = Number.isFinite(displayScore) ? displayScore.toFixed(1) : '—';
        const scoreStateLabel = scoreState ? `, ${scoreState.label.toLowerCase()}` : '';
        const componentSummary = fiveIndicators
            ? signalIndicatorScores(signal, scoreModel).map(item => `${item.indicator}: ${item.state}`).join('; ')
            : Array.isArray(signal.score_basis?.components) ? signal.score_basis.components.map(component =>
                `${component.indicator || 'Indicator'} ${component.state || 'state unavailable'}, ${Number(component.points || 0).toFixed(1)} points`,
            ).join('; ')
            : '';
        const entryStatus = scoreState?.current === false ? 'no current market-entry signal' : 'current technical market-entry signal';
        const accessibleScore = fiveIndicators
            ? `${Number.isFinite(displayScore) ? `${scoreValue} out of ${scoreScaleMax} weighted total; ` : ''}${signalIndicatorSummary(signal, scoreModel)}`
            : Number.isFinite(displayScore) ? `${scoreValue} out of ${scoreScaleMax}` : 'unavailable';
        const scoreAccessibleLabel = `${rank ? `Entry ${rank}, ` : ''}${signal.symbol}, ${directionLabel}, ${signal.timeframe}, ${entryStatus}, ${fiveIndicators ? 'indicator scores' : scoreLabel.toLowerCase()} ${accessibleScore}${scoreStateLabel}. Score weights: ${signalScoreWeightSummary(signal, scoreModel)}. ${componentSummary}. ${totalStrength ? `${totalStrength.label}. ${totalStrength.description} ` : ''}${afterFeeAccessibleText ? `${afterFeeAccessibleText}. ` : ''}Setup strength is not win probability or execution availability. ${exitText ? `${exitText}. ${exitExplanation} ` : ''}Select to inspect chart.`;
        const productLabel = signal.product_id === 'stockbit_regular'
            ? ''
            : signal.product_id.replaceAll('_', ' ');
        const strategyLabel = scoreModel?.short_label || signal.signal_strategy_name;
        const rowContext = [appLabel(signal.exchange_id), productLabel, strategyLabel]
            .filter(Boolean)
            .join(' · ');
        const provenance = [signal.data_provider, signal.data_source, signal.data_quality]
            .filter(value => String(value || '').trim() && value !== 'unknown')
            .map(value => String(value).replaceAll('_', ' '));
        return `<article class="signal-row-wrap" role="listitem">
            <button type="button" class="signal-row${totalStrength?.tone === 'low' ? ' is-low-total-strength' : ''}${selected ? ' is-selected' : ''}" data-signal-id="${escape(signal.signal_id)}" aria-pressed="${selected}" aria-label="${escape(scoreAccessibleLabel)}">
                <span class="signal-row-top"><span class="signal-row-rank">#${rank}</span><span class="signal-row-symbol">${escape(signal.symbol)}</span><span class="signal-row-timeframe">${escape(signal.timeframe.toUpperCase())}</span></span>
                <span class="signal-row-meta">${escape(rowContext)}</span>
                ${scoreState ? `<span class="signal-score-state signal-score-state-${scoreState.tone}" role="status">${escape(scoreState.label)} · ${signal.persistent_score === true ? 'strategy' : 'price'} bar ${escape(signalPriceBarAgeLabel(signal.persistent_score === true ? signal.score_bar_time : signal.price_bar_time))}</span>` : ''}
                <span class="signal-row-main"><span class="${directionClass}">${escape(directionLabel)}</span><span class="signal-row-price">${escape(formatPrice(signal.current_price))}</span>${fiveIndicators ? `<span class="signal-row-score">${scoreState?.tone === 'stale' ? 'Last recorded total' : 'Weighted total'} <strong>${scoreValue}</strong><span aria-hidden="true"> / 50</span> · five /10 indicators</span>` : `<span class="signal-row-score">${escape(scoreLabel)} <strong>${scoreValue}</strong><span aria-hidden="true"> / ${Number.isFinite(scoreScaleMax) ? scoreScaleMax : 10}</span></span>`}</span>
                ${exitCue ? `<span class="signal-exit-cue"><strong>${escape(exitText)}</strong><small>${escape(exitExplanation)}</small></span>` : ''}
                ${totalStrength ? `<span class="signal-total-strength signal-total-strength-${totalStrength.tone}" title="${escape(totalStrength.description)}">${escape(totalStrength.label)}<small>${escape(totalStrength.description)}</small></span>` : ''}
                ${afterFeeMetricText ? `<span class="signal-after-fee-metric" title="${escape(hasAfterFeeMetric ? 'Historical cohort evidence does not predict this individual signal.' : 'After-fee status is informational; it does not filter signals.')}">${escape(afterFeeMetricText)}<small>${hasAfterFeeMetric ? 'Historical cohort holdout evidence; not an individual win-rate forecast.' : 'This signal remains visible without a validated profile.'}</small></span>` : ''}
                ${renderScoreBasis(signal)}
                <span class="signal-row-footer"><span>${escape(evidence)}</span><span>Price ${escape(signalPriceBarAgeLabel(signal.price_bar_time))}</span><span>Scanned ${escape(ageLabel(signal.last_updated_at))}</span></span>
                ${provenance.length ? `<span class="signal-row-provenance">${escape(provenance.join(' · '))}</span>` : ''}
            </button>
        </article>`;
    };
}
