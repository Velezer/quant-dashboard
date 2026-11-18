import test from 'node:test';
import assert from 'node:assert/strict';
import { createSignalScoreBasisMarkup } from './signal-score-basis.js';
import { createSignalScreenerCard } from './signal-screener-card.js';
import { oppositeEntryExitCue, signalDirectionalScore, signalDisplayScore, signalScoreState, signalScoreSummary } from './signal-screener-model.js';

function escapeHtml(value) {
    return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
}

test('signal card presenter preserves stale labels, score basis, provenance, and escaping', () => {
    const renderSignalCard = createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: () => 'Stockbit',
        scoreModelFor: () => ({ short_label: 'Daily DSS', score_scale_max: 60 }),
        signalScoreState,
        signalScorePresentation: () => ({ scoreScaleMax: 60 }),
        signalDisplayScore,
        signalScoreWeightSummary: () => 'Strategy 50%; confluence 50%',
        signalPriceBarAgeLabel: value => value ? 'fresh' : 'age unavailable',
        formatPrice: () => '9500',
        ageLabel: () => '1 minute ago',
        renderScoreBasis: createSignalScoreBasisMarkup({ escape: escapeHtml, scoreModelFor: () => ({ score_scale_max: 60 }) }),
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
        setup_strength: 12,
        score_contract: 'signed_indicator_sum_v1',
        persistent_score: true,
        score_status: 'stale',
        score_bar_time: 1,
        price_bar_time: 1,
        last_updated_at: 1,
        current_price: 9500,
        data_provider: 'authorized_provider',
        data_source: 'venue_native',
        data_quality: 'validated',
        score_basis: {
            score_scale_max: 60,
            score_model_status: 'current',
            components: [
                { indicator: 'Nadaraya-Watson Envelope', state: 'UNAVAILABLE', points: null, weighted_points: 0, weight_points: 10 },
                { indicator: 'DSS Bressert', state: 'near_cross', points: 6, weighted_points: 6, weight_points: 10 },
                { indicator: 'Relative Volume', state: 'AVAILABLE', points: 0, weighted_points: 0, weight_points: 10 },
                { indicator: 'Estimated Volume Delta (OHLCV proxy)', state: 'ALIGNED', points: 5, weighted_points: 5, weight_points: 10 },
                { indicator: 'Markov Regime', state: 'BULLISH', points: 4, weighted_points: 4, weight_points: 10 },
                { indicator: 'Lag-one Return Autocorrelation', state: 'MEAN_REVERSION', points: -2, weighted_points: -2, weight_points: 10 },
            ],
        },
    }, 1, true);

    assert.match(markup, /data-signal-id="signal-&quot;quoted&quot;"/);
    assert.match(markup, /aria-pressed="true"/);
    assert.match(markup, /&lt;BBCA&gt;/);
    assert.match(markup, /LAST LONG/);
    assert.match(markup, /STALE · NO MARKET ENTRY · HELD/);
    assert.doesNotMatch(markup, /If holding/);
    assert.match(markup, /Six signed indicators/);
    assert.equal((markup.match(/class="signal-indicator-score"/g) || []).length, 6);
    assert.equal((markup.match(/class="signal-indicator-weight">17% weight/g) || []).length, 6);
    assert.match(markup, /6\.0<small>\/10<\/small>/);
    assert.match(markup, /—<small>\/10<\/small>/);
    assert.doesNotMatch(markup, /out of 40|\/40|\/ 40/);
    assert.doesNotMatch(markup, /Setup strength <strong>0\.0<\/strong>/);
    assert.ok(markup.includes('NWE —/10'));
    assert.ok(markup.includes('DSS Bressert +6.0/10'));
    assert.match(markup, /RESEARCH ONLY/);
    assert.match(markup, /authorized provider · venue native · validated/);
    assert.match(markup, /signal-score-basis/);
});

