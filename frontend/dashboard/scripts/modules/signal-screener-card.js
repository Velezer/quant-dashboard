import { signalScoreBalance, hasSixIndicatorScores, oppositeEntryExitCue, signalDirectionalScore, signalIndicatorScores, signalIndicatorSummary, signalTotalStrength, signedDirectionScore } from './signal-screener-model.js';
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
            ? `<span class="signal-row-regime-score" title="Markov regime contributes a fixed 10-point weight to the current /60 scoring model"><span>Regime</span><strong>${markovScore.points === null ? '—' : signedDirectionScore(markovScore.points)}/10</strong><small>${escape(markovScore.state.replaceAll('_', ' ').toLowerCase())}</small></span>`
            : currentModelScaleMax === 60
                ? presentation.legacy && presentation.scoreScaleMax < 60
                    ? `<span class="signal-row-regime-score signal-row-regime-legacy" title="This saved row uses the previous scoring model and has no recorded Markov regime score"><span>Regime</span><strong>Not scored</strong><small>legacy /${presentation.scoreScaleMax} row</small></span>`
                    : `<span class="signal-row-regime-score signal-row-regime-unavailable" title="This row uses the current /60 scoring model, but its saved regime component is unavailable"><span>Regime</span><strong>Unavailable</strong><small>saved score basis</small></span>`
                : '';
        const autocorrelationRho = Number(autocorrelationScore?.value);
        const autocorrelationState = String(autocorrelationScore?.state || 'UNAVAILABLE').toUpperCase();
        const autocorrelationAlignment = autocorrelationScore?.points === null || !autocorrelationScore
            || autocorrelationState === 'UNAVAILABLE' ? 'unavailable'
            : Math.abs(autocorrelationScore.points) < 0.5 ? 'neutral'
                : (autocorrelationScore.points > 0) === (signal.direction === 'LONG') ? 'aligned' : 'opposed';
        const autocorrelationRelation = autocorrelationAlignment === 'opposed'
            ? `Against ${signal.direction}`
            : autocorrelationAlignment === 'aligned'
                ? `Supports ${signal.direction}`
                : autocorrelationAlignment === 'neutral' ? 'No clear direction' : 'Unavailable';
        const autocorrelationTendency = autocorrelationState.includes('MEAN_REVERSION')
            ? 'mean-reversion tendency'
            : autocorrelationState.includes('CONTINUATION') ? 'continuation tendency' : '';
        const autocorrelationExplanation = autocorrelationAlignment === 'opposed'
            ? `Opposes ${signal.direction}. Its signed contribution offsets the other indicators in the total. The total sign determines the final direction.`
            : autocorrelationAlignment === 'aligned'
                ? `Supports ${signal.direction} as one of six score inputs. It adds to the total but is not a standalone signal or a win guarantee.`
                : autocorrelationAlignment === 'neutral'
                    ? 'Near-neutral forecast contributes around zero. Positive points support LONG and negative points support SHORT.'
                    : 'No reading: this factor contributes 0/10, and the other factors keep their weights.';
        const autocorrelationSummary = autocorrelationScore
            ? `<span class="signal-row-regime-score" title="${escape(`${autocorrelationExplanation} ρ is the lag-one return correlation; the /10 points come from a causal forecast: positive is bullish and negative is bearish.`)}"><span>Autocorrelation · 1 of 6</span><strong>${autocorrelationScore.points === null ? '—' : `${signedDirectionScore(autocorrelationScore.points)}/10 · ${autocorrelationRelation}`}</strong><small>${Number.isFinite(autocorrelationRho) ? `ρ ${autocorrelationRho >= 0 ? '+' : ''}${autocorrelationRho.toFixed(2)}` : 'ρ unavailable'}${autocorrelationTendency ? ` · ${autocorrelationTendency}` : ''}</small></span>`
            : '';
        const autocorrelationGuidance = autocorrelationScore
            ? `<span class="signal-autocorrelation-guidance is-${autocorrelationAlignment}" role="note">${escape(autocorrelationExplanation)}</span>`
            : '';
        const afterFeeGate = signal.score_basis?.after_fee_gate;
        const gateHoldout = afterFeeGate?.holdout || {};
        const gateWinRate = Number(gateHoldout.win_rate_after_fees_pct);
        const gateMeanNet = Number(gateHoldout.mean_net_return_pct);
        const gateProfitFactor = Number(gateHoldout.profit_factor);
        const gateTrades = Number(gateHoldout.trades);
        const gateMinimumScore = Number(afterFeeGate?.minimum_score_out_of_60);
        const hasAfterFeeMetric = Number.isFinite(gateMeanNet) && Number.isFinite(gateProfitFactor)
            && Number.isFinite(gateTrades) && Number.isFinite(gateMinimumScore);
        const afterFeeMetricText = sixIndicators && (hasAfterFeeMetric || signal.persistent_score !== true)
            ? hasAfterFeeMetric
                ? `Product holdout ${gateMeanNet >= 0 ? '+' : ''}${gateMeanNet.toFixed(3)}% mean net/trade · PF ${gateProfitFactor.toFixed(2)} · ${Math.trunc(gateTrades)} trades · win rate ${Number.isFinite(gateWinRate) ? `${gateWinRate.toFixed(1)}% (descriptive)` : 'unavailable'} · profile cutoff ${gateMinimumScore.toFixed(0)}/60 is informational, not a signal filter`
                : 'No promoted after-fee profile · equal weights used · signal shown without a profile filter'
            : '';
        const afterFeeAccessibleText = hasAfterFeeMetric
            ? `${afterFeeMetricText}. This is product-scoped cohort evidence; the setup-strength-matched win estimate is shown separately.`
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
        const hasWinProbability = ['ESTIMATED', 'PROFILE_GATE_ESTIMATE', 'TICKER_HOLDOUT_RATE'].includes(winProbabilityStatus)
            && Number.isFinite(winProbabilityPct) && Number.isFinite(winProbabilityTrades)
            && winProbabilityTrades > 0;
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
            ? `${winProbabilityStatus === 'TICKER_HOLDOUT_RATE'
                ? 'Ticker holdout win rate'
                : winProbabilityStatus === 'ESTIMATED' ? 'Estimated win probability' : 'Profile-gate win estimate'} ${winProbabilityPct.toFixed(1)}%`
            : `After-fee win rate unavailable · ${winProbabilityStatus === 'INSUFFICIENT_SAMPLE'
                ? `${Number.isFinite(winProbabilityTrades) ? Math.trunc(winProbabilityTrades) : 0} matching outcomes; at least 30 required`
                : winProbabilityStatus === 'BELOW_PROFILE_GATE'
                    ? 'no setup-strength-matched profile estimate; checking exact-ticker holdout'
                    : String(winProbability.reason || 'run the exact ticker and timeframe backtest to populate this metric')}`;
        const winProbabilityDetail = hasWinProbability
            ? winProbabilityStatus === 'TICKER_HOLDOUT_RATE'
                ? `${probabilityRange} · ${Math.trunc(winProbabilityTrades)} exact-ticker after-fee holdout outcomes · not matched to setup strength · long/short pooled`
                : `${probabilityRange} · ${Math.trunc(winProbabilityTrades)} after-fee holdout outcomes${scoreBand} · long/short pooled${winProbabilityStatus === 'PROFILE_GATE_ESTIMATE' ? ` · ${Number.isFinite(scoreBandSampleTrades) ? `${Math.trunc(scoreBandSampleTrades)} score-band outcomes; profile-gate cohort used` : 'score-band sample unavailable; profile-gate cohort used'}` : ''}`
            : String(winProbability.reason || winProbability.basis || 'Refresh the backtest profile to build a matching estimate.');
        const winProbabilityAccessibleText = hasWinProbability
            ? `${winProbabilityText}, ${Math.trunc(winProbabilityTrades)} after-fee holdout outcomes${probabilityRange}${winProbabilityStatus === 'TICKER_HOLDOUT_RATE' ? ', not matched to setup strength' : scoreBand}.`
            : `${winProbabilityText}.`;
        const totalStrength = signalTotalStrength(signal, scoreModel);
        const displayScore = signalDisplayScore(signal);
        const scoreLabel = scoreState?.tone === 'stale' ? 'Last recorded setup strength' : 'Setup strength';
        const scoreValue = Number.isFinite(displayScore) ? displayScore.toFixed(1) : '—';
        const scoreStateText = scoreState
            ? String(scoreState.label || 'Signal status unavailable')
            : '';
        const scoreStateLabel = scoreState ? `, ${scoreStateText.toLowerCase()}` : '';
        const componentSummary = sixIndicators
            ? indicatorScores.map(item => `${item.indicator}: ${item.state}`).join('; ')
            : Array.isArray(signal.score_basis?.components) ? signal.score_basis.components.map(component =>
                `${component.indicator || 'Indicator'} ${component.state || 'state unavailable'}, ${Number(component.points || 0).toFixed(1)} points`,
            ).join('; ')
            : '';
        const entryStatus = scoreState?.current === false ? 'no current market-entry signal' : 'current technical market-entry signal';
        const accessibleScore = sixIndicators
            ? `${Number.isFinite(displayScore) ? `${scoreValue} out of ${scoreScaleMax} setup strength; ` : ''}${signalIndicatorSummary(signal, scoreModel)}`
            : Number.isFinite(displayScore) ? `${scoreValue} out of ${scoreScaleMax}` : 'unavailable';
        const balance = signalScoreBalance(signal, scoreModel);
        const balanceText = balance ? `Bullish +${balance.bullish.toFixed(1)} · bearish −${balance.bearish.toFixed(1)} · ${balance.cancellationPct.toFixed(0)}% cancellation` : '';
        const directionScore = signalDirectionalScore(signal);
        const directionScoreTone = directionScore?.score > 0 ? 'long'
            : directionScore?.score < 0 ? 'short' : 'neutral';
        const directionScoreText = directionScore
            ? directionScore.lastKnown
                ? `Last ${signal.direction} ${signedDirectionScore(directionScore.score)} · no current net`
                : directionScore.netUnavailable
                    ? `${signal.direction} ${signedDirectionScore(directionScore.score)} · net unavailable`
                    : `${signal.direction} ${signedDirectionScore(directionScore.score)} · Net ${signedDirectionScore(directionScore.netScore)} · ${directionScore.longCount}L / ${directionScore.shortCount}S`
            : '';
        const directionScoreMarkup = directionScore
            ? `<span class="signal-row-direction-score signal-row-direction-score-${directionScoreTone}${directionScore.lastKnown ? ' signal-row-direction-score-last-known' : ''}" title="${escape(directionScore.lastKnown
                ? `Last saved ${signal.direction} direction score ${signedDirectionScore(directionScore.score)}. It is stale or held and is excluded from the current ticker/timeframe net.`
                : directionScore.netUnavailable
                    ? `Signed direction score ${signedDirectionScore(directionScore.score)} for ${signal.direction}. The current ticker/timeframe net is unavailable.`
                    : `Direction score ${signedDirectionScore(directionScore.score)} for this ${signal.direction} signal. Net sums current ${signal.symbol} ${signal.timeframe.toUpperCase()} entries across products in the selected app. Setup strength and win-rate evidence stay separate.`)}">${escape(directionScoreText)}</span>`
            : '';
        const directionScoreAccessible = directionScore
            ? directionScore.lastKnown
                ? `Last saved signed ${signal.direction} direction ${signedDirectionScore(directionScore.score)}; current net is unavailable because this row is stale or held. `
                : directionScore.netUnavailable
                    ? `${signal.direction} signed direction score ${signedDirectionScore(directionScore.score)}; the current net is unavailable. `
                    : `Signed direction score ${signedDirectionScore(directionScore.score)} for ${signal.direction}; net ${signedDirectionScore(directionScore.netScore)} from ${directionScore.longCount} long and ${directionScore.shortCount} short current entries in the selected app. `
            : '';
        const scoreAccessibleLabel = `${rank ? `Entry ${rank}, ` : ''}${signal.symbol}, ${directionLabel}, ${signal.timeframe}, ${entryStatus}, ${sixIndicators ? 'indicator scores' : scoreLabel.toLowerCase()} ${accessibleScore}${scoreStateLabel}. Setup-strength weights: ${signalScoreWeightSummary(signal, scoreModel)}. ${componentSummary}. ${autocorrelationScore ? `${autocorrelationExplanation} ` : ''}${totalStrength ? `${totalStrength.label}. ${totalStrength.description} ` : ''}${afterFeeAccessibleText ? `${afterFeeAccessibleText}. ` : ''}${winProbabilityAccessibleText} Setup strength is separate from the signed direction score; win-rate evidence is historical, not a guarantee or execution probability. ${exitText ? `${exitText}. ${exitExplanation} ` : ''}Select to inspect chart.`;
        const productLabel = signal.product_id === 'stockbit_regular'
            ? ''
            : signal.product_id === 'pluang_commodities'
                ? 'Commodities'
                : String(signal.product_id || '').replaceAll('_', ' ');
        const strategyLabel = scoreModel?.short_label || signal.signal_strategy_name;
        const rowContext = [appLabel(signal.exchange_id), productLabel, strategyLabel]
            .filter(Boolean)
            .join(' · ');
        const provenance = [signal.data_provider, signal.data_source, signal.data_quality]
            .filter(value => String(value || '').trim() && value !== 'unknown')
            .map(value => String(value).replaceAll('_', ' '));
        const compactState = scoreState?.current === false
            ? scoreStateText
            : scoreState?.tone === 'provisional' ? scoreStateText || 'Provisional' : '';
        const compactWinRate = hasWinProbability ? `After-fee win ${winProbabilityPct.toFixed(1)}%` : 'Unvalidated · win rate unavailable';
        const riskRewardPlan = signal.risk_reward_plan || signal.score_basis?.risk_reward_plan || {};
        const riskRewardPlanStatus = String(riskRewardPlan.status || 'UNAVAILABLE');
        const riskRewardMinimum = Number(riskRewardPlan.minimum_target_after_fee_r_multiple);
        const riskRewardMultiple = Number(riskRewardPlan.planned_after_fee_r_multiple);
        const riskRewardMinimumLabel = Number.isFinite(riskRewardMinimum) && riskRewardMinimum > 0
            ? riskRewardMinimum.toFixed(2).replace(/\.?0+$/, '')
            : null;
        const riskRewardPlanReady = ['PLANNED', 'REFERENCE_ONLY'].includes(riskRewardPlanStatus)
            && riskRewardPlan.fees_applied === true
            && riskRewardMinimumLabel !== null
            && Number.isFinite(riskRewardMultiple)
            && riskRewardMultiple >= riskRewardMinimum;
        const riskRewardPlanLabel = riskRewardPlanReady
            ? riskRewardPlanStatus === 'REFERENCE_ONLY' ? `1:${riskRewardMinimumLabel} reference plan` : `1:${riskRewardMinimumLabel} net plan`
            : riskRewardPlanStatus === 'FEE_POLICY_UNAVAILABLE' ? 'fee plan unavailable' : '1:4 plan unavailable';
        const riskRewardPlanTitle = String(riskRewardPlan.reason || 'A planned bracket does not guarantee future execution or profit.');
        const compactScore = directionScore ? signedDirectionScore(directionScore.score) : '—';
        const compactContext = rowContext ? `${rowContext}, ` : '';
        const compactLabel = `${compactContext}${signal.symbol}, ${signal.timeframe}, ${directionLabel}, direction score ${compactScore}, price ${formatPrice(signal.current_price)}, ${riskRewardPlanLabel}, ${compactWinRate}${compactState ? `, ${compactState}` : ''}. Select chart.`;
        return `<article class="signal-row-wrap" role="listitem">
            <button type="button" class="signal-row${totalStrength?.tone === 'low' ? ' is-low-total-strength' : ''}${selected ? ' is-selected' : ''}" data-signal-id="${escape(signal.signal_id)}" aria-pressed="${selected}" aria-label="${escape(compactLabel)}">
                <span class="signal-row-top"><span class="signal-row-symbol">${escape(signal.symbol)}</span><span class="signal-row-timeframe">${escape(signal.timeframe.toUpperCase())}</span></span>
                <span class="signal-row-meta">${escape(rowContext)}</span>
                <span class="signal-row-main"><span class="${directionClass}">${escape(directionLabel)}</span><span class="signal-row-direction-score signal-row-direction-score-${directionScoreTone}">${escape(compactScore)}</span><span class="signal-row-price">${escape(formatPrice(signal.current_price))}</span></span>
                <span class="signal-row-footer"><span>${escape(compactWinRate)}</span><span class="signal-risk-reward-chip${riskRewardPlanReady ? ' is-ready' : ''}${riskRewardPlanStatus === 'REFERENCE_ONLY' ? ' is-reference' : ''}" title="${escape(riskRewardPlanTitle)}">${escape(riskRewardPlanLabel)}</span><span>Price ${escape(signalPriceBarAgeLabel(signal.price_bar_time))}</span></span>
                ${compactState ? `<span class="signal-score-state signal-score-state-${scoreState.tone}">${escape(compactState)}</span>` : ''}
            </button>
            <details class="signal-card-details">
                <summary aria-label="Details for ${escape(rowContext)} ${escape(signal.symbol)} ${escape(signal.timeframe)}">Details</summary>
                <div class="signal-card-detail-body">
                    <p>${escape(scoreAccessibleLabel)}</p>
                    ${directionScoreMarkup}
                    <span class="signal-row-score">${escape(scoreLabel)} <strong>${scoreValue}</strong> / ${scoreScaleMax}</span>
                    ${regimeSummary}${autocorrelationSummary}
                ${balance ? `<span class="signal-row-provenance" title="Opposing contributions cancel. This describes indicator disagreement, not win probability. NWE and DSS profiles share these factors; matching rows are not independent confirmations.">${escape(balanceText)}</span>` : ''}
                ${autocorrelationGuidance}
                ${exitCue ? `<span class="signal-exit-cue"><strong>${escape(exitText)}</strong><small>${escape(exitExplanation)}</small></span>` : ''}
                ${totalStrength ? `<span class="signal-total-strength signal-total-strength-${totalStrength.tone}" title="${escape(totalStrength.description)}">${escape(totalStrength.label)}<small>${escape(totalStrength.description)}</small></span>` : ''}
                <span class="signal-win-probability${hasWinProbability ? '' : ' is-unavailable'}" title="${escape(winProbability.basis || winProbability.reason || 'Estimate uses after-fee holdout outcomes matched to the active setup-strength profile.')}"><strong>${escape(winProbabilityText)}</strong><small>${escape(winProbabilityDetail)}</small></span>
                ${afterFeeMetricText ? `<span class="signal-after-fee-metric" title="${escape(hasAfterFeeMetric ? 'Historical product-scoped net-return and profit-factor evidence; the separate setup-strength-matched win estimate is shown above.' : 'After-fee status is informational; it does not filter signals.')}">${escape(afterFeeMetricText)}<small>${hasAfterFeeMetric ? 'Product-scoped cohort holdout evidence; the win estimate above is matched to setup strength.' : 'This signal remains visible without a validated profile.'}</small></span>` : ''}
                ${renderScoreBasis(signal)}
                <span class="signal-row-footer"><span>${escape(evidence)}</span><span>Price ${escape(signalPriceBarAgeLabel(signal.price_bar_time))}</span><span>Scanned ${escape(ageLabel(signal.last_updated_at))}</span></span>
                ${provenance.length ? `<span class="signal-row-provenance">${escape(provenance.join(' · '))}</span>` : ''}
                </div>
            </details>
        </article>`;
    };
}
