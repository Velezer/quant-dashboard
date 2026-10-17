import test from 'node:test';
import assert from 'node:assert/strict';
import { pineBacktestDiagnostics, renderPineBacktestDiagnostics } from './pine-backtest-diagnostics.js';

test('Pine backtest diagnostics preserve warning severities and paragraph boundaries', () => {
    const result = pineBacktestDiagnostics([
        { severity: 'approximation', message: 'Chart OHLC fills are approximate.' },
        { severity: 'warning', message: 'Margin is unsupported.' },
    ]);
    assert.equal(result.state, 'ready');
    assert.equal(result.text, 'approximation: Chart OHLC fills are approximate.\n\nwarning: Margin is unsupported.');
});

test('Pine backtest diagnostics distinguish empty and unavailable responses', () => {
    assert.match(pineBacktestDiagnostics([]).text, /No additional runtime diagnostics/);
    assert.equal(pineBacktestDiagnostics(null).state, 'unavailable');
    assert.match(pineBacktestDiagnostics(null).text, /unavailable/);
});

test('Pine backtest diagnostics ignore malformed diagnostic rows', () => {
    assert.match(pineBacktestDiagnostics([null, {}, { message: ' ' }]).text, /No additional runtime diagnostics/);
});

test('Pine backtest diagnostics render in the provided UI target without HTML interpretation', () => {
    const target = { dataset: {}, textContent: '' };
    renderPineBacktestDiagnostics(target, [{ severity: 'warning', message: '<margin unsupported>' }]);
    assert.equal(target.dataset.state, 'ready');
    assert.equal(target.textContent, 'warning: <margin unsupported>');
    renderPineBacktestDiagnostics(target, null);
    assert.equal(target.dataset.state, 'empty');
});
