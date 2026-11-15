import { hasSixIndicatorScores, oppositeEntryExitCue, signalIndicatorScores, signalIndicatorSummary, signalTotalStrength } from './signal-screener-model.js';
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
        const sixIndicators = hasSixIndicatorScores(signal, scoreModel);
        const indicatorScores = sixIndicators ? signalIndicatorScores(signal, scoreModel) : [];
        const markovScore = sixIndicators
            ? indicatorScores.find(item => item.id === 'markov_regime')
            : null;
        const autocorrelationScore = sixIndicators
            ? indicatorScores.find(item => item.id === 'lag_one_return_autocorrelation')
            : null;
        const currentModelScaleMax = Number(scoreModel?.score_scale_max);
        const regimeSummary = markovScore
            ? `<span class="signal-row-regime-score" title="Markov regime contributes a fixed 10-point weight to the current /60 scoring model"><span>Regime</span><strong>${markovScore.points === null ? '—' : markovScore.points.toFixed(1)}/10</strong><small>${escape(markovScore.state.replaceAll('_', ' ').toLowerCase())}</small></span>`
            : currentModelScaleMax === 60
                ? presentation.legacy && presentation.scoreScaleMax < 60
                    ? `<span class="signal-row-regime-score signal-row-regime-legacy" title="This saved row uses the previous scoring model and has no recorded Markov regime score"><span>Regime</span><strong>Not scored</strong><small>legacy /${presentation.scoreScaleMax} row</small></span>`
                    : `<span class="signal-row-regime-score signal-row-regime-unavailable" title="This row uses the current /60 scoring model, but its saved regime component is unavailable"><span>Regime</span><strong>Unavailable</strong><small>saved score basis</small></span>`
                : '';
        const autocorrelationRho = Number(autocorrelationScore?.value);
        const autocorrelationSummary = autocorrelationScore
            ? `<span class="signal-row-regime-score" title="Lag-one return autocorrelation contributes a fixed 10 points. The displayed rho is a same-timeframe return correlation; its causal AR(1) forecast is scored in the signal direction."><span>Autocorrelation</span><strong>${autocorrelationScore.points === null ? '—' : autocorrelationScore.points.toFixed(1)}/10</strong><small>${Number.isFinite(autocorrelationRho) ? `ρ ${autocorrelationRho.toFixed(2)}` : 'unavailable'} · ${escape(autocorrelationScore.state.replaceAll('_', ' ').toLowerCase())}</small></span>`
            : '';
        const afterFeeGate = signal.score_basis?.after_fee_gate;
        const gateHoldout = afterFeeGate?.holdout || {};
        const gateWinRate = Number(gateHoldout.win_rate_after_fees_pct);
        const gateLowerBound = Number(gateHoldout.win_rate_after_fees_wilson_95_lower_pct);
        const gateTrades = Number(gateHoldout.trades);
        const gateMinimumScore = Number(afterFeeGate?.minimum_score_out_of_60);
        const targetAfterFeeWinRate = Number(afterFeeGate?.target_after_fee_win_rate_pct ?? 50);
        const hasAfterFeeMetric = Number.isFinite(gateWinRate) && Number.isFinite(gateLowerBound)
            && Number.isFinite(gateTrades) && Number.isFinite(gateMinimumScore);
        const afterFeeMetricText = sixIndicators && (hasAfterFeeMetric || signal.persistent_score !== true)
            ? hasAfterFeeMetric
                ? `Product holdout ${gateWinRate.toFixed(1)}% after fees vs ${targetAfterFeeWinRate.toFixed(0)}% target · ${Math.trunc(gateTrades)} trades · 95% lower bound ${gateLowerBound.toFixed(1)}% · profile cutoff ${gateMinimumScore.toFixed(0)}/60 is informational, not a signal filter`
                : 'No promoted after-fee profile · equal weights used · signal shown without a profile filter'
            : '';
        const afterFeeAccessibleText = hasAfterFeeMetric
            ? `${afterFeeMetricText}. This is product-scoped cohort evidence; the score-matched win estimate is shown separately.`
            : afterFeeMetricText;
        const winProbability = signal.win_probability || signal.score_basis?.win_probability || {};
        const winProbabilityStatus = String(winProbability.status || 'UNAVAILABLE');
        const winProbabilityPct = Number(winProbability.win_probability_pct);
        const winProbabilityInterval = Array.isArray(winProbability.confidence_interval_95_pct)
            ? winProbability.confidence_interval_95_pct.map(Number)
            : [];
        const winProbabilityTrades = Number(winProbability.sample_trades);
        const scoreBandSampleTrades = winProbability.score_band_sample_trades === null
            ? Number.NaN
            : Number(winProbability.score_band_sample_trades);
        const hasWinProbability = ['ESTIMATED', 'PROFILE_GATE_ESTIMATE'].includes(winProbabilityStatus)
            && Number.isFinite(winProbabilityPct) && Number.isFinite(winProbabilityTrades);
        const probabilityRange = Number.isFinite(winProbabilityInterval[0])
            && Number.isFinite(winProbabilityInterval[1])
            ? ` · 95% interval ${winProbabilityInterval[0].toFixed(1)}–${winProbabilityInterval[1].toFixed(1)}%`
            : '';
        const hasScoreBand = Number.isFinite(Number(winProbability.score_band_minimum_out_of_60))
            && Number.isFinite(Number(winProbability.score_band_maximum_out_of_60));
        const scoreBand = hasScoreBand
            ? ` · score band ${Number(winProbability.score_band_minimum_out_of_60).toFixed(0)}–${Number(winProbability.score_band_maximum_out_of_60).toFixed(0)}/60${winProbability.score_band_maximum_inclusive ? ' inclusive' : ''}`
            : '';
        const winProbabilityText = hasWinProbability
            ? `${winProbabilityStatus === 'ESTIMATED' ? 'Estimated win probability' : 'Profile-gate win estimate'} ${winProbabilityPct.toFixed(1)}%`
            : `Win probability unavailable · ${winProbabilityStatus === 'INSUFFICIENT_SAMPLE'
                ? `${Number.isFinite(winProbabilityTrades) ? Math.trunc(winProbabilityTrades) : 0} matching outcomes; at least 30 required`
                : winProbabilityStatus === 'BELOW_PROFILE_GATE'
                    ? 'signal is below the validated profile score gate'
                    : String(winProbability.reason || 'no matching validated holdout calibration')}`;
        const winProbabilityDetail = hasWinProbability
            ? `${probabilityRange} · ${Math.trunc(winProbabilityTrades)} after-fee holdout outcomes${scoreBand} · long/short pooled${winProbabilityStatus === 'PROFILE_GATE_ESTIMATE' ? ` · ${Number.isFinite(scoreBandSampleTrades) ? `${Math.trunc(scoreBandSampleTrades)} score-band outcomes; profile-gate cohort used` : 'score-band sample unavailable; profile-gate cohort used'}` : ''}`
            : String(winProbability.reason || winProbability.basis || 'Refresh the backtest profile to build a matching estimate.');
        const winProbabilityAccessibleText = hasWinProbability
            ? `${winProbabilityText}, ${Math.trunc(winProbabilityTrades)} after-fee holdout outcomes${probabilityRange}${scoreBand}.`
            : winProbabilityText;
        const totalStrength = signalTotalStrength(signal, scoreModel);
        const displayScore = signalDisplayScore(signal);
        const scoreLabel = scoreState?.tone === 'stale' ? 'Last recorded score' : 'Market-entry score';
        const scoreValue = Number.isFinite(displayScore) ? displayScore.toFixed(1) : '—';
        const scoreStateLabel = scoreState ? `, ${scoreState.label.toLowerCase()}` : '';
        const componentSummary = sixIndicators
            ? indicatorScores.map(item => `${item.indicator}: ${item.state}`).join('; ')
            : Array.isArray(signal.score_basis?.components) ? signal.score_basis.components.map(component =>
                `${component.indicator || 'Indicator'} ${component.state || 'state unavailable'}, ${Number(component.points || 0).toFixed(1)} points`,
            ).join('; ')
            : '';
        const entryStatus = scoreState?.current === false ? 'no current market-entry signal' : 'current technical market-entry signal';
        const accessibleScore = sixIndicators
            ? `${Number.isFinite(displayScore) ? `${scoreValue} out of ${scoreScaleMax} weighted total; ` : ''}${signalIndicatorSummary(signal, scoreModel)}`
            : Number.isFinite(displayScore) ? `${scoreValue} out of ${scoreScaleMax}` : 'unavailable';
        const scoreAccessibleLabel = `${rank ? `Entry ${rank}, ` : ''}${signal.symbol}, ${directionLabel}, ${signal.timeframe}, ${entryStatus}, ${sixIndicators ? 'indicator scores' : scoreLabel.toLowerCase()} ${accessibleScore}${scoreStateLabel}. Score weights: ${signalScoreWeightSummary(signal, scoreModel)}. ${componentSummary}. ${totalStrength ? `${totalStrength.label}. ${totalStrength.description} ` : ''}${afterFeeAccessibleText ? `${afterFeeAccessibleText}. ` : ''}${winProbabilityAccessibleText} Setup score measures strength; the estimate is historical, not a guarantee or execution probability. ${exitText ? `${exitText}. ${exitExplanation} ` : ''}Select to inspect chart.`;
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
                <span class="signal-row-main"><span class="${directionClass}">${escape(directionLabel)}</span><span class="signal-row-price">${escape(formatPrice(signal.current_price))}</span>${sixIndicators ? `<span class="signal-row-score">${scoreState?.tone === 'stale' ? 'Last recorded total' : 'Weighted total'} <strong>${scoreValue}</strong><span aria-hidden="true"> / 60</span> · six /10 indicators</span>` : `<span class="signal-row-score">${escape(scoreLabel)} <strong>${scoreValue}</strong><span aria-hidden="true"> / ${Number.isFinite(scoreScaleMax) ? scoreScaleMax : 10}</span></span>`}${regimeSummary}${autocorrelationSummary}</span>
                ${exitCue ? `<span class="signal-exit-cue"><strong>${escape(exitText)}</strong><small>${escape(exitExplanation)}</small></span>` : ''}
                ${totalStrength ? `<span class="signal-total-strength signal-total-strength-${totalStrength.tone}" title="${escape(totalStrength.description)}">${escape(totalStrength.label)}<small>${escape(totalStrength.description)}</small></span>` : ''}
                <span class="signal-win-probability${hasWinProbability ? '' : ' is-unavailable'}" title="${escape(winProbability.basis || winProbability.reason || 'Estimate uses after-fee holdout outcomes matched to the active score profile.')}"><strong>${escape(winProbabilityText)}</strong><small>${escape(winProbabilityDetail)}</small></span>
                ${afterFeeMetricText ? `<span class="signal-after-fee-metric" title="${escape(hasAfterFeeMetric ? 'Historical product-scoped cohort evidence; the separate score-matched win estimate is shown above.' : 'After-fee status is informational; it does not filter signals.')}">${escape(afterFeeMetricText)}<small>${hasAfterFeeMetric ? 'Product-scoped cohort holdout evidence; the win estimate above is matched to the signal score.' : 'This signal remains visible without a validated profile.'}</small></span>` : ''}
                ${renderScoreBasis(signal)}
                <span class="signal-row-footer"><span>${escape(evidence)}</span><span>Price ${escape(signalPriceBarAgeLabel(signal.price_bar_time))}</span><span>Scanned ${escape(ageLabel(signal.last_updated_at))}</span></span>
                ${provenance.length ? `<span class="signal-row-provenance">${escape(provenance.join(' · '))}</span>` : ''}
            </button>
        </article>`;
    };
}
