import test from 'node:test';
import assert from 'node:assert/strict';
import { meanInterval, rankCorrelation, summarizeCohorts } from './gainer-statistics.js';

test('rank correlation averages ties and leaves constant rankings unavailable', () => {
    assert.equal(rankCorrelation([1,2,3].map(score => ({ score, gross_return: score }))), 1);
    assert.equal(rankCorrelation([1,2,3].map(score => ({ score, gross_return: -score }))), -1);
    assert.equal(rankCorrelation([1,1,1].map(score => ({ score, gross_return: score }))), null);
    assert.equal(rankCorrelation([1,1,2].map(score => ({ score, gross_return: score }))), 1);
});

test('uncertainty requires 30 observations and responds to dispersion', () => {
    assert.equal(meanInterval([1,2,3]).lower, null);
    assert.deepEqual(meanInterval(Array(30).fill(2)), { count: 30, mean: 2, lower: 2, upper: 2 });
    const mixed = meanInterval(Array.from({length:60}, (_, i) => i % 2 ? 2 : -1));
    assert.equal(mixed.mean, 0.5); assert.ok(mixed.lower < mixed.mean && mixed.upper > mixed.mean);
});

function cohort(time, count = 10) {
    return Array.from({length:count}, (_, index) => ({ symbol: String(index), score: index + 1, eligible: true, momentum: -index,
        gross_return: index / 100, net_return: index - 1, signal_time: time - 1, exit_time: time + 1 }));
}

test('common splits purge observations across boundaries and compare identical cohorts', () => {
    const cohorts = new Map(Array.from({length:100}, (_, index) => [index * 10, cohort(index * 10)]));
    const report = summarizeCohorts(cohorts, 10);
    assert.equal(report.splits.training.count, 60);
    assert.equal(report.splits.validation.count, 19);
    assert.equal(report.splits.holdout.count, 19);
    assert.equal(report.purged, 20);
    assert.equal(report.metrics.holdout.top5.mean, 6);
    assert.equal(report.metrics.holdout.universe.mean, 3.5);
    assert.equal(report.metrics.holdout.excessUniverse.mean, 2.5);
    assert.equal(report.metrics.holdout.rankCorrelation.mean, 1);
    assert.equal(report.metrics.holdout.decilePrecision.mean, 20);
});

test('partial universe coverage cannot become successful evidence', () => {
    const report = summarizeCohorts(new Map([[10,cohort(10)]]), 100);
    assert.equal(report.metrics.holdout.top1.mean, null);
    assert.equal(report.splits.holdout.count, 0);
});

test('signed bearish scores do not enter a long gainer ranking', () => {
    const rows = cohort(10).map(row => ({...row, score: -1, eligible: false}));
    const report = summarizeCohorts(new Map([[10, rows]]), 10);
    assert.equal(report.splits.holdout.count, 0);
});
