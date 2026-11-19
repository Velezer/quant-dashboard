import test from 'node:test';
import assert from 'node:assert/strict';
import { signalRiskRewardBracket, visibleOverlayPriceRange, withChartPricePaneClip } from './chart-renderer.js';

test('chart accepts fee-aware 1:4 bracket levels only on the signal timeframe', () => {
    const signal = {
        timeframe: '4h',
        direction: 'LONG',
        risk_reward_plan: {
            status: 'PLANNED',
            timeframe: '4h',
            fees_applied: true,
            direction: 'LONG',
            entry_price: 100,
            stop_price: 98,
            target_price: 108.1,
            minimum_target_after_fee_r_multiple: 4,
            planned_after_fee_r_multiple: 4,
        },
    };

    assert.deepEqual(signalRiskRewardBracket(signal, '4h'), {
        entry: 100,
        stop: 98,
        target: 108.1,
        rMultiple: 4,
        direction: 'LONG',
        referenceOnly: false,
    });
    assert.equal(signalRiskRewardBracket(signal, '1h'), null);
    assert.equal(signalRiskRewardBracket({ ...signal, risk_reward_plan: { ...signal.risk_reward_plan, fees_applied: false } }, '4h'), null);
});

test('chart accepts short reference brackets and rejects inverted levels', () => {
    const signal = {
        timeframe: '4h', direction: 'SHORT',
        risk_reward_plan: {
            status: 'REFERENCE_ONLY', timeframe: '4h', fees_applied: true,
            direction: 'SHORT', entry_price: 100, stop_price: 102, target_price: 91.8,
            minimum_target_after_fee_r_multiple: 4,
            planned_after_fee_r_multiple: 4, entry_price_is_reference_only: true,
        },
    };
    assert.equal(signalRiskRewardBracket(signal, '4h')?.referenceOnly, true);
    assert.equal(signalRiskRewardBracket({ ...signal, risk_reward_plan: { ...signal.risk_reward_plan, target_price: 101 } }, '4h'), null);
    assert.equal(signalRiskRewardBracket({ ...signal, risk_reward_plan: { ...signal.risk_reward_plan, planned_after_fee_r_multiple: 3.99 } }, '4h'), null);
});

test('overlay autoscale ignores off-screen Pine values', () => {
    const range = visibleOverlayPriceRange([
        { values: [120, 1.02, 1.03, 1.01, -50] },
    ], 1, 4);

    assert.deepEqual(range, { min: 1.01, max: 1.03 });
});

test('overlay autoscale uses the visible window and skips invalid values', () => {
    const range = visibleOverlayPriceRange([
        { values: [-900, null, 0.91, Number.NaN, 1.08, 1400] },
        { values: [null, undefined, 0.95, 1.01, Number.NaN] },
    ], 2, 5);

    assert.deepEqual(range, { min: 0.91, max: 1.08 });
    assert.deepEqual(visibleOverlayPriceRange([], 0, 5), { min: Infinity, max: -Infinity });
});

test('price pane overlay drawing is clipped to the plot bounds', () => {
    const operations = [];
    const context = {
        save() { operations.push('save'); },
        beginPath() { operations.push('beginPath'); },
        rect(...bounds) { operations.push(['rect', ...bounds]); },
        clip() { operations.push('clip'); },
        restore() { operations.push('restore'); },
    };

    withChartPricePaneClip(
        context,
        { left: 15, top: 20, width: 889, height: 240 },
        () => operations.push('draw overlay'),
    );

    assert.deepEqual(operations, [
        'save',
        'beginPath',
        ['rect', 15, 20, 889, 240],
        'clip',
        'draw overlay',
        'restore',
    ]);
});

test('price pane clipping restores the canvas if overlay drawing throws', () => {
    const operations = [];
    const context = {
        save() { operations.push('save'); },
        beginPath() { operations.push('beginPath'); },
        rect() { operations.push('rect'); },
        clip() { operations.push('clip'); },
        restore() { operations.push('restore'); },
    };

    assert.throws(
        () => withChartPricePaneClip(context, { left: 0, top: 0, width: 10, height: 10 }, () => {
            throw new Error('overlay failed');
        }),
        /overlay failed/,
    );
    assert.equal(operations.at(-1), 'restore');
});
