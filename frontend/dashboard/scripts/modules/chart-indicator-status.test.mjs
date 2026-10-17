import test from 'node:test';
import assert from 'node:assert/strict';
import { describeMarkovRegimeStatus, describeNweEnvelopeStatus } from './chart-indicator-status.js';

test('NWE status keeps repainting warmup and stale provenance visible', () => {
    const status = describeNweEnvelopeStatus({
        candles: [{ close: 10 }],
        envelope: null,
        timeframe: '15m',
        repaintingMode: true,
        stale: true,
    });
    assert.equal(status.text, 'Latest NWE 15M · REPAINT: warming up · requires 2 history bars · stale bars');
    assert.match(status.ariaLabel, /using stale chart data/);
    assert.match(status.ariaLabel, /can repaint/);
});

test('NWE status distinguishes upper and lower crossing events', () => {
    const upper = describeNweEnvelopeStatus({
        candles: [{ close: 12 }],
        envelope: { upper: 11, lower: 9, crossed_above_upper: true },
        timeframe: '1h',
    });
    const lower = describeNweEnvelopeStatus({
        candles: [{ close: 8 }],
        envelope: { upper: 11, lower: 9, crossed_below_lower: true },
        timeframe: '1h',
    });
    assert.match(upper.text, /crossed above upper · SHORT/);
    assert.match(lower.text, /crossed below lower · LONG/);
    assert.match(upper.ariaLabel, /Crossing markers are study events, not trade instructions/);
});

test('NWE status handles missing bars and endpoint warmup', () => {
    const empty = describeNweEnvelopeStatus({ candles: [] });
    const warming = describeNweEnvelopeStatus({
        candles: [{ close: 10 }],
        endpointWarmupBars: 42,
    });
    assert.match(empty.text, /no bars/);
    assert.match(warming.text, /warming up · requires 42 history bars/);
});

test('Markov status covers disabled, waiting, and stale causal states', () => {
    const disabled = describeMarkovRegimeStatus({ enabled: false, timeframe: '1h' });
    const waiting = describeMarkovRegimeStatus({ enabled: true, signal: {}, timeframe: '1h' });
    const stale = describeMarkovRegimeStatus({
        enabled: true,
        timeframe: '1h',
        signal: {
            markov_regimes_by_tf: { '1h': [{ state: 'mean_reverting' }] },
            chart_freshness: { '1h': { is_stale: true } },
        },
    });
    assert.equal(disabled.text, 'Markov ribbon off');
    assert.match(waiting.text, /waiting for chart data/);
    assert.equal(stale.text, 'Markov 1H: mean reverting · stale bars');
    assert.match(stale.ariaLabel, /Historical state context, not a forecast/);
});

