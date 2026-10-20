import test from 'node:test';
import assert from 'node:assert/strict';
import { createSignalScreenerCard } from './signal-screener-card.js';
import { signalDisplayScore, signalScoreState } from './signal-screener-model.js';

function escapeHtml(value) {
    return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
}

test('signal card presenter preserves stale labels, score basis, provenance, and escaping', () => {
    const renderSignalCard = createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: () => 'Stockbit',
        scoreModelFor: () => ({ short_label: 'Daily DSS', score_scale_max: 40 }),
        signalScoreState,
        signalScorePresentation: () => ({ scoreScaleMax: 40 }),
        signalDisplayScore,
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
        persistent_score: true,
        score_status: 'stale',
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
    assert.match(markup, /Last recorded score <strong>12\.0<\/strong>/);
    assert.doesNotMatch(markup, /Market-entry score <strong>0\.0<\/strong>/);
    assert.match(markup, /no current market-entry signal, last recorded score 12\.0 out of 40/);
    assert.match(markup, /RESEARCH ONLY/);
    assert.match(markup, /authorized provider · venue native · validated/);
    assert.match(markup, /score-basis/);
});

test('signal cards distinguish a fresh non-qualifying zero from an unavailable score', () => {
    const renderSignalCard = createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: () => 'Stockbit',
        scoreModelFor: () => ({ short_label: 'Daily DSS', score_scale_max: 40 }),
        signalScoreState,
        signalScorePresentation: () => ({ scoreScaleMax: 40 }),
        signalDisplayScore,
        signalScoreWeightSummary: () => 'Price 50%; volume 50%',
        signalPriceBarAgeLabel: () => 'fresh',
        formatPrice: () => '9500',
        ageLabel: () => '1 minute ago',
        renderScoreBasis: () => '',
    });
    const signal = {
        signal_id: 'stockbit-bbca-daily', symbol: 'BBCA', timeframe: '1d',
        direction: 'LONG', product_id: 'stockbit_regular', exchange_id: 'stockbit',
        persistent_score: true, score_status: 'not_qualifying', score: 0,
    };
    const nonQualifying = renderSignalCard(signal, 1, false);
    assert.match(nonQualifying, /NO MARKET ENTRY · SCORE 0/);
    assert.match(nonQualifying, /Market-entry score <strong>0\.0<\/strong>/);
    assert.doesNotMatch(nonQualifying, /Last recorded score/);

    const unavailable = renderSignalCard({ ...signal, score_status: 'stale', score: null }, 1, false);
    assert.match(unavailable, /Last recorded score <strong>—<\/strong>/);
    assert.match(unavailable, /no current market-entry signal, last recorded score unavailable/);
    assert.doesNotMatch(unavailable, /<strong>0\.0<\/strong>/);
});
