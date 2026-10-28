export const APP_LABELS = {
    stockbit: 'Stockbit',
    pintu: 'Pintu',
    pluang: 'Pluang',
    tokocrypto: 'Tokocrypto',
};
export const PRODUCTS_BY_APP = {
    stockbit: ['stockbit_regular'],
    pintu: ['pintu_retail_spot', 'pintu_pro_spot', 'pintu_pro_futures'],
    pluang: ['pluang_spot', 'pluang_perpetual'],
    tokocrypto: ['tokocrypto_spot', 'tokocrypto_tokenized_stock'],
};
const SIGNAL_BAR_TIME_NANOSECONDS_PER_SECOND = 1_000_000_000;

export function signalIdentity(signal) {
    return [signal.exchange_id, signal.product_id, signal.asset_category_id,
        signal.symbol, signal.signal_strategy_id, signal.timeframe].join('|');
}

export function signalScoreState(signal) {
    if (signal?.persistent_score === true) {
        if (signal.score_status === 'stale') {
            return { label: 'STALE · NO MARKET ENTRY · HELD', tone: 'stale', current: false };
        }
        if (signal.score_status === 'not_qualifying') {
            return { label: 'NO MARKET ENTRY · SCORE 0', tone: 'inactive', current: false };
        }
        const timeframe = String(signal.timeframe || '').toUpperCase();
        return signal.score_bar_is_forming
            ? { label: `MARKET ENTRY NOW · PROVISIONAL ${timeframe}`, tone: 'provisional', current: true }
            : { label: `MARKET ENTRY NOW · LATEST CLOSED ${timeframe}`, tone: 'current', current: true };
    }
    if (!signalPriceIsFresh(signal)) {
        return { label: 'SETUP RETAINED · STALE PRICE · NOT A MARKET ENTRY', tone: 'stale', current: false };
    }
    return { label: 'MARKET ENTRY NOW · FRESH PRICE', tone: 'current', current: true };
}

export function signalStrategyLabel(signal, strategies) {
    const strategy = (Array.isArray(strategies) ? strategies : []).find(item =>
        String(item.id || '') === String(signal?.signal_strategy_id || ''),
    );
    return String(strategy?.short_label || strategy?.label || signal?.signal_strategy_name || 'Signal model');
}

export function normalizeSignalRecord(raw, strategies) {
    const exchange = String(raw.exchange_id || '').toLowerCase();
    const symbol = String(raw.symbol || '').trim();
    const storageSymbol = String(raw.storage_symbol || symbol).trim();
    const timeframe = String(raw.timeframe || '').toLowerCase();
    const category = String(raw.asset_category_id || '').toLowerCase();
    const product = String(raw.product_id || '').toLowerCase();
    const signal = {
        ...raw,
        signal_id: signalIdentity({ ...raw, exchange_id: exchange, symbol, timeframe }),
        asset: symbol,
        symbol,
        data_symbol: storageSymbol,
        Data_Symbol: storageSymbol,
        exchange,
        venue: exchange,
        class: category === 'idx_stock'
            ? 'idx'
            : category === 'tokenized_stock'
                ? 'tokenized_stock'
                : product.endsWith('futures')
                    ? 'crypto_futures'
                    : 'crypto',
        direction: String(raw.direction || '').toUpperCase(),
        signal_direction: String(raw.direction || '').toUpperCase(),
        strategy_id: String(raw.signal_strategy_id || ''),
        strategy: String(raw.signal_strategy_id || ''),
        signal_strategy_name: String(raw.signal_strategy_name || raw.signal_strategy_id || 'Signal model'),
        timeframe,
        current_price: Number(raw.current_price),
        chart_data_lazy: true,
    };
    signal.signal_strategy_name = signalStrategyLabel(signal, strategies);
    return signal;
}

