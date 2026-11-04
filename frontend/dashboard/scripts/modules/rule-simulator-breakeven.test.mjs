import test from 'node:test';
import assert from 'node:assert/strict';
import { feeAdjustedBreakeven } from './rule-simulator-breakeven.js';

test('long break-even exit covers buy fee at entry and sell fee at exit', () => {
    const result = feeAdjustedBreakeven(100, 0.1, 0.2, 'LONG');

    assert.ok(result);
    assert.ok(Math.abs(result.exitPrice - 100 * 100.1 / 99.8) < 1e-10);
    assert.ok(Math.abs(result.requiredMovePct - 0.3006012024) < 1e-8);
});

test('short break-even exit applies sell fee at entry and buy fee at exit', () => {
    const result = feeAdjustedBreakeven(100, 0.1, 0.2, 'SHORT');

    assert.ok(result);
    assert.ok(Math.abs(result.exitPrice - 100 * 99.8 / 100.1) < 1e-10);
    assert.ok(Math.abs(result.requiredMovePct - 0.2997002997) < 1e-8);
});

test('zero fees leave the break-even exit at the entry price', () => {
    assert.deepEqual(feeAdjustedBreakeven(42.5, 0, 0, 'LONG'), {
        exitPrice: 42.5,
        requiredMovePct: 0,
    });
});

test('break-even calculator rejects missing, invalid, and unsupported inputs', () => {
    assert.equal(feeAdjustedBreakeven('', 0.1, 0.2, 'LONG'), null);
    assert.equal(feeAdjustedBreakeven(0, 0.1, 0.2, 'LONG'), null);
    assert.equal(feeAdjustedBreakeven(100, null, 0.2, 'LONG'), null);
    assert.equal(feeAdjustedBreakeven(100, '', 0.2, 'LONG'), null);
    assert.equal(feeAdjustedBreakeven(100, -0.1, 0.2, 'LONG'), null);
    assert.equal(feeAdjustedBreakeven(100, 0.1, 100, 'LONG'), null);
    assert.equal(feeAdjustedBreakeven(100, 0.1, 0.2, 'FLAT'), null);
});
