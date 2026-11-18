export const SIGNED_DIRECTION_SCORE_CONTRACT = 'signed_indicator_sum_v1';

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
    const strategy = Array.isArray(signal.timeframe_strategy_ids) ? '' : signal.signal_strategy_id;
    return [signal.exchange_id, signal.product_id, signal.asset_category_id,
        signal.symbol, strategy, signal.timeframe].join('|');
}

export function signalScoreState(signal, nowEpochMs = Date.now()) {
    if (signal?.persistent_score === true) {
        if (signal.score_status === 'stale') {
            return { label: 'STALE · NO MARKET ENTRY · HELD', tone: 'stale', current: false };
        }
        if (signal.score_status === 'not_qualifying') {
            return { label: 'NO MARKET ENTRY · SCORE 0', tone: 'inactive', current: false };
        }
        const timeframe = String(signal.timeframe || '').toUpperCase();
        if (signal.market_entry_status === 'NO_MARKET_ENTRY') {
            return { label: `LATEST CLOSED ${timeframe} · NO MARKET ENTRY`, tone: 'inactive', current: false };
        }
        if (!['current', 'qualifying'].includes(signal.score_status)) {
            return { label: 'SCORE STATUS UNKNOWN · NO MARKET ENTRY', tone: 'unknown', current: false };
        }
        if (hasUnknownMarketEntryStatus(signal)) {
            return { label: 'ENTRY STATUS UNKNOWN · NO MARKET ENTRY', tone: 'unknown', current: false };
        }
        if (!signalPriceIsFresh(signal, nowEpochMs)) {
            return { label: 'STALE · NO MARKET ENTRY · HELD', tone: 'stale', current: false };
        }
        return signal.score_bar_is_forming
            ? { label: `MARKET ENTRY NOW · PROVISIONAL ${timeframe}`, tone: 'provisional', current: true }
            : { label: `MARKET ENTRY NOW · LATEST CLOSED ${timeframe}`, tone: 'current', current: true };
    }
    if (!signalPriceIsFresh(signal, nowEpochMs)) {
        if (signal?.score_basis?.price_reference_only === true) {
            const timeframe = String(signal.price_timeframe || signal.timeframe || '').toUpperCase();
            return {
                label: `SETUP · ${timeframe} CANDLE REFERENCE · NO MARKET ENTRY`,
                tone: 'stale',
                current: false,
            };
        }
        return { label: 'SETUP RETAINED · STALE PRICE · NOT A MARKET ENTRY', tone: 'stale', current: false };
    }
    if (signal?.market_entry_status === 'NO_MARKET_ENTRY') {
        const timeframe = String(signal.timeframe || '').toUpperCase();
        return { label: `LATEST ${timeframe} · NO MARKET ENTRY`, tone: 'inactive', current: false };
    }
    if (hasUnknownMarketEntryStatus(signal)) {
        return { label: 'ENTRY STATUS UNKNOWN · NO MARKET ENTRY', tone: 'unknown', current: false };
    }
    return { label: 'MARKET ENTRY NOW · FRESH PRICE', tone: 'current', current: true };
}

function hasUnknownMarketEntryStatus(signal) {
    const status = signal?.market_entry_status;
    return status != null && status !== '' && status !== 'MARKET_ENTRY';
}

// An opposite entry is an exit cue only for a matching open position.
// The dashboard has no position state, so this remains conditional.
export function oppositeEntryExitCue(signal, nowEpochMs = Date.now()) {
    if (!signal || signal.score_status !== 'qualifying'
        || signal.price_is_fresh === false || !signalScoreState(signal, nowEpochMs)?.current
        || !signalPriceIsFresh(signal, nowEpochMs)) return null;
    const freshUntil = Number(signal.price_fresh_until_ms);
    if (!Number.isFinite(freshUntil) || freshUntil <= 0 || !Number.isFinite(nowEpochMs) || nowEpochMs > freshUntil) return null;
    const direction = String(signal.direction || '').toUpperCase();
    if (direction !== 'LONG' && direction !== 'SHORT') return null;
    return {
        positionSide: direction === 'SHORT' ? 'LONG' : 'SHORT',
        signalDirection: direction,
        provisional: signal.score_bar_is_forming === true,
    };
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
        current_price: positiveFinitePrice(raw.current_price),
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
    if (signal?.price_is_fresh === false) return false;
    const freshUntilMs = Number(signal?.price_fresh_until_ms);
    if (!Number.isFinite(freshUntilMs) || freshUntilMs <= 0) return false;
    return Number.isFinite(nowEpochMs) && nowEpochMs > 0 && nowEpochMs <= freshUntilMs;
}

