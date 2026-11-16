import test from 'node:test';
import assert from 'node:assert/strict';
import {signalScoreBalance, SIGNAL_SCORE_INDICATORS} from './signal-screener-model.js';
const signal = points => ({score_basis: {score_scale_max: 60, components: points.map((p,i)=>({
    indicator: SIGNAL_SCORE_INDICATORS[i].indicator, points:p, weight_points:10, weighted_points:p
}))}});
test('opposing readings disclose cancellation rather than suggesting consensus', () => {
    const result = signalScoreBalance(signal([8,-6,3,-7,-10,2]), {});
    assert.equal(result.bullish,13);
    assert.equal(result.bearish,23);
    assert.equal(result.net,-10);
    assert.ok(Math.abs(result.cancellationPct - 72.222222) < 0.001);
});
test('unanimous and neutral readings remain distinct; missing factors stay unknown', () => {
    assert.equal(signalScoreBalance(signal([-8,-6,-3,-7,-10,-2]), {}).cancellationPct,0);
    assert.equal(signalScoreBalance(signal([0,0,0,0,0,0]), {}).net,0);
    assert.equal(signalScoreBalance(signal([8]), {}),null);
});
