import test from 'node:test';
import assert from 'node:assert/strict';
import { parseGrowthProjectionInputs, parseRuleInputs } from './rule-simulator-inputs.js';

function input(value, valid = true) {
    const result = {
        value: String(value),
        valid,
        attributes: {},
        classes: {},
        checkValidity() { return this.valid; },
        setAttribute(name, nextValue) { this.attributes[name] = nextValue; },
    };
    result.classList = { toggle(name, enabled) { result.classes[name] = enabled; } };
    return result;
}

test('R:R simulator parses valid rule inputs and preserves field validity feedback', () => {
    const fields = {
        risk_pct: input(1.25),
        reward_pct: input(2.5),
        win_rate_pct: input(55),
        max_trades_per_day: input(4),
    };

    assert.deepEqual(parseRuleInputs(fields), {
        risk_pct: 1.25,
        reward_pct: 2.5,
        win_rate_pct: 55,
        max_trades_per_day: 4,
    });
    assert.ok(Object.values(fields).every(field => field.attributes['aria-invalid'] === 'false'));
    assert.ok(Object.values(fields).every(field => field.classes['border-rose-600'] === false));
});

test('R:R simulator rejects invalid rule inputs and marks the invalid control', () => {
    const fields = {
        risk_pct: input(1),
        reward_pct: input(2, false),
        win_rate_pct: input(50),
        max_trades_per_day: input(1),
    };

    assert.equal(parseRuleInputs(fields), null);
    assert.equal(fields.reward_pct.attributes['aria-invalid'], 'true');
    assert.equal(fields.reward_pct.classes['border-rose-600'], true);
});

test('R:R simulator growth projection requires finite values and whole trading days', () => {
    const fields = {
        starting_balance_units: input(1000),
        trade_size_pct_of_equity: input(2.5),
        trading_days: input(252),
    };

    assert.deepEqual(parseGrowthProjectionInputs(fields), {
        starting_balance_units: 1000,
        trade_size_pct_of_equity: 2.5,
        trading_days: 252,
    });
    fields.trading_days.value = '252.5';
    assert.equal(parseGrowthProjectionInputs(fields), null);
    fields.trading_days.valid = false;
    assert.equal(parseGrowthProjectionInputs(fields), null);
    assert.equal(fields.trading_days.attributes['aria-invalid'], 'true');
});
