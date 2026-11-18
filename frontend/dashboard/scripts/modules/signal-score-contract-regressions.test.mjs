import test from 'node:test';
import assert from 'node:assert/strict';
import {
    SIGNAL_SCORE_INDICATORS,
    signalDisplayScore,
    signalIndicatorScoreForFilter,
    signalIndicatorScores,
    signalScoreBalance,
    signalDirectionalScore,
    signalScoreState,
} from './signal-screener-model.js';

const freshRow = overrides => ({
    persistent_score: true,
    score_status: 'qualifying',
    timeframe: '1h',
    price_is_fresh: true,
    price_fresh_until_ms: 2_000,
    ...overrides,
});

test('unknown persistent score and market-entry statuses fail closed', () => {
    assert.deepEqual(signalScoreState(freshRow({ score_status: 'future_status' }), 1_000), {
        label: 'SCORE STATUS UNKNOWN · NO MARKET ENTRY', tone: 'unknown', current: false,
    });
    assert.deepEqual(signalScoreState(freshRow({ market_entry_status: 'WAITING' }), 1_000), {
        label: 'ENTRY STATUS UNKNOWN · NO MARKET ENTRY', tone: 'unknown', current: false,
    });
    assert.deepEqual(signalScoreState({
        timeframe: '1h',
        price_is_fresh: true,
        price_fresh_until_ms: 2_000,
        market_entry_status: 'WAITING',
    }, 1_000), {
        label: 'ENTRY STATUS UNKNOWN · NO MARKET ENTRY', tone: 'unknown', current: false,
    });
});

test('only legacy rows without a score contract may use score as setup strength', () => {
    assert.equal(signalDisplayScore({ score: 22.5 }), 22.5);
    assert.equal(signalDisplayScore({ score: -50, score_contract: 'signed_direction_vote_v1' }), null);
    assert.equal(signalDisplayScore({
        score: -50,
        score_basis: { score_contract: 'signed_direction_vote_v1' },
    }), null);
    assert.equal(signalDisplayScore({
        score: -50,
        score_contract: '',
        score_basis: { score_contract: 'signed_direction_vote_v1' },
    }), null);
    assert.equal(signalDisplayScore({
        score: -50,
        score_contract: 'signed_indicator_sum_v1',
        setup_strength: 22.5,
    }), 22.5);
});

test('a signed score cannot be presented under a contradictory or missing direction', () => {
    const valid = { score: -8, score_contract: 'signed_indicator_sum_v1', direction: 'SHORT' };
    assert.equal(signalDirectionalScore(valid)?.score, -8);
    assert.equal(signalDirectionalScore({ ...valid, direction: 'LONG' }), null);
    assert.equal(signalDirectionalScore({ ...valid, direction: undefined }), null);
});

test('an unavailable numeric component cannot retain an orphaned weighted contribution or pass its filter', () => {
    const components = SIGNAL_SCORE_INDICATORS.map(({ indicator }) => ({
        indicator,
        state: 'AVAILABLE',
        points: 4,
        weighted_points: 4,
        weight_points: 10,
    }));
    components[0] = { ...components[0], points: null, weighted_points: 7 };
    const signal = { score_basis: { score_scale_max: 60, components } };
    const scores = signalIndicatorScores(signal, { score_scale_max: 60 });

    assert.equal(scores[0].points, null);
    assert.equal(scores[0].weightedPoints, null);
    assert.equal(signalIndicatorScoreForFilter(signal, { score_scale_max: 60 }, 'nwe'), null);
    assert.equal(signalScoreBalance(signal, { score_scale_max: 60 }), null);
});
