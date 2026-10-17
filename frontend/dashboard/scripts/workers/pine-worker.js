let wasmInstance;
let initialization = null;

async function loadRuntime() {
    const response = await fetch('/web/wasm/quant_pine_runtime.wasm', { cache: 'no-store' });
    if (!response.ok) throw new Error(`WASM asset request failed (${response.status}).`);
    let instantiated;
    try {
        instantiated = await WebAssembly.instantiateStreaming(response.clone(), {});
    } catch {
        instantiated = await WebAssembly.instantiate(await response.arrayBuffer(), {});
    }
    wasmInstance = instantiated.instance;
}

function runSource(source, candles, timeframe = '1d', timeframes = {}, inputSources = {}, mode = 'indicator', inputs = {}) {
    if (!wasmInstance) throw new Error('Browser WASM runtime is not ready.');
    const input = new TextEncoder().encode(JSON.stringify({
        source,
        candles,
        timeframe,
        timeframes,
        input_sources: inputSources,
        mode,
        inputs,
    }));
    const exports = wasmInstance.exports;
    const inputPointer = exports.alloc(input.length);
    new Uint8Array(exports.memory.buffer, inputPointer, input.length).set(input);
    let packed;
    try {
        packed = exports.run(inputPointer, input.length);
    } finally {
        exports.dealloc(inputPointer, input.length);
    }
    const pointer = Number(packed & 0xffffffffn);
    const length = Number(packed >> 32n);
    try {
        const encoded = new Uint8Array(exports.memory.buffer, pointer, length);
        return JSON.parse(new TextDecoder().decode(encoded));
    } finally {
        exports.free_result(pointer, length);
    }
}

self.addEventListener('message', async event => {
    const { id, type, source, candles, timeframe, timeframes, input_sources: inputSources, mode, inputs } = event.data || {};
    if (type === 'init') {
        try {
            initialization ||= loadRuntime();
            await initialization;
            self.postMessage({ id, type: 'ready' });
        } catch (error) {
            initialization = null;
            self.postMessage({ id, type: 'error', error: String(error?.message || error) });
        }
        return;
    }
    if (type !== 'run') return;
    try {
        self.postMessage({ id, type: 'result', output: runSource(source, candles, timeframe, timeframes, inputSources, mode, inputs) });
    } catch (error) {
        self.postMessage({ id, type: 'error', error: String(error?.message || error) });
    }
});
