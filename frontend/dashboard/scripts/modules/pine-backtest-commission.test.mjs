import test from 'node:test';
import assert from 'node:assert/strict';
import { pineBacktestCommissionLabel } from './pine-backtest-commission.js';

test('backtest UI commission label shows configured rate and return basis', () => {
    assert.equal(pineBacktestCommissionLabel({
        model: 'Pine strategy commission: 2 chart-currency units per contract traded',
        return_basis: 'after Pine strategy.commission.cash_per_contract in chart currency',
    }), 'Commission: Pine strategy commission: 2 chart-currency units per contract traded · after Pine strategy.commission.cash_per_contract in chart currency');
});

test('backtest UI commission label distinguishes fixed per-order costs', () => {
    assert.equal(pineBacktestCommissionLabel({
        model: 'Pine strategy commission: 2 chart-currency units per filled order',
        return_basis: 'after Pine strategy.commission.cash_per_order in chart currency',
    }), 'Commission: Pine strategy commission: 2 chart-currency units per filled order · after Pine strategy.commission.cash_per_order in chart currency');
});

test('backtest UI commission label makes missing commission metadata explicit', () => {
    assert.equal(pineBacktestCommissionLabel(null), 'Commission: Commission model unavailable · return basis unavailable');
});
