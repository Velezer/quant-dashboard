export function createPineStrategyInputs({ select, inputControls, requestJson, onStateChange }) {
    let controller;
    let loading = false;

    function collectOverrides() {
        const overrides = {};
        for (const control of inputControls?.querySelectorAll('[data-pine-input-title]') || []) {
            const title = control.dataset.pineInputTitle;
            if (control.type === 'checkbox') {
                overrides[title] = control.checked;
            } else if (control.type === 'number') {
                const value = Number(control.value);
                if (!control.value || !Number.isFinite(value) || control.validity.badInput || !control.checkValidity()) {
                    control.reportValidity();
                    throw new Error(`Enter a valid value for “${title}” within its declared range.`);
                }
                overrides[title] = control.dataset.pineInputType === 'int' ? Math.trunc(value) : value;
            } else {
                overrides[title] = control.value;
            }
        }
        return overrides;
    }

    async function load(strategyId) {
        controller?.abort();
        inputControls?.replaceChildren();
        if (!strategyId || !inputControls) {
            loading = false;
            onStateChange();
            return;
        }

        loading = true;
        const requestController = new AbortController();
        controller = requestController;
        onStateChange();
        const loadingMessage = document.createElement('p');
        loadingMessage.className = 'text-[10px] text-gray-500';
        loadingMessage.textContent = 'Loading declared Pine inputs…';
        inputControls.append(loadingMessage);

        try {
            const payload = await requestJson(`/api/pine/strategies?id=${encodeURIComponent(strategyId)}`, {
                signal: requestController.signal,
            });
            if (select.value !== strategyId) return;
            inputControls.replaceChildren();
            const entry = Array.isArray(payload?.strategies) ? payload.strategies[0] : null;
            const definitions = Array.isArray(entry?.inputs) ? entry.inputs : [];
            if (!definitions.length) {
                const note = document.createElement('p');
                note.className = 'text-[10px] text-gray-500';
                note.textContent = 'This strategy has no supported literal input controls; declared source defaults will be used.';
                inputControls.append(note);
                return;
            }
            for (const definition of definitions) {
                if (!definition || typeof definition.title !== 'string') continue;
                const label = document.createElement('label');
                let control;
                if (definition.input_type === 'bool') {
                    label.className = 'inline-flex items-center gap-2 rounded border border-gray-800 px-2 py-2 text-[10px] text-gray-300';
                    control = document.createElement('input');
                    control.type = 'checkbox';
                    control.checked = definition.default_value === true;
                    control.className = 'accent-violet-500';
                    label.append(control, document.createTextNode(` ${definition.title}`));
                } else {
                    label.className = 'text-[10px] text-gray-400';
                    label.append(document.createTextNode(definition.title));
                    control = document.createElement('input');
                    control.type = definition.input_type === 'string' ? 'text' : 'number';
                    control.value = String(definition.default_value ?? '');
                    control.className = 'mt-1 block w-full rounded border border-gray-700 bg-gray-950 px-2 py-1.5 text-[10px] text-gray-200';
                    if (definition.input_type === 'int') control.step = '1';
                    if (definition.input_type === 'float') control.step = 'any';
                    if (Number.isFinite(definition.min_value)) control.min = String(definition.min_value);
                    if (Number.isFinite(definition.max_value)) control.max = String(definition.max_value);
                    label.append(control);
                }
                control.dataset.pineInputTitle = definition.title;
                control.dataset.pineInputType = definition.input_type;
                inputControls.append(label);
            }
        } catch (error) {
            if (error?.name === 'AbortError') return;
            const note = document.createElement('p');
            note.className = 'text-[10px] text-rose-300';
            note.textContent = `Strategy inputs unavailable: ${error.message}. Reload the strategy list and try again.`;
            inputControls.replaceChildren(note);
        } finally {
            if (controller === requestController) {
                controller = null;
                loading = false;
                onStateChange();
            }
        }
    }

    function abortLoad() {
        controller?.abort();
        controller = null;
        loading = false;
        inputControls?.replaceChildren();
        onStateChange();
    }

    return {
        abortLoad,
        collectOverrides,
        get isLoading() { return loading; },
        load,
    };
}