function positiveFinitePrice(value) {
    if (typeof value !== 'number' && (typeof value !== 'string' || value.trim() === '')) return null;
    const price = Number(value);
    return Number.isFinite(price) && price > 0 ? price : null;
}

export const SIGNAL_LIST_PAGE_SIZE = 50;

export function signalRowsForDisplay(signals, limit = SIGNAL_LIST_PAGE_SIZE) {
    const pageLimit = Number.isInteger(limit) && limit > 0 ? limit : SIGNAL_LIST_PAGE_SIZE;
    return signals.slice(0, pageLimit);
}

export function signalPriceCoverageMessage(status, unavailableCount) {
    const normalizedStatus = String(status || '').toLowerCase();
    if (normalizedStatus === 'ready') return '';
    if (!['unavailable', 'partial_stale'].includes(normalizedStatus)) {
        return 'Fresh 15m price coverage is not reported; current-entry readiness cannot be confirmed.';
    }
    const count = typeof unavailableCount === 'number'
        ? unavailableCount
        : typeof unavailableCount === 'string' && unavailableCount.trim() !== ''
            ? Number(unavailableCount)
            : Number.NaN;
    if (!Number.isSafeInteger(count) || count < 0) {
        return 'Fresh 15m price coverage is incomplete; the number of affected instruments was not reported.';
    }
    const instrumentCount = Math.floor(count);
    return normalizedStatus === 'unavailable'
        ? `No fresh 15m price data was available for any of ${instrumentCount} registered instruments in the full scan universe.`
        : `Fresh 15m price data was unavailable for ${instrumentCount} registered instruments in the full scan universe.`;
}

