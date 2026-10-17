import test from 'node:test';
import assert from 'node:assert/strict';
import { createSignalScreenerCard } from './signal-screener-card.js';

function escapeHtml(value) {
    return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
}

test('signal card presenter preserves stale labels, score basis, provenance, and escaping', () => {
    const renderSignalCard = createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: () => 'Stockbit',
        scoreModelFor: () => ({ short_label: 'Daily DSS', score_scale_max: 40 }),
        signalScoreState: () => ({ label: 'STALE · NO MARKET ENTRY · HELD', tone: 'stale', current: false }),
        signalScorePresentation: () => ({ scoreScaleMax: 40 }),
        signalEntryScore: () => 0,
        signalScoreWeightSummary: () => 'Strategy 50%; confluence 50%',
        signalPriceBarAgeLabel: value => value ? 'fresh' : 'age unavailable',
        formatPrice: () => '9500',
        ageLabel: () => '1 minute ago',
        renderScoreBasis: () => '<div class="score-basis">details</div>',
    });

    const markup = renderSignalCard({
        signal_id: 'signal-"quoted"',
        symbol: '<BBCA>',
        timeframe: '1d',
        direction: 'LONG',
        evidence_status: 'RESEARCH_ONLY',
        product_id: 'stockbit_regular',
        exchange_id: 'stockbit',
        signal_strategy_name: 'DSS',
        score: 12,
        score_bar_time: 1,
        price_bar_time: 1,
        last_updated_at: 1,
        current_price: 9500,
        data_provider: 'authorized_provider',
        data_source: 'venue_native',
        data_quality: 'validated',
        score_basis: { components: [{ indicator: 'DSS', state: 'near_cross', points: 6 }] },
    }, 1, true);

    assert.match(markup, /data-signal-id="signal-&quot;quoted&quot;"/);
    assert.match(markup, /aria-pressed="true"/);
    assert.match(markup, /&lt;BBCA&gt;/);
    assert.match(markup, /LAST LONG/);
    assert.match(markup, /STALE · NO MARKET ENTRY · HELD/);
    assert.match(markup, /Market-entry score <strong>0\.0<\/strong>/);
    assert.match(markup, /RESEARCH ONLY/);
    assert.match(markup, /authorized provider · venue native · validated/);
    assert.match(markup, /score-basis/);
});
