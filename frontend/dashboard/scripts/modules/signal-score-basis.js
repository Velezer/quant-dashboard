import { scoreWeightLabel, signalScoreComponentWeights, signalScorePresentation } from './signal-screener-model.js';

export function createSignalScoreBasisMarkup({ escape, scoreModelFor }) {
    function componentLabel(component) {
        const indicator = String(component.indicator || 'Indicator');
        const state = String(component.state || 'state unavailable').replaceAll('_', ' ').toLowerCase();
        const points = Number(component.points);
        const pointsLabel = Number.isFinite(points) ? ` · ${points.toFixed(1)} pts` : '';
        const value = Number(component.value);
        const valueLabel = Number.isFinite(value)
            ? indicator.startsWith('Relative Volume')
                ? ` · ${value.toFixed(2)}× baseline`
                : indicator.includes('Delta')
                    ? ` · close-location ${value.toFixed(2)}`
                    : ''
            : '';
        return `${indicator}: ${state}${pointsLabel}${valueLabel}`;
    }

    function scoreBasisMarkup(signal) {
        const basis = signal.score_basis || {};
        const scoreModel = scoreModelFor(signal);
        const presentation = signalScorePresentation(signal, scoreModel);
        const componentScaleMax = presentation.componentScoreScaleMax;
        const indicatorComponentScaleMax = presentation.indicatorComponentScoreScaleMax;
        const components = Array.isArray(basis.components) ? basis.components : [];
        const chips = components.length
            ? components.map(component => {
                const points = Number(component.points);
                const pointsScale = Number.isFinite(indicatorComponentScaleMax)
                    ? ` ${Number.isFinite(points) ? points.toFixed(1) : '—'}/${indicatorComponentScaleMax}`
                    : '';
                return `<span class="signal-component-chip">${escape(componentLabel(component))}${escape(pointsScale)}</span>`;
            }).join('')
            : '<span class="signal-component-chip signal-component-unavailable">Score component details unavailable</span>';
        const strategyStrengthValue = basis.strategy_strength === null || basis.strategy_strength === undefined || basis.strategy_strength === ''
            ? null
            : Number(basis.strategy_strength);
        const confluenceValue = basis.confluence_score === null || basis.confluence_score === undefined || basis.confluence_score === ''
            ? null
            : Number(basis.confluence_score);
        const strategyScoreLabel = String(scoreModel?.strategy_score_label || 'Strategy');
        const confluenceScoreLabel = String(scoreModel?.confluence_score_label || 'Confluence');
        const componentSuffix = Number.isFinite(componentScaleMax) ? `/${componentScaleMax}` : '';
        const weights = signalScoreComponentWeights(signal, scoreModel);
        if (Number.isFinite(presentation.indicatorComponentScoreScaleMax)) {
            const priceLabel = `${strategyScoreLabel} ${Number.isFinite(strategyStrengthValue) ? strategyStrengthValue.toFixed(1) : '—'}/${componentScaleMax}`;
            const volumeLabel = `${confluenceScoreLabel} ${Number.isFinite(confluenceValue) ? confluenceValue.toFixed(1) : '—'}/${componentScaleMax}`;
            return `<div class="signal-score-basis"${scoreModel?.score_basis_description ? ` title="${escape(scoreModel.score_basis_description)}"` : ''}><div class="signal-basis-values">${escape(priceLabel)} (${scoreWeightLabel(weights.strategy)}) <span aria-hidden="true">·</span> ${escape(volumeLabel)} (${scoreWeightLabel(weights.confluence)})</div><div class="signal-component-list">${chips}</div></div>`;
        }
        const strategyStrength = Number.isFinite(strategyStrengthValue)
            ? `${strategyScoreLabel} ${strategyStrengthValue.toFixed(1)}${componentSuffix}`
            : 'Strategy score unavailable';
        const confluence = Number.isFinite(confluenceValue)
            ? `${confluenceScoreLabel} ${confluenceValue.toFixed(1)}${componentSuffix}`
            : 'Confluence unavailable';
        if (presentation.scoreScaleMax === 20 || presentation.scoreScaleMax === 40) {
            const conciseComponentLabel = value => String(value)
                .replace(/\s+(strategy|confluence)$/i, '');
            const componentsSummary = `${conciseComponentLabel(strategyScoreLabel)} ${Number.isFinite(strategyStrengthValue) ? strategyStrengthValue.toFixed(1) : '—'}/${componentScaleMax} (${scoreWeightLabel(weights.strategy)})`
                + ` · ${conciseComponentLabel(confluenceScoreLabel)} ${Number.isFinite(confluenceValue) ? confluenceValue.toFixed(1) : '—'}/${componentScaleMax} (${scoreWeightLabel(weights.confluence)})`;
            return `<div class="signal-score-basis"${scoreModel?.score_basis_description ? ` title="${escape(scoreModel.score_basis_description)}"` : ''}><div class="signal-basis-values">${escape(componentsSummary)}</div><details class="signal-score-details"><summary>Details</summary><div class="signal-component-list">${chips}</div></details></div>`;
        }
        const legacyLabel = presentation.legacy
            ? ` · legacy ${presentation.scoreScaleMax}-point score model retained`
            : '';
        const scoreBasisDescription = String(scoreModel?.score_basis_description || '').trim();
        return `<div class="signal-score-basis"${scoreBasisDescription ? ` title="${escape(scoreBasisDescription)}"` : ''}><div class="signal-basis-values">${escape(strategyStrength)} (${scoreWeightLabel(weights.strategy)}) <span aria-hidden="true">·</span> ${escape(confluence)} (${scoreWeightLabel(weights.confluence)})${escape(legacyLabel)}</div><div class="signal-component-list">${chips}</div></div>`;
    }

    return scoreBasisMarkup;
}