export function signalScorePresentation(signal, model) {
    const basis = signal?.score_basis || {};
    const scoreScaleMax = Number(
        signal?.setup_strength_scale_max
            ?? basis.setup_strength_scale_max
            ?? basis.score_scale_max
            ?? signal?.score_scale_max
            ?? model?.score_scale_max,
    );
    const basisIsCurrent = basis.score_model_status !== 'legacy'
        && (!basis.model_id || !model?.score_model_id || basis.model_id === model.score_model_id);
    const minimumKey = scoreScaleMax === 60
        ? 'minimum_score_out_of_60'
        : scoreScaleMax === 50
            ? 'minimum_score_out_of_50'
        : scoreScaleMax === 40
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

export function hasSixIndicatorScores(signal, model) {
    const presentation = signalScorePresentation(signal, model);
    return presentation.scoreScaleMax === 60 && !presentation.legacy;
}

export const SIGNAL_SCORE_INDICATORS = Object.freeze([
    { id: 'nwe', indicator: 'Nadaraya-Watson Envelope', label: 'NWE' },
    { id: 'dss_bressert', indicator: 'DSS Bressert', label: 'DSS Bressert' },
    { id: 'relative_volume', indicator: 'Relative Volume', label: 'Relative volume' },
    { id: 'delta_proxy', indicator: 'Estimated Volume Delta (OHLCV proxy)', label: 'Delta proxy' },
    { id: 'markov_regime', indicator: 'Markov Regime', label: 'Markov regime' },
    { id: 'lag_one_return_autocorrelation', indicator: 'Lag-one Return Autocorrelation', label: 'Autocorrelation' },
]);

function normalizedIndicatorIdentity(value) {
    return String(value || '').toLowerCase().replace(/[^a-z0-9]/g, '');
}

function componentMatchesIndicator(component, indicator) {
    const expected = normalizedIndicatorIdentity(indicator.indicator);
    const accepted = new Set([expected, normalizedIndicatorIdentity(indicator.id)]);
    if (indicator.id === 'markov_regime') {
        accepted.add('markov');
        accepted.add('regime');
        accepted.add('markovstate');
    }
    if (indicator.id === 'lag_one_return_autocorrelation') {
        accepted.add('autocorrelation');
        accepted.add('lagoneautocorrelation');
    }
    return [component?.indicator, component?.indicator_id, component?.id, component?.key]
        .some(identity => accepted.has(normalizedIndicatorIdentity(identity)));
}

export function signalIndicatorScores(signal, model) {
    if (!hasSixIndicatorScores(signal, model)) return [];
    const components = Array.isArray(signal?.score_basis?.components) ? signal.score_basis.components : [];
    return SIGNAL_SCORE_INDICATORS.map(({ id, indicator, label }) => {
        const component = components.find(item => componentMatchesIndicator(item, { id, indicator }));
        const value = component?.points;
        const points = typeof value === 'number' && Number.isFinite(value) && value >= -10 && value <= 10 ? value : null;
        const weightPoints = typeof component?.weight_points === 'number' && Number.isFinite(component.weight_points)
            ? component.weight_points : 10;
        const weightedPoints = points === null ? null
            : typeof component?.weighted_points === 'number' && Number.isFinite(component.weighted_points)
                ? component.weighted_points : points * weightPoints / 10;
        return { id, indicator, label, points, weightedPoints, scale: 10, weight: weightPoints / 60,
            profileVersion: component?.scoring_profile_version ?? null,
            state: String(component?.state || 'UNAVAILABLE'), value: component?.value };
    });
}

export function signalIndicatorScoreForFilter(signal, model, indicatorId) {
    const indicator = SIGNAL_SCORE_INDICATORS.find(item => item.id === indicatorId);
    if (!indicator) return null;
    const score = signalIndicatorScores(signal, model).find(item => item.id === indicator.id);
    return score && score.points !== null && score.state.toUpperCase() !== 'UNAVAILABLE'
        ? Math.abs(score.points) : null;
}

// A presentation threshold only; registered model eligibility owns its own floor.
const LOW_TOTAL_INDICATOR_POINTS = 10;

export function signalTotalStrength(signal, model) {
    if (!hasSixIndicatorScores(signal, model)) return null;
    const indicators = signalIndicatorScores(signal, model);
    const known = indicators.every(item => item.points !== null);
    const total = known ? indicators.reduce((sum, item) => sum + (item.weightedPoints ?? item.points), 0) : null;
    const low = known && Math.abs(total) < LOW_TOTAL_INDICATOR_POINTS;
    const label = !known ? 'Setup strength unavailable' : low ? 'Low setup strength' : `Setup strength ${LOW_TOTAL_INDICATOR_POINTS}+ points`;
    const retained = signalScoreState(signal)?.tone === 'stale';
    return {
        tone: !known ? 'unavailable' : low ? 'low' : 'standard',
        label: retained ? `Last recorded: ${label.toLowerCase()}` : label,
        description: !known ? 'All six numeric indicator contributions are needed to classify setup strength.'
            : `The six signed readings range from −10 to +10. Their weighted sum determines direction; its magnitude is ${low ? 'less than' : 'at least'} ${LOW_TOTAL_INDICATOR_POINTS} weighted points.`,
    };
}

export function signalIndicatorSummary(signal, model) {
    return signalIndicatorScores(signal, model).map(item =>
        `${item.label} ${item.points === null ? '—' : signedDirectionScore(item.points)}/10 (${scoreWeightLabel(item.weight)}, contributes ${item.weightedPoints === null ? '—' : signedDirectionScore(item.weightedPoints)}${item.state.toUpperCase() === 'UNAVAILABLE' ? ', unavailable' : ''})`
    ).join(' · ');
}

export function signalScoreWeightSummary(signal, model) {
    if (hasSixIndicatorScores(signal, model)) return signalIndicatorScores(signal, model).map(item => `${item.label} ${Math.round(item.weight * 100)}%`).join(', ');
    const weights = signalScoreComponentWeights(signal, model);
    const strategyLabel = String(model?.strategy_score_label || 'Strategy');
    const confluenceLabel = String(model?.confluence_score_label || 'Confluence');
    const share = weight => Number.isFinite(weight) ? `${Math.round(weight * 100)}%` : 'unavailable';
    return `${strategyLabel} ${share(weights.strategy)}, ${confluenceLabel} ${share(weights.confluence)}`;
}

export function isPersistentNoEntryScore(signal) {
    return signal?.persistent_score === true && signal?.score_status === 'not_qualifying';
}

export function signalMeetsRenderMinimum(signal, model) {
    if (isPersistentNoEntryScore(signal)) return true;
    const { minimumScore } = signalScorePresentation(signal, model);
    const score = signalDisplayScore(signal);
    return !Number.isFinite(minimumScore)
        || (Number.isFinite(score) && score >= minimumScore);
}

export function signalDisplayScore(signal) {
    if (signal?.persistent_score === true && signal.score_status === 'not_qualifying') return 0;
    // `score` is a signed vote on the current API. The positive weighted
    // metric is setup strength. Fall back to `score` only for older payloads.
    const hasSetupStrength = Object.prototype.hasOwnProperty.call(signal || {}, 'setup_strength');
    const contract = String(signal?.score_contract || '').trim()
        || String(signal?.score_basis?.score_contract || '').trim();
    const value = hasSetupStrength
        ? signal.setup_strength
        : contract ? null : signal?.score;
    const isNumericInput = typeof value === 'number'
        || (typeof value === 'string' && value.trim() !== '');
    if (!isNumericInput) return null;
    const score = Number(value);
    return Number.isFinite(score) ? score : null;
}

export function signalDirectionalScore(signal) {
    if (isPersistentNoEntryScore(signal)) return null;
    const raw = signal?.score;
    const score = typeof raw === 'number' && Number.isFinite(raw) ? raw : null;
    const contract = signal?.score_contract || signal?.score_basis?.score_contract;
    if (contract !== SIGNED_DIRECTION_SCORE_CONTRACT || score === null) return null;
    const direction = String(signal?.direction || '').toUpperCase();
    if ((direction !== 'LONG' && direction !== 'SHORT')
        || (direction === 'LONG' && score <= 0)
        || (direction === 'SHORT' && score >= 0)) return null;
    const state = signalScoreState(signal);
    const lastKnown = !state.current || signal.market_entry_status === 'NO_MARKET_ENTRY';
    const net = signal?.direction_score;
    const netScore = typeof net?.net_score === 'number' && Number.isFinite(net.net_score) ? net.net_score : null;
    return {
        score,
        netScore: lastKnown ? null : netScore,
        longCount: lastKnown ? null : net?.long_count ?? null,
        shortCount: lastKnown ? null : net?.short_count ?? null,
        ...(lastKnown ? { lastKnown: true } : netScore === null ? { netUnavailable: true } : {}),
    };
}

export function signedDirectionScore(value) {
    if (!Number.isFinite(value)) return '—';
    const magnitude = Math.abs(value).toFixed(1);
    return value > 0 ? `+${magnitude}` : value < 0 ? `−${magnitude}` : '0.0';
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
    const directionScore = signalDirectionalScore(signal);
    const directionSummaryBase = directionScore
        ? directionScore.lastKnown
            ? `Last direction ${signedDirectionScore(directionScore.score)} · current net unavailable`
            : directionScore.netUnavailable
                ? `Direction score ${signedDirectionScore(directionScore.score)} · current net unavailable`
                : `Direction score ${signedDirectionScore(directionScore.score)} · net ${signedDirectionScore(directionScore.netScore)}`
        : 'Direction score unavailable';
    const balance = signalScoreBalance(signal, model);
    const directionSummary = directionSummaryBase + (balance
        ? ` · bullish +${balance.bullish.toFixed(1)} / bearish −${balance.bearish.toFixed(1)} · ${balance.cancellationPct.toFixed(0)}% cancellation` : '');
    if (hasSixIndicatorScores(signal, model)) {
        const label = scoreState?.tone === 'stale' ? 'Last recorded indicator scores' : 'Indicator scores';
        const strength = signalTotalStrength(signal, model);
        const total = signalDisplayScore(signal);
        const totalLabel = Number.isFinite(total) ? `Setup strength ${total.toFixed(1)}/60 · ` : '';
        const indicators = `${totalLabel}${label}: ${signalIndicatorSummary(signal, model)}${state}`;
        const summary = strength.tone === 'low' ? `${strength.label} · ${indicators}` : `${indicators} · ${strength.label}`;
        return `${directionSummary} · ${summary} · ${strength.description} · setup strength is separate from the direction score, win probability, and execution availability`;
    }
    const displayScore = signalDisplayScore(signal);
    const scoreLabel = scoreState?.tone === 'stale' ? 'Last recorded setup strength' : 'Setup strength';
    const scoreValue = Number.isFinite(displayScore)
        ? `${displayScore.toFixed(1)}/${scoreScaleMax}`
        : 'unavailable';
    return `${directionSummary} · ${scoreLabel} ${scoreValue}${state} · ${signalScoreWeightSummary(signal, model)} · setup strength is separate from the direction score, win probability, and execution availability`;
}

export function averageNormalizedScore(signals) {
    const scores = signals.map(signal => {
        const model = signalScorePresentation(signal, signal.scoreModel);
        const scaleMax = model.scoreScaleMax;
        const score = signalDisplayScore(signal);
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


/** Describe cancellation without changing entry eligibility or implying accuracy. */
export function signalScoreBalance(signal, model) {
    const factors = signalIndicatorScores(signal, model);
    if (factors.length !== 6 || factors.some(f => !Number.isFinite(f.weightedPoints))) return null;
    const bullish = factors.reduce((sum, f) => sum + Math.max(0, f.weightedPoints), 0);
    const bearish = factors.reduce((sum, f) => sum + Math.max(0, -f.weightedPoints), 0);
    const gross = bullish + bearish;
    return { bullish, bearish, net: bullish - bearish,
        cancellationPct: gross > 0 ? (1 - Math.abs(bullish - bearish) / gross) * 100 : 0 };
}

export function retainStockbitScoreReference(signal) {
    return signal?.persistent_score === true && signal?.exchange_id === 'stockbit'
        && signal?.score_basis?.score_model_status !== 'legacy';
}
