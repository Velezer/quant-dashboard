import { hasFourIndicatorScores, signalIndicatorScores, signalIndicatorSummary } from './signal-screener-model.js';
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
        const directionClass = signal.direction === 'SHORT' ? 'signal-direction-short' : 'signal-direction-long';
        const directionLabel = scoreState && !scoreState.current ? `LAST ${signal.direction}` : signal.direction;
        const evidence = String(signal.evidence_status || 'VALIDATION_PENDING').replaceAll('_', ' ');
        const scoreModel = scoreModelFor(signal);
        const presentation = signalScorePresentation(signal, scoreModel);
        const scoreScaleMax = Number.isFinite(presentation.scoreScaleMax)
            ? presentation.scoreScaleMax
            : 10;
        const fourIndicators = hasFourIndicatorScores(signal, scoreModel);
        const displayScore = signalDisplayScore(signal);
        const scoreLabel = scoreState?.tone === 'stale' ? 'Last recorded score' : 'Market-entry score';
        const scoreValue = Number.isFinite(displayScore) ? displayScore.toFixed(1) : '—';
        const scoreStateLabel = scoreState ? `, ${scoreState.label.toLowerCase()}` : '';
        const componentSummary = fourIndicators
            ? signalIndicatorScores(signal, scoreModel).map(item => `${item.indicator}: ${item.state}`).join('; ')
            : Array.isArray(signal.score_basis?.components) ? signal.score_basis.components.map(component =>
                `${component.indicator || 'Indicator'} ${component.state || 'state unavailable'}, ${Number(component.points || 0).toFixed(1)} points`,
            ).join('; ')
            : '';
        const entryStatus = scoreState?.current === false ? 'no current market-entry signal' : 'current technical market-entry signal';
        const accessibleScore = fourIndicators ? signalIndicatorSummary(signal, scoreModel) : Number.isFinite(displayScore)
            ? `${scoreValue} out of ${scoreScaleMax}`
            : 'unavailable';
        const scoreAccessibleLabel = `${rank ? `Entry ${rank}, ` : ''}${signal.symbol}, ${directionLabel}, ${signal.timeframe}, ${entryStatus}, ${fourIndicators ? 'indicator scores' : scoreLabel.toLowerCase()} ${accessibleScore}${scoreStateLabel}. Score weights: ${signalScoreWeightSummary(signal, scoreModel)}. ${componentSummary}. Setup strength is not win probability or execution availability. Select to inspect chart.`;
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
            <button type="button" class="signal-row${selected ? ' is-selected' : ''}" data-signal-id="${escape(signal.signal_id)}" aria-pressed="${selected}" aria-label="${escape(scoreAccessibleLabel)}">
                <span class="signal-row-top"><span class="signal-row-rank">#${rank}</span><span class="signal-row-symbol">${escape(signal.symbol)}</span><span class="signal-row-timeframe">${escape(signal.timeframe.toUpperCase())}</span></span>
                <span class="signal-row-meta">${escape(rowContext)}</span>
                ${scoreState ? `<span class="signal-score-state signal-score-state-${scoreState.tone}" role="status">${escape(scoreState.label)} · strategy bar ${escape(signalPriceBarAgeLabel(signal.score_bar_time))}</span>` : '<span class="signal-score-state signal-score-state-current" role="status">MARKET ENTRY NOW · FRESH PRICE</span>'}
                <span class="signal-row-main"><span class="${directionClass}">${escape(directionLabel)}</span><span class="signal-row-price">${escape(formatPrice(signal.current_price))}</span>${fourIndicators ? `<span class="signal-row-score">${scoreState?.tone === 'stale' ? 'Last recorded indicators' : '4 indicator scores'} · 25% each</span>` : `<span class="signal-row-score">${escape(scoreLabel)} <strong>${scoreValue}</strong><span aria-hidden="true"> / ${Number.isFinite(scoreScaleMax) ? scoreScaleMax : 10}</span></span>`}</span>
                ${renderScoreBasis(signal)}
                <span class="signal-row-footer"><span>${escape(evidence)}</span><span>Price ${escape(signalPriceBarAgeLabel(signal.price_bar_time))}</span><span>Scanned ${escape(ageLabel(signal.last_updated_at))}</span></span>
                ${provenance.length ? `<span class="signal-row-provenance">${escape(provenance.join(' · '))}</span>` : ''}
            </button>
        </article>`;
    };
}
