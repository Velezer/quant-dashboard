import test from 'node:test';
import assert from 'node:assert/strict';
import { appSignalReadiness } from './signal-readiness-model.js';

const holdout = (rate, trades = 40) => ({
    win_rate_after_fees_pct: rate,
    win_rate_after_fees_wilson_95_lower_pct: rate - 10,
    mean_net_return_pct: 0.25,
    profit_factor: 1.4,
    trades,
});

const evaluation = overrides => ({
    evaluated_at: 200,
    decision: 'BLOCKED',
    fees_applied: true,
    scope: {
        score_model_id: 'six_indicator_v1',
        asset_category_id: 'crypto_spot',
        timeframe: '4h',
        source_family: 'venue_native',
    },
    holdout: holdout(99, 500),
    product_slices: [
        { exchange_id: 'pintu', product_id: 'pintu_pro_spot', holdout: holdout(62) },
        { exchange_id: 'tokocrypto', product_id: 'tokocrypto_spot', holdout: holdout(91) },
    ],
    blockers: ['Product coverage below target.'],
    ...overrides,
});

const metricFor = (app, product, timeframe, evaluations) => {
    const result = appSignalReadiness(app, null, {
        live_scoring: { latest_evaluations: evaluations },
    }, { product, timeframe });
    return result.metrics.find(metric => metric.label.startsWith('Latest after-fee holdout')) || null;
};

test('readiness uses the exact selected app and product holdout slice', () => {
    const metric = metricFor('pintu', 'pintu_pro_spot', '4h', [evaluation()]);

    assert.match(metric.label, /pintu pro spot/);
    assert.match(metric.value, /^\+0\.250% mean net\/trade · PF 1\.40 · 62\.0% wins \(descriptive\)/);
    assert.doesNotMatch(metric.value, /99\.0%|91\.0%/);
    assert.match(metric.detail, /Pintu · pintu pro spot · crypto_spot · 4H · six_indicator_v1 · venue_native/);
    assert.match(metric.detail, /transaction fees applied/);
});

test('a blocked evaluation cannot be presented as a promoted target result', () => {
    const metric = metricFor('pintu', 'pintu_pro_spot', '4h', [evaluation()]);

    assert.match(metric.value, /blocked evaluation/);
    assert.doesNotMatch(metric.value, /target/);
    assert.match(metric.detail, /Blockers: Product coverage below target\./);
});

test('all-products view chooses the latest exact slice and never borrows global or another-app data', () => {
    const older = evaluation({
        evaluated_at: 100,
        product_slices: [
            { exchange_id: 'pintu', product_id: 'pintu_pro_spot', holdout: holdout(52) },
        ],
    });
    const latest = evaluation({
        evaluated_at: 300,
        product_slices: [
            { exchange_id: 'pintu', product_id: 'pintu_pro_futures', holdout: holdout(57) },
            { exchange_id: 'tokocrypto', product_id: 'tokocrypto_spot', holdout: holdout(96) },
        ],
    });
    const metric = metricFor('pintu', 'all', '4h', [older, latest]);

    assert.match(metric.label, /pintu pro futures/);
    assert.match(metric.value, /^\+0\.250% mean net\/trade · PF 1\.40 · 57\.0% wins \(descriptive\)/);
    assert.doesNotMatch(metric.value, /99\.0%|96\.0%|52\.0%/);
});

test('readiness hides holdouts without explicit fee and decision provenance or exact product slices', () => {
    assert.equal(metricFor('pintu', 'pintu_pro_spot', '4h', [evaluation({ fees_applied: false })]), null);
    assert.equal(metricFor('pintu', 'pintu_pro_spot', '4h', [evaluation({ decision: 'UNKNOWN' })]), null);
    assert.equal(metricFor('pintu', 'pintu_pro_spot', '4h', [evaluation({ product_slices: [] })]), null);
});
