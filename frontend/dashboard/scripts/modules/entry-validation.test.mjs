import test from 'node:test';
import assert from 'node:assert/strict';
import { collectScope, metric, normalizeList, officialFeeSourceUrl, processQueue, reportModelState, riskRewardRuleForBacktest } from './entry-validation.js';

test('missing and malformed financial metrics remain unavailable rather than zero', () => {
    for (const value of [null, undefined, '', ' ', false, true, {}, [], NaN, Infinity]) assert.equal(metric(value), '—');
    assert.equal(metric(0, '%'), '0.00%');
    assert.equal(metric(-1.25, '%'), '-1.25%');
    assert.equal(metric(1500, '', 0), '1,500');
});

test('official fee sources accept only HTTPS links on supported venue domains', () => {
    assert.equal(officialFeeSourceUrl('https://pluang.com/biaya/crypto-futures'), 'https://pluang.com/biaya/crypto-futures');
    assert.equal(officialFeeSourceUrl('http://pluang.com/biaya'), null);
    assert.equal(officialFeeSourceUrl('https://example.com/fees'), null);
    assert.equal(officialFeeSourceUrl(null), null);
});

test('completed research is separate from validation and old protocol evidence stays pending', () => {
    assert.equal(reportModelState({ backtest: { status: 'HOLDOUT_RESEARCH_COMPLETE' } }, 'v1', 'v1').tone, 'pending');
    assert.equal(reportModelState({ entry_validation_status: 'VALIDATED_HOLDOUT' }, 'v1', 'v1').tone, 'valid');
    assert.deepEqual(reportModelState({ entry_validation_status: 'VALIDATED_HOLDOUT' }, 'v2', 'v1'), { label: 'Outdated protocol', tone: 'pending' });
    assert.equal(reportModelState({ status: 'INSUFFICIENT_HISTORY' }, 'v1', 'v1').label, 'Insufficient history');
});

test('4h status reads the current after-fee rule from the weight-learning result', () => {
    const rule = { protocol_id: 'entry_score_signed_risk_reward_after_fee_v7', status: 'INSUFFICIENT_HISTORY' };
    const backtest = { status: 'HOLDOUT_RESEARCH_COMPLETE', weight_learning: { risk_reward_rule: rule } };
    assert.equal(riskRewardRuleForBacktest(backtest), rule);
    assert.deepEqual(reportModelState({ backtest }, 'v1', 'v1', '4h'), {
        label: 'Research complete · not validated', tone: 'pending',
    });
    assert.equal(reportModelState({ backtest: { status: 'HOLDOUT_RESEARCH_COMPLETE', risk_reward_rule: rule } }, 'v1', 'v1', '4h').label, 'R:R must be rerun');
});

test('malformed pages and non-advancing cursors do not look like an empty success', () => {
    for (const payload of [null, {}, { rows: [], total: null }, { rows: [], total: -1 }, { rows: [], total: 50, has_more: true, offset: 0, next_offset: 50 }, { rows: [{}], total: 50, has_more: true, offset: 0, next_offset: 0 }]) {
        assert.throws(() => normalizeList(payload));
    }
    assert.equal(normalizeList({ rows: [], total: 0 }).total, 0);
});

test('batch scope walks every page beyond 100 tickers and excludes unverified identities', async () => {
    const requests = [];
    const progress = [];
    const rows = Array.from({ length: 127 }, (_, index) => ({ instrument_id: `id-${index}`, symbol: `T${index}`, research_eligibility: index === 120 ? 'pending_venue_verification' : index === 125 ? 'excluded' : 'eligible' }));
    const result = await collectScope(async offset => {
        requests.push(offset);
        return { rows: rows.slice(offset, offset + 50), total: rows.length, offset, next_offset: offset + 50, has_more: offset + 50 < rows.length };
    }, { onPage: value => progress.push(value) });
    assert.deepEqual(requests, [0, 50, 100]);
    assert.equal(result.eligible.length, 125);
    assert.equal(result.skipped, 2);
    assert.equal(result.eligible.at(-1).instrument_id, 'id-126');
    assert.equal(progress.at(-1).loaded, 127);
    assert.equal(result.stopped, false);
});

test('scope collection rejects a repeated page even when the response lies about its offset', async () => {
    let requests = 0;
    await assert.rejects(collectScope(async () => {
        requests++;
        return { rows: [{ instrument_id: 'one', research_eligibility: 'eligible' }], total: 3, offset: 0, next_offset: 1, has_more: true };
    }), /repeated a page/);
    assert.equal(requests, 2);
});

test('queue executes sequentially, preserves per-ticker failures, and continues the remaining scope', async () => {
    let active = 0;
    let maximumActive = 0;
    const processed = [];
    const progress = [];
    const result = await processQueue([{ instrument_id: 'a' }, { instrument_id: 'b' }, { instrument_id: 'c' }], {
        run: async row => {
            active++; maximumActive = Math.max(active, maximumActive);
            await new Promise(resolve => setImmediate(resolve));
            active--;
            if (row.instrument_id === 'b') throw Error('worker busy');
            return { report_id: row.instrument_id };
        },
        onResult: (row, report) => processed.push([row.instrument_id, report.report_id]),
        onProgress: value => progress.push(value),
    });
    assert.equal(maximumActive, 1);
    assert.deepEqual(processed, [['a', 'a'], ['c', 'c']]);
    assert.equal(result.processed, 3);
    assert.equal(result.failures[0].instrument_id, 'b');
    assert.match(result.failures[0].error, /worker busy/);
    assert.equal(result.stopped, false);
    assert.equal(progress.at(-1).processed, 3);
});

test('stop finishes the current ticker and does not start another one', async () => {
    let stopping = false;
    const called = [];
    const result = await processQueue([{ instrument_id: 'a' }, { instrument_id: 'b' }], {
        shouldStop: () => stopping,
        run: async row => { called.push(row.instrument_id); stopping = true; return {}; },
    });
    assert.deepEqual(called, ['a']);
    assert.equal(result.processed, 1);
    assert.equal(result.stopped, true);
});
