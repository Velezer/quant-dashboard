import test from 'node:test';
import assert from 'node:assert/strict';
import { signalDirectionalScore, signalIndicatorScores, signalIndicatorScoreForFilter, signalDisplayScore } from './signal-screener-model.js';
import { exampleTotals } from './scoring-guide.js';

test('opposing signed factor readings cancel in the guide', () => {
    assert.equal(exampleTotals([8, -6, 3, -7, -10, 2]).total, -10);
    assert.equal(exampleTotals([8, -8, 0, 0, 0, 0]).total, 0);
});

test('negative factor and total remain visible and strength filters use magnitude', () => {
    const signal = { score: -12.5, setup_strength: 12.5, score_contract: 'signed_indicator_sum_v1',
        direction: 'SHORT', price_is_fresh: true, score_basis: { score_scale_max: 60,
            components: [{indicator: 'DSS Bressert', points: -8, weighted_points: -8, weight_points: 10, state: 'SIGNED_DSS_SPREAD'}] } };
    assert.equal(signalDirectionalScore(signal).score, -12.5);
    assert.equal(signalDisplayScore(signal), 12.5);
    assert.equal(signalIndicatorScores(signal, {}).find(x => x.id === 'dss_bressert').points, -8);
    assert.equal(signalIndicatorScoreForFilter(signal, {}, 'dss_bressert'), 8);
});