test('signal cards show signed direction score and keep setup strength distinct', () => {
    const renderSignalCard = createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: () => 'Pintu',
        scoreModelFor: () => ({ short_label: 'NWE/DSS', score_scale_max: 60 }),
        signalScoreState: () => ({ current: true }),
        signalScorePresentation: () => ({ scoreScaleMax: 60, legacy: false }),
        signalDisplayScore: () => 35.4,
        signalScoreWeightSummary: () => 'six indicators',
        signalPriceBarAgeLabel: () => 'fresh',
        formatPrice: () => '100.00',
        ageLabel: () => '1 minute ago',
        renderScoreBasis: () => '',
    });
    const longSignal = {
        signal_id: 'btc-long-4h', symbol: 'BTC-USD', timeframe: '4h', direction: 'LONG',
        exchange_id: 'pintu', product_id: 'pintu_pro_spot', signal_strategy_name: 'NWE/DSS',
        score: 50, score_contract: 'signed_indicator_sum_v1', setup_strength: 35.4,
        current_price: 100, price_bar_time: 1, last_updated_at: 1,
        price_is_fresh: true, price_fresh_until_ms: Date.now() + 60_000,
        score_basis: {},
        direction_score: { score: 50, net_score: 0, long_count: 1, short_count: 1 },
    };
    const longCard = renderSignalCard(longSignal, 1, true);
    assert.match(longCard, /LONG \+50\.0 · Net 0\.0 · 1L \/ 1S/);
    assert.match(longCard, /Setup strength <strong>35\.4<\/strong>/);
    assert.match(longCard, /Direction score \+50\.0 for this LONG signal/);
    assert.doesNotMatch(longCard, /Vote \+50|Weighted total/);
    assert.deepEqual(signalDirectionalScore(longSignal), { score: 50, netScore: 0, longCount: 1, shortCount: 1 });
    assert.match(signalScoreSummary(longSignal, { score_scale_max: 60 }), /^Direction score \+50\.0 · net 0\.0 · Setup strength 35\.4\/60/);

    const shortSignal = {
        ...longSignal,
        signal_id: 'btc-short-4h',
        direction: 'SHORT',
        score: -50,
        direction_score: { score: -50, net_score: 0, long_count: 1, short_count: 1 },
    };
    assert.match(renderSignalCard(shortSignal, 2, false), /SHORT −50\.0 · Net 0\.0 · 1L \/ 1S/);
    assert.equal(signalDisplayScore(shortSignal), 35.4);
    assert.deepEqual(signalDirectionalScore(shortSignal), { score: -50, netScore: 0, longCount: 1, shortCount: 1 });
    assert.equal(signalDirectionalScore({ ...shortSignal, direction: 'LONG' }), null);
});

test('signal cards distinguish a fresh non-qualifying zero from an unavailable score', () => {
    const renderSignalCard = createSignalScreenerCard({
        escape: escapeHtml,
        appLabel: () => 'Stockbit',
        scoreModelFor: () => ({ short_label: 'Daily DSS', score_scale_max: 20 }),
        signalScoreState,
        signalScorePresentation: () => ({ scoreScaleMax: 20 }),
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
    assert.match(nonQualifying, /Setup strength <strong>0\.0<\/strong>/);
    assert.doesNotMatch(nonQualifying, /Last recorded setup strength/);

    const unavailable = renderSignalCard({ ...signal, score_status: 'stale', score: null }, 1, false);
    assert.match(unavailable, /Last recorded setup strength <strong>—<\/strong>/);
    assert.match(unavailable, /no current market-entry signal, last recorded setup strength unavailable/);
    assert.doesNotMatch(unavailable, /<strong>0\.0<\/strong>/);

    const current = { ...signal, direction: 'SHORT', score_status: 'qualifying', score: 12, price_is_fresh: true, price_fresh_until_ms: Date.now() + 60_000 };
    assert.match(renderSignalCard(current, 1, false), /If holding LONG: exit cue/);
    assert.match(renderSignalCard({ ...current, score_bar_is_forming: true }, 1, false), /If holding LONG: exit watch/);
});

test('opposite entry produces only conditional and fresh exit cues', () => {
    const shortSignal = {
        direction: 'SHORT', score_status: 'qualifying', persistent_score: true,
        price_is_fresh: true, price_fresh_until_ms: 2_000, score_bar_is_forming: false,
    };
    assert.deepEqual(oppositeEntryExitCue(shortSignal, 1_000),
        { positionSide: 'LONG', signalDirection: 'SHORT', provisional: false });
    assert.deepEqual(oppositeEntryExitCue({ ...shortSignal, direction: 'LONG', score_bar_is_forming: true }, 1_000),
        { positionSide: 'SHORT', signalDirection: 'LONG', provisional: true });
    assert.equal(oppositeEntryExitCue({ ...shortSignal, score_status: 'stale' }, 1_000), null);
    assert.equal(oppositeEntryExitCue({ ...shortSignal, score_status: 'not_qualifying' }, 1_000), null);
    assert.equal(oppositeEntryExitCue({ ...shortSignal, price_is_fresh: false }, 1_000), null);
    const cryptoSignal = { ...shortSignal, persistent_score: false };
    assert.equal(oppositeEntryExitCue(cryptoSignal, 1_000)?.positionSide, 'LONG');
    assert.equal(oppositeEntryExitCue(cryptoSignal, 2_001), null);
    assert.equal(oppositeEntryExitCue(shortSignal, 2_001), null);
    assert.equal(oppositeEntryExitCue({ ...shortSignal, price_fresh_until_ms: null }, 1_000), null);
    assert.equal(oppositeEntryExitCue({ ...shortSignal, direction: 'UNKNOWN' }, 1_000), null);
});
