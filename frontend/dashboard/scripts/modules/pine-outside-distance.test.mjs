import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { NWE_MAX_OUTSIDE_DISTANCE_SOURCE } from './pine-sources.js';

const wasmUrl = new URL('../../../../target/wasm32-unknown-unknown/release/quant_pine_runtime.wasm', import.meta.url);

async function loadWasm() {
    const bytes = await readFile(wasmUrl);
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return instance.exports;
}

function runIndicator(exports, candles, upper, lower) {
    const request = new TextEncoder().encode(JSON.stringify({
        source: NWE_MAX_OUTSIDE_DISTANCE_SOURCE,
        candles,
        timeframe: '1d',
        timeframes: {},
        input_sources: {
            'NWE Upper': upper,
            'NWE Lower': lower,
        },
    }));
    const requestPointer = exports.alloc(request.length);
    new Uint8Array(exports.memory.buffer, requestPointer, request.length).set(request);
    const packed = exports.run(requestPointer, request.length);
    exports.dealloc(requestPointer, request.length);
    const resultPointer = Number(packed & 0xffffffffn);
    const resultLength = Number(packed >> 32n);
    try {
        const resultBytes = new Uint8Array(exports.memory.buffer, resultPointer, resultLength);
        return JSON.parse(new TextDecoder().decode(resultBytes));
    } finally {
        exports.free_result(resultPointer, resultLength);
    }
}

test('NWE max outside distance measures the largest wick excursion after a band touch', async () => {
    const exports = await loadWasm();
    const candles = [
        { open: 9.6, high: 10.0, low: 9.5, close: 9.8, volume: 1, time: 1 },
        { open: 10.0, high: 11.0, low: 10.1, close: 10.5, volume: 1, time: 2 },
        { open: 10.4, high: 11.5, low: 9.7, close: 9.9, volume: 1, time: 3 },
        { open: 8.7, high: 9.4, low: 8.0, close: 8.2, volume: 1, time: 4 },
        { open: 8.3, high: 8.4, low: 7.5, close: 8.0, volume: 1, time: 5 },
        { open: 8.2, high: 9.0, low: 8.0, close: 8.8, volume: 1, time: 6 },
    ];
    const output = runIndicator(exports, candles, [10, 10, 10, 11, 11, 11], [8, 8, 8, 8.5, 8.5, 8.7]);

    assert.equal(output.valid, true, JSON.stringify(output.diagnostics));
    assert.equal(output.plots.find(plot => plot.title === 'Max above upper band (%)').values[2], 15);
    assert.ok(Math.abs(output.plots.find(plot => plot.title === 'Max below lower band (%)').values[5] - (1 / 8.5 * 100)) < 1e-9);
});

test('NWE max outside distance stays empty when no candle touches either band', async () => {
    const exports = await loadWasm();
    const candles = [
        { open: 9.5, high: 10, low: 9, close: 9.8, volume: 1, time: 1 },
        { open: 10, high: 11, low: 9.5, close: 10.2, volume: 1, time: 2 },
    ];
    const output = runIndicator(exports, candles, [20, 20], [5, 5]);

    assert.equal(output.valid, true, JSON.stringify(output.diagnostics));
    assert.deepEqual(output.plots.map(plot => plot.values), [[null, null], [null, null]]);
});
