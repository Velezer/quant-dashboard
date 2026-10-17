import test from 'node:test';
import assert from 'node:assert/strict';
import { createPineStrategyInputs } from './pine-strategy-inputs.js';

class FakeElement {
    constructor(tagName) {
        this.tagName = tagName;
        this.children = [];
        this.dataset = {};
        this.validity = { badInput: false };
        this.value = '';
        this.type = '';
    }

    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = [...children]; }
    querySelectorAll(selector) {
        const matches = [];
        const visit = element => {
            for (const child of element.children || []) {
                if (selector === '[data-pine-input-title]' && child.dataset?.pineInputTitle) matches.push(child);
                visit(child);
            }
        };
        visit(this);
        return matches;
    }
    checkValidity() { return this.valid !== false; }
    reportValidity() { this.reportedValidity = true; }
}

async function withFakeDocument(run) {
    const originalDocument = globalThis.document;
    globalThis.document = {
        createElement: tagName => new FakeElement(tagName),
        createTextNode: textContent => ({ textContent }),
    };
    try {
        return await run();
    } finally {
        globalThis.document = originalDocument;
    }
}

test('Pine strategy input controller renders declared types and collects validated overrides', async () => {
    await withFakeDocument(async () => {
        const select = { value: '7' };
        const inputControls = new FakeElement('section');
        const stateChanges = [];
        const controller = createPineStrategyInputs({
            select,
            inputControls,
            onStateChange: () => stateChanges.push(controller.isLoading),
            requestJson: async (url, options) => {
                assert.equal(url, '/api/pine/strategies?id=7');
                assert.ok(options.signal);
                return { strategies: [{ inputs: [
                    { title: 'Enabled', input_type: 'bool', default_value: true },
                    { title: 'Count', input_type: 'int', default_value: 3, min_value: 1, max_value: 8 },
                    { title: 'Ratio', input_type: 'float', default_value: 1.5 },
                    { title: 'Label', input_type: 'string', default_value: 'base' },
                ] }] };
            },
        });

        await controller.load('7');

        const controls = inputControls.querySelectorAll('[data-pine-input-title]');
        assert.deepEqual(controls.map(control => [control.dataset.pineInputTitle, control.type]), [
            ['Enabled', 'checkbox'], ['Count', 'number'], ['Ratio', 'number'], ['Label', 'text'],
        ]);
        assert.equal(controls[1].step, '1');
        assert.equal(controls[1].min, '1');
        assert.equal(controls[1].max, '8');
        assert.deepEqual(controller.collectOverrides(), { Enabled: true, Count: 3, Ratio: 1.5, Label: 'base' });
        assert.deepEqual(stateChanges, [true, false]);
    });
});

test('Pine strategy input controller rejects invalid numeric overrides', async () => {
    await withFakeDocument(async () => {
        const inputControls = new FakeElement('section');
        const numeric = new FakeElement('input');
        numeric.type = 'number';
        numeric.value = '';
        numeric.dataset.pineInputTitle = 'Length';
        numeric.dataset.pineInputType = 'int';
        inputControls.append(numeric);
        const controller = createPineStrategyInputs({ inputControls, requestJson: async () => ({}), onStateChange() {} });

        assert.throws(() => controller.collectOverrides(), /Enter a valid value for “Length”/);
        assert.equal(numeric.reportedValidity, true);
    });
});

test('Pine strategy input controller shows a retryable unavailable state', async () => {
    await withFakeDocument(async () => {
        const inputControls = new FakeElement('section');
        const controller = createPineStrategyInputs({
            select: { value: '9' },
            inputControls,
            onStateChange() {},
            requestJson: async () => { throw new Error('service unavailable'); },
        });

        await controller.load('9');

        assert.equal(controller.isLoading, false);
        assert.match(inputControls.children[0].textContent, /Strategy inputs unavailable: service unavailable/);
    });
});
