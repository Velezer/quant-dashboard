import test from 'node:test';
import assert from 'node:assert/strict';
import { hasFourIndicatorScores, signalIndicatorScores, signalScoreSummary, signalScoreState } from './signal-screener-model.js';

const model = { score_scale_max: 40 };
const components = [
    { indicator: 'Nadaraya-Watson Envelope', state: 'NEAR_LOWER', points: 8 },
    { indicator: 'DSS Bressert', state: 'BULLISH_CROSS', points: 6 },
    { indicator: 'Relative Volume', state: 'UNAVAILABLE', points: 0 },
    { indicator: 'Estimated Volume Delta (OHLCV proxy)', state: 'ALIGNED', points: 5 },
];

test('current four-factor models expose four separate /10 values, each with 25% weight', () => {
    const signal = { score: 19, score_basis: { components } };
    assert.equal(hasFourIndicatorScores(signal, model), true);
    const scores = signalIndicatorScores(signal, model);
    assert.deepEqual(scores.map(item => item.points), [8, 6, 0, 5]);
    assert(scores.every(item => item.scale === 10 && item.weight === 0.25));
    const summary = signalScoreSummary(signal, model);
    assert.equal((summary.match(/\/10/g) || []).length, 4);
    assert.equal((summary.match(/25% weight/g) || []).length, 4);
    assert.match(summary, /Relative volume 0\.0\/10 \(25% weight, unavailable\)/);
    assert.doesNotMatch(summary, /19\.0|\/40/);
});

test('stale factors retain their values and missing or invalid factors remain unavailable', () => {
    const stale = { score: 19, persistent_score: true, score_status: 'stale', score_basis: { components } };
    const summary = signalScoreSummary(stale, model, signalScoreState(stale));
    assert.match(summary, /^Last recorded indicator scores: NWE 8\.0\/10/);
    assert.match(summary, /stale · no market entry/);
    for (const points of [null, undefined, '', '6', -1, 11, NaN, true]) {
        const result = signalIndicatorScores({ score_basis: { components: [{ ...components[0], points }] } }, model);
        assert.equal(result[0].points, null);
        assert.equal(result[1].points, null);
    }
    assert.equal(signalIndicatorScores({ score_basis: { components: [null, false, {}] } }, model)[0].points, null);
    assert.match(signalScoreSummary({}, model), /NWE —\/10 \(25% weight, unavailable\)/);
});

test('legacy models retain their recorded scales rather than inventing four component values', () => {
    const legacy = { score: 7.4, score_basis: { score_model_status: 'legacy', score_scale_max: 10 } };
    assert.equal(hasFourIndicatorScores(legacy, model), false);
    assert.deepEqual(signalIndicatorScores(legacy, model), []);
    assert.match(signalScoreSummary(legacy, model), /7\.4\/10/);
    assert.equal(hasFourIndicatorScores({}, { score_scale_max: 20 }), false);
});