export function ageLabelFromSeconds(seconds) {
    if (!Number.isFinite(seconds) || seconds < 0) return 'age unavailable';
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m ago`;
}

export function signalPriceBarAgeLabel(priceBarTimeNs, nowEpochMs = Date.now()) {
    const timestampNanoseconds = Number(priceBarTimeNs);
    if (!Number.isFinite(timestampNanoseconds) || timestampNanoseconds <= 0
        || !Number.isFinite(nowEpochMs) || nowEpochMs <= 0) {
        return 'age unavailable';
    }
    const priceBarEpochSeconds = Math.floor(
        timestampNanoseconds / SIGNAL_BAR_TIME_NANOSECONDS_PER_SECOND,
    );
    const nowEpochSeconds = Math.floor(nowEpochMs / 1000);
    return ageLabelFromSeconds(Math.max(0, nowEpochSeconds - priceBarEpochSeconds));
}

export function signalPriceIsFresh(signal, nowEpochMs = Date.now()) {
    if (signal?.persistent_score === true) return true;
    const freshUntilMs = Number(signal?.price_fresh_until_ms);
    if (!Number.isFinite(freshUntilMs) || freshUntilMs <= 0) return true;
    return Number.isFinite(nowEpochMs) && nowEpochMs <= freshUntilMs;
}

export const SIGNAL_LIST_PAGE_SIZE = 50;

export function signalRowsForDisplay(signals, limit = SIGNAL_LIST_PAGE_SIZE) {
    const pageLimit = Number.isInteger(limit) && limit > 0 ? limit : SIGNAL_LIST_PAGE_SIZE;
    return signals.slice(0, pageLimit);
}

export function signalPriceCoverageMessage(status, unavailableCount) {
    if (!['unavailable', 'partial_stale'].includes(String(status || ''))) return '';
    const count = Number(unavailableCount);
    if (!Number.isFinite(count) || count < 0) return '';
    const instrumentCount = Math.floor(count);
    return status === 'unavailable'
        ? `No fresh 15m price data was available for any of ${instrumentCount} registered instruments in the full scan universe.`
        : `Fresh 15m price data was unavailable for ${instrumentCount} registered instruments in the full scan universe.`;
}

export function signalScorePresentation(signal, model) {
    const basis = signal?.score_basis || {};
    const scoreScaleMax = Number(
        basis.score_scale_max ?? signal?.score_scale_max ?? model?.score_scale_max,
    );
    const basisIsCurrent = basis.score_model_status !== 'legacy'
        && (!basis.model_id || !model?.score_model_id || basis.model_id === model.score_model_id);
    const minimumKey = scoreScaleMax === 40
        ? 'minimum_score_out_of_40'
        : scoreScaleMax === 20
            ? 'minimum_score_out_of_20'
            : 'minimum_score_out_of_10';
    const minimumCandidate = scoreScaleMax >= 20
        ? basisIsCurrent
            ? model?.[minimumKey] ?? basis[minimumKey]
            : basis[minimumKey]
        : basis.minimum_score_out_of_10 ?? model?.minimum_score_out_of_10;
    const minimumScore = Number(minimumCandidate ?? basis.minimum_score);
    return {
        scoreScaleMax,
        minimumScore: Number.isFinite(minimumScore) ? minimumScore : null,
        componentScoreScaleMax: Number(
            basis.component_score_scale_max ?? model?.component_score_scale_max,
        ),
        indicatorComponentScoreScaleMax: Number(
            basis.indicator_component_score_scale_max
                ?? model?.indicator_component_score_scale_max
                ?? basis.component_score_scale_max
                ?? model?.component_score_scale_max,
        ),
        legacy: basis.score_model_status === 'legacy',
    };
}

export function signalScoreComponentWeights(signal, model) {
    const basis = signal?.score_basis || {};
    const presentation = signalScorePresentation(signal, model);
    const strategyWeight = Number(basis.strategy_weight ?? model?.strategy_weight);
    const confluenceWeight = Number(basis.confluence_weight ?? model?.confluence_weight);
    if (Number.isFinite(strategyWeight) && Number.isFinite(confluenceWeight)) {
        return { strategy: strategyWeight, confluence: confluenceWeight };
    }
    const scoreScaleMax = presentation.scoreScaleMax;
    const componentScaleMax = presentation.componentScoreScaleMax;
    if (scoreScaleMax === 20 && Number.isFinite(componentScaleMax) && componentScaleMax > 0) {
        const componentShare = componentScaleMax / scoreScaleMax;
        return { strategy: componentShare, confluence: componentShare };
    }
    return { strategy: null, confluence: null };
}

export function scoreWeightLabel(weight) {
    return Number.isFinite(weight) ? `${Math.round(weight * 100)}% weight` : 'weight unavailable';
}

export function hasFourIndicatorScores(signal, model) {
    const presentation = signalScorePresentation(signal, model);
    return presentation.scoreScaleMax === 40 && !presentation.legacy;
}

export function signalIndicatorScores(signal, model) {
    if (!hasFourIndicatorScores(signal, model)) return [];
    const names = [
        ['Nadaraya-Watson Envelope', 'NWE'], ['DSS Bressert', 'DSS Bressert'],
        ['Relative Volume', 'Relative volume'], ['Estimated Volume Delta (OHLCV proxy)', 'Delta proxy'],
    ];
    const components = Array.isArray(signal?.score_basis?.components) ? signal.score_basis.components : [];
    return names.map(([indicator, label]) => {
        const component = components.find(item => item?.indicator === indicator);
        const value = component?.points;
        const points = typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 10 ? value : null;
        return { indicator, label, points, scale: 10, weight: 1 / names.length,
            state: String(component?.state || 'UNAVAILABLE'), value: component?.value };
    });
}

// A presentation threshold only; registered model eligibility owns its own floor.
const LOW_TOTAL_INDICATOR_POINTS = 10;

export function signalTotalStrength(signal, model) {
    if (!hasFourIndicatorScores(signal, model)) return null;
    const indicators = signalIndicatorScores(signal, model);
    const known = indicators.every(item => item.points !== null);
    const total = known ? indicators.reduce((sum, item) => sum + item.points, 0) : null;
    const low = known && total < LOW_TOTAL_INDICATOR_POINTS;
    const label = !known ? 'Total strength unavailable' : low ? 'Low total strength' : `${LOW_TOTAL_INDICATOR_POINTS}+ combined points`;
    const retained = signalScoreState(signal)?.tone === 'stale';
    return {
        tone: !known ? 'unavailable' : low ? 'low' : 'standard',
        label: retained ? `Last recorded: ${label.toLowerCase()}` : label,
        description: !known ? 'All four numeric indicator contributions are needed to classify total strength.'
            : `The four indicator scores add up to ${low ? 'less than' : 'at least'} ${LOW_TOTAL_INDICATOR_POINTS} points (before rounding).`,
    };
}

export function signalIndicatorSummary(signal, model) {
    return signalIndicatorScores(signal, model).map(item =>
        `${item.label} ${item.points === null ? '—' : item.points.toFixed(1)}/10 (${scoreWeightLabel(item.weight)}${item.state.toUpperCase() === 'UNAVAILABLE' ? ', unavailable' : ''})`
    ).join(' · ');
}

export function signalScoreWeightSummary(signal, model) {
    if (hasFourIndicatorScores(signal, model)) return signalIndicatorScores(signal, model).map(item => `${item.label} ${Math.round(item.weight * 100)}%`).join(', ');
    const weights = signalScoreComponentWeights(signal, model);
    const strategyLabel = String(model?.strategy_score_label || 'Strategy');
    const confluenceLabel = String(model?.confluence_score_label || 'Confluence');
    const share = weight => Number.isFinite(weight) ? `${Math.round(weight * 100)}%` : 'unavailable';
    return `${strategyLabel} ${share(weights.strategy)}, ${confluenceLabel} ${share(weights.confluence)}`;
}

export function signalMeetsRenderMinimum(signal, model) {
    const { minimumScore } = signalScorePresentation(signal, model);
    const score = Number(signal?.score);
    return !Number.isFinite(minimumScore)
        || (Number.isFinite(score) && score >= minimumScore);
}

export function signalDisplayScore(signal) {
    if (signal?.persistent_score === true && signal.score_status === 'not_qualifying') return 0;
    const value = signal?.score;
    const isNumericInput = typeof value === 'number'
        || (typeof value === 'string' && value.trim() !== '');
    if (!isNumericInput) return null;
    const score = Number(value);
    return Number.isFinite(score) ? score : null;
}

export function signalScoreSummary(signal, model, scoreState = null) {
    const presentation = signalScorePresentation(signal, model);
    const scoreScaleMax = Number.isFinite(presentation.scoreScaleMax)
        ? presentation.scoreScaleMax
        : 10;
    const stateLabel = scoreState?.label || '';
    const state = scoreState
        ? ` · ${stateLabel.includes('PROVISIONAL')
            ? 'forming'
            : stateLabel.includes('CLOSED')
                ? 'closed'
                : stateLabel.toLowerCase()}`
        : '';
    if (hasFourIndicatorScores(signal, model)) {
        const label = scoreState?.tone === 'stale' ? 'Last recorded indicator scores' : 'Indicator scores';
        const strength = signalTotalStrength(signal, model);
        const indicators = `${label}: ${signalIndicatorSummary(signal, model)}${state}`;
        const summary = strength.tone === 'low' ? `${strength.label} · ${indicators}` : `${indicators} · ${strength.label}`;
        return `${summary} · ${strength.description} · setup strength, not win probability or execution availability`;
    }
    const displayScore = signalDisplayScore(signal);
    const scoreLabel = scoreState?.tone === 'stale'
        ? 'Last recorded setup score'
        : 'Market-entry setup score';
    const scoreValue = Number.isFinite(displayScore)
        ? `${displayScore.toFixed(1)}/${scoreScaleMax}`
        : 'unavailable';
    return `${scoreLabel} ${scoreValue}${state} · ${signalScoreWeightSummary(signal, model)} · setup strength, not win probability or execution availability`;
}

export function averageNormalizedScore(signals) {
    const scores = signals.map(signal => {
        const model = signalScorePresentation(signal, signal.scoreModel);
        const scaleMax = model.scoreScaleMax;
        const score = Number(signal.score);
        return Number.isFinite(scaleMax) && scaleMax > 0 && Number.isFinite(score)
            ? Math.max(0, Math.min(1, score / scaleMax))
            : null;
    }).filter(value => value !== null);
    return scores.length
        ? scores.reduce((sum, score) => sum + score, 0) / scores.length * 100
        : null;
}

export function hasScorePresentation(model) {
    const presentation = signalScorePresentation(null, model);
    return Number.isFinite(presentation.scoreScaleMax)
        && Number.isFinite(presentation.minimumScore);
}

export function cryptoSignalModelsForSelection(models, exchange, product = 'all', timeframe = 'all') {
    const selectedExchange = String(exchange || '').toLowerCase();
    const selectedProduct = String(product || 'all').toLowerCase();
    const selectedTimeframe = String(timeframe || 'all').toLowerCase();
    return (Array.isArray(models) ? models : []).filter(model => {
        const categories = Array.isArray(model.asset_category_ids) ? model.asset_category_ids : [];
        const exchanges = Array.isArray(model.exchange_ids) ? model.exchange_ids : [];
        const products = Array.isArray(model.product_ids) ? model.product_ids : [];
        const productMatches = selectedProduct === 'all'
            || categories.includes(selectedProduct)
            || !products.length
            || products.includes(selectedProduct);
        return categories.some(id => String(id).startsWith('crypto_'))
            && exchanges.includes(selectedExchange)
            && productMatches
            && (selectedTimeframe === 'all'
                || String(model.timeframe || '').toLowerCase() === selectedTimeframe)
            && hasScorePresentation(model)
            && Number.isFinite(Number(model.minimum_history_bars));
    });
}

export function stockbitSignalModelsForSelection(models, exchange, product = 'all', timeframe = 'all') {
    const selectedExchange = String(exchange || '').toLowerCase();
    const selectedProduct = String(product || 'all').toLowerCase();
    const selectedTimeframe = String(timeframe || 'all').toLowerCase();
    if (selectedExchange !== 'stockbit') return [];
    return (Array.isArray(models) ? models : []).filter(model =>
        Array.isArray(model.exchange_ids) && model.exchange_ids.includes(selectedExchange)
        && Array.isArray(model.product_ids) && model.product_ids.includes('stockbit_regular')
        && Array.isArray(model.asset_category_ids) && model.asset_category_ids.includes('idx_stock')
        && (selectedProduct === 'all'
            || model.product_ids.includes(selectedProduct)
            || model.asset_category_ids.includes(selectedProduct))
        && (selectedTimeframe === 'all'
            || String(model.timeframe || '').toLowerCase() === selectedTimeframe)
        && hasScorePresentation(model)
        && Number.isFinite(Number(model.minimum_history_bars))
    );
}

