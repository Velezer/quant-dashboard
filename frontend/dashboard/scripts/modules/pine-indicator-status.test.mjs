import test from 'node:test';
import assert from 'node:assert/strict';
import { createPineIndicatorStatus } from './pine-indicator-status.js';

function createElement() {
    return {
        children: [],
        append(child) { this.children.push(child); },
        replaceChildren(...children) { this.children = children; },
        dataset: {},
        textContent: '',
    };
}

test('Pine status preserves state styling and distinguishes errors from approximations', () => {
    const status = createElement();
    const diagnosticList = createElement();
    const previousDocument = globalThis.document;
    globalThis.document = { createElement };
    const presenter = createPineIndicatorStatus({ status, diagnosticList });

    try {
        presenter.setStatus('Evaluating saved source…', 'loading');
        assert.equal(status.textContent, 'Evaluating saved source…');
        assert.equal(status.dataset.state, 'loading');
        assert.match(status.className, /text-amber-300/);

        presenter.showDiagnostics(
            [{ severity: 'error', line: 4, column: 2, message: 'unsupported order' }],
            ['bar magnifier uses bounded OHLC replay'],
        );
        assert.equal(diagnosticList.children.length, 1);
        assert.deepEqual(diagnosticList.children[0].children.map(item => item.textContent), [
            'Unsupported/error · 4:2: unsupported order',
            'Known approximation: bar magnifier uses bounded OHLC replay',
        ]);
        assert.deepEqual(diagnosticList.children[0].children.map(item => item.className), [
            'text-rose-300',
            'text-amber-200',
        ]);

        presenter.showDiagnostics();
        assert.match(diagnosticList.textContent, /No parse errors/);
    } finally {
        globalThis.document = previousDocument;
    }
});
