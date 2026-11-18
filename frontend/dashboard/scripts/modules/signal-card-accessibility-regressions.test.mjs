import test from 'node:test';
import assert from 'node:assert/strict';
import { createSignalScreenerCard } from './signal-screener-card.js';

function escapeHtml(value) {
    return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
}

function makeRenderer(signalScoreState) {
    return createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: exchange => ({ pintu: 'Pintu' })[exchange] || exchange,
        scoreModelFor: signal => ({ short_label: signal.modelLabel, score_scale_max: 20 }),
        signalScoreState,
        signalScorePresentation: () => ({ scoreScaleMax: 20, legacy: false }),
        signalDisplayScore: () => 12,
        signalScoreWeightSummary: () => 'Strategy 50%; confluence 50%',
        signalPriceBarAgeLabel: () => 'fresh',
        formatPrice: () => '100.00',
        ageLabel: () => '1 minute ago',
        renderScoreBasis: () => '',
    });
}

const signal = overrides => ({
    signal_id: 'signal-1',
    symbol: 'BTC-USD',
    timeframe: '4h',
    direction: 'LONG',
    exchange_id: 'pintu',
    product_id: 'pintu_pro_spot',
    modelLabel: 'NWE / DSS',
    signal_strategy_name: 'Signal model',
    current_price: 100,
    ...overrides,
});

test('signal row and details names distinguish matching tickers with different models', () => {
    const render = makeRenderer(() => ({ label: 'MARKET ENTRY NOW', tone: 'current', current: true }));
    const first = render(signal({ modelLabel: 'NWE / DSS' }), 1, false);
    const second = render(signal({ signal_id: 'signal-2', modelLabel: 'DSS Near Cross' }), 2, false);
    const accessibleNames = markup => [...markup.matchAll(/aria-label="([^"]+)"/g)].map(match => match[1]);
    const firstNames = accessibleNames(first);
    const secondNames = accessibleNames(second);

    assert.match(firstNames[0], /Pintu.*pintu pro spot.*NWE \/ DSS/);
    assert.match(firstNames[1], /Details for Pintu.*pintu pro spot.*NWE \/ DSS/);
    assert.notEqual(firstNames[0], secondNames[0]);
    assert.notEqual(firstNames[1], secondNames[1]);
});

test('compact status keeps held and no-entry score details visible', () => {
    const render = makeRenderer(signal => ({
        label: signal.score_status === 'stale'
            ? 'STALE · NO MARKET ENTRY · HELD'
            : 'NO MARKET ENTRY · SCORE 0',
        tone: signal.score_status === 'stale' ? 'stale' : 'inactive',
        current: false,
    }));
    const stale = render(signal({ persistent_score: true, score_status: 'stale' }), 1, false);
    const noEntry = render(signal({ persistent_score: true, score_status: 'not_qualifying' }), 1, false);

    assert.match(stale, /STALE · NO MARKET ENTRY · HELD/);
    assert.match(noEntry, /NO MARKET ENTRY · SCORE 0/);
});

test('a score state without a label and a row without product metadata still render safely', () => {
    const render = makeRenderer(() => ({ current: true }));
    const markup = render(signal({ product_id: undefined }), 1, false);

    assert.match(markup, /signal status unavailable/i);
    assert.match(markup, /Pintu.*NWE \/ DSS/);
});
