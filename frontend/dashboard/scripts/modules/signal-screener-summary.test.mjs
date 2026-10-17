import test from 'node:test';
import assert from 'node:assert/strict';
import { createSignalScreenerSummary } from './signal-screener-summary.js';

test('summary KPI renderer defaults to current visible signals and escapes its markup', () => {
    const grid = { innerHTML: '' };
    const previousDocument = globalThis.document;
    globalThis.document = { getElementById: id => id === 'kpi-grid' ? grid : null };
    const renderer = createSignalScreenerSummary({
        getSummary: () => ({}),
        getRequestState: () => 'ready',
        getRequestError: () => '',
        getRows: () => [],
        getVisibleSignals: () => [{ direction: 'LONG', score: 1 }],
        stockbitSignalModelsForView: () => [],
        cryptoSignalModelsForView: () => [],
        signalPriceCoverageMessage: () => '',
        ageLabel: () => '',
        formatTime: () => '',
        signalScoreState: () => null,
        scoreModelFor: () => null,
        averageNormalizedScore: () => 42,
        escape: value => String(value).replaceAll('<', '&lt;').replaceAll('>', '&gt;'),
    });

    try {
        renderer.renderKpis();
        assert.match(grid.innerHTML, /SIGNAL ROWS<\/p><strong>1<\/strong>/);
        assert.match(grid.innerHTML, /LONG MARKET ENTRIES<\/p><strong>1<\/strong>/);
        assert.match(grid.innerHTML, /MEAN SETUP STRENGTH<\/p><strong>42\.0%<\/strong>/);
    } finally {
        globalThis.document = previousDocument;
    }
});
