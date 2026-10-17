import assert from 'node:assert/strict';
import test from 'node:test';
import { MAX_ACTIVE_PINE_INDICATORS, pineChartPlots } from './pine-chart-output.js';
import { NWE_MAX_OUTSIDE_DISTANCE_ID } from './pine-sources.js';

test('two registered Pine outputs retain overlay and pane plots together', () => {
    const candles = [1, 2, 3];
    const outputs = [
        {
            name: 'Nadaraya-Watson Envelope [LuxAlgo]',
            timeframe: '15m',
            plots: [{ title: 'Upper', overlay: true, values: candles }],
        },
        {
            name: 'DSS Bressert (Double Smoothed Stochastic)',
            author: 'HPotter',
            timeframe: '15m',
            requested_timeframes: ['1h'],
            requested_data_contexts: ['PINTU · ZRX · 1h · binance spot proxy'],
            plots: [
                { title: 'DSS', overlay: false, values: candles },
                { title: 'Trigger', overlay: false, values: candles },
                { title: 'Overbought', kind: 'hline', color: '#089981', overlay: false, values: [80, 80, 80] },
                { title: 'Oversold', kind: 'hline', color: '#f23645', overlay: false, values: [20, 20, 20] },
            ],
        },
    ];

    const result = pineChartPlots(outputs, '15m', candles.length);

    assert.equal(MAX_ACTIVE_PINE_INDICATORS, 2);
    assert.deepEqual(result.overlayPlots.map(plot => plot.title), ['Upper']);
    assert.deepEqual(result.panePlots.map(plot => plot.title), ['DSS', 'Trigger', 'Overbought', 'Oversold']);
    assert.equal(result.hasDssBressert, true);
    assert.equal(result.dssOutput.author, 'HPotter');
    assert.equal(result.paneOutput.name, 'DSS Bressert (Double Smoothed Stochastic)');
    assert.deepEqual(result.dssOutput.requested_timeframes, ['1h']);
    assert.deepEqual(result.dssOutput.plots.map(plot => plot.kind), [undefined, undefined, 'hline', 'hline']);
    assert.deepEqual(result.dssOutput.plots.map(plot => plot.values[0]), [1, 1, 80, 20]);
});

test('Pine chart plots ignore other timeframes and misaligned series', () => {
    const outputs = [
        {
            name: 'DSS Bressert (Double Smoothed Stochastic)',
            timeframe: '1h',
            plots: [{ title: 'DSS', overlay: false, values: [1, 2] }],
        },
        {
            name: 'NWE',
            timeframe: '15m',
            plots: [{ title: 'Upper', overlay: true, values: [1] }],
        },
    ];

    assert.deepEqual(pineChartPlots(outputs, '15m', 2), {
        overlayPlots: [],
        panePlots: [],
        hasDssBressert: false,
        dssOutput: null,
        nweOutsideDistanceOutput: null,
        paneOutput: null,
    });
});

test('NWE maximum outside distance is selected for the indicator pane', () => {
    const output = {
        id: NWE_MAX_OUTSIDE_DISTANCE_ID,
        name: 'NWE Max Outside Distance (%)',
        author: 'Quant Research',
        timeframe: '1d',
        plots: [
            { title: 'Max above upper band (%)', color: '#089981', overlay: false, values: [null, 15] },
            { title: 'Max below lower band (%)', color: '#f23645', overlay: false, values: [null, 4] },
        ],
    };
    const result = pineChartPlots([output], '1d', 2);

    assert.equal(result.nweOutsideDistanceOutput.name, output.name);
    assert.equal(result.paneOutput.name, output.name);
    assert.deepEqual(result.paneOutput.plots.map(plot => plot.title), output.plots.map(plot => plot.title));
});
