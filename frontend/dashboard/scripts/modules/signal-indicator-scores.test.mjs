import test from 'node:test';
import assert from 'node:assert/strict';
import {
    hasSixIndicatorScores,
    SIGNAL_SCORE_INDICATORS,
    signalIndicatorScores,
    signalScoreSummary,
    signalScoreState,
} from './signal-screener-model.js';

const model = { score_scale_max: 60 };
const components = SIGNAL_SCORE_INDICATORS.map(({ indicator }, index) => ({
    indicator,
    state: index === 2 ? 'UNAVAILABLE' : 'AVAILABLE',
    points: index === 2 ? 0 : index + 1,
    weight_points: 10,
    weighted_points: index === 2 ? 0 : index + 1,
}));

test('current six-factor models expose six signed /10 values at the configured weights', () => {
    const signal = { score: 19, score_basis: { score_scale_max: 60, components } };
    assert.equal(hasSixIndicatorScores(signal, model), true);
    const scores = signalIndicatorScores(signal, model);
    assert.deepEqual(scores.map(item => item.points), [1, 2, 0, 4, 5, 6]);
    assert(scores.every(item => item.scale === 10 && Math.abs(item.weight - 1 / 6) < 1e-9));
    const summary = signalScoreSummary(signal, model);
    assert.equal((summary.match(/\/10/g) || []).length, 6);
    assert.equal((summary.match(/17% weight/g) || []).length, 6);
    assert.match(summary, /Relative volume 0\.0\/10 \(17% weight, contributes 0\.0, unavailable\)/);
    assert.doesNotMatch(summary, /\/40/);
});

test('stale factors retain their values and missing or invalid factors remain unavailable', () => {
    const stale = { score: 19, persistent_score: true, score_status: 'stale',
        score_basis: { score_scale_max: 60, components } };
    const summary = signalScoreSummary(stale, model, signalScoreState(stale));
    assert.match(summary, /^Direction score unavailable.*Last recorded indicator scores: NWE \+1\.0\/10/);
    assert.match(summary, /stale · no market entry · held/);
    for (const points of [null, undefined, '', '6', -11, 11, NaN, true]) {
        const malformed = [{ ...components[0], points, weighted_points: 9 }, ...components.slice(1)];
        const result = signalIndicatorScores({ score_basis: { score_scale_max: 60, components: malformed } }, model);
        assert.equal(result[0].points, null);
        assert.equal(result[0].weightedPoints, null);
    }
});

test('legacy models keep their recorded scale instead of inventing six factors', () => {
    const legacy = { score: 7.4, score_basis: { score_scale_max: 10, score_model_status: 'legacy', components } };
    assert.equal(hasSixIndicatorScores(legacy, model), false);
    assert.deepEqual(signalIndicatorScores(legacy, model), []);
    assert.match(signalScoreSummary(legacy, model), /7\.4\/10/);
    assert.equal(hasSixIndicatorScores({}, { score_scale_max: 20 }), false);
});
