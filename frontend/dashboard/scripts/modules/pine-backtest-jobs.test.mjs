import test from 'node:test';
import assert from 'node:assert/strict';
import { createPineBacktestJobClient, parseBacktestDateRange } from './pine-backtest-jobs.js';

test('Pine backtest date range is UTC and includes the selected final date', () => {
    assert.deepEqual(parseBacktestDateRange('2025-01-01', '2025-01-31'), {
        from_time_ms: Date.UTC(2025, 0, 1),
        to_time_exclusive_ms: Date.UTC(2025, 1, 1),
    });
});

test('Pine backtest date range leaves latest-history behavior when both dates are blank', () => {
    assert.equal(parseBacktestDateRange('', ''), null);
});

test('Pine backtest date range rejects partial, malformed, or reversed dates', () => {
    assert.throws(() => parseBacktestDateRange('2025-01-01', ''), /Choose both UTC dates/);
    assert.throws(() => parseBacktestDateRange('2025-02-30', '2025-03-01'), /date range is invalid/);
    assert.throws(() => parseBacktestDateRange('2025-02-01', '2025-01-31'), /date range is invalid/);
});

test('backtest job polls status and returns the completed result field', async () => {
    const requests = [];
    const statusTimes = [];
    const states = [];
    const responses = [
        { state: 'queued', phase: 'queued', progress_pct: 0 },
        { state: 'completed', phase: 'completed', progress_pct: 100, result: { backtest: { closed_trades: 2 } } },
    ];
    const client = createPineBacktestJobClient(async (url, options = {}) => {
        requests.push({ url, options });
        if (url.includes('/jobs/')) statusTimes.push(Date.now());
        return url === '/api/pine/backtest' ? { job_id: 'job-1' } : responses.shift();
    });

    const result = await client.start({ strategy_id: 7 }, event => states.push(event));

    assert.equal(requests[0].url, '/api/pine/backtest');
    assert.equal(requests[0].options.method, 'POST');
    assert.match(requests[1].url, /\/api\/pine\/backtest\/jobs\/job-1$/);
    assert.ok(statusTimes[1] - statusTimes[0] >= 500, 'status polls must stay at least 500 ms apart');
    assert.deepEqual(result.result, { backtest: { closed_trades: 2 } });
    assert.deepEqual(states.map(event => event.state), ['queued', 'queued', 'completed']);
    assert.equal(client.busy, false);
});

test('user cancellation keeps polling until the service reports a terminal state', async () => {
    const requests = [];
    const states = [];
    let statusPolls = 0;
    const client = createPineBacktestJobClient(async url => {
        requests.push(url);
        if (url === '/api/pine/backtest') return { job_id: 'job-stop' };
        if (url.endsWith('/cancel')) return { state: 'running', phase: 'executing', progress_pct: 20 };
        statusPolls += 1;
        return statusPolls === 1
            ? { state: 'running', phase: 'executing', progress_pct: 20 }
            : { state: 'cancelled', phase: 'cancelled', progress_pct: 20 };
    });

    const running = client.start({}, event => {
        states.push(event);
        if (event.state === 'running' && !event.cancel_requested) void client.cancel();
    });
    const result = await running;

    assert.equal(result.state, 'cancelled');
    assert.ok(states.some(event => event.state === 'running' && event.cancel_requested));
    assert.ok(states.some(event => event.state === 'cancelled'));
    assert.equal(requests.filter(url => url.endsWith('/cancel')).length, 1);
    assert.equal(client.busy, false);
});

test('server cancelling state is accepted while cancellation completes', async () => {
    const states = [];
    let statusPolls = 0;
    const client = createPineBacktestJobClient(async url => {
        if (url === '/api/pine/backtest') return { job_id: 'job-cancelling' };
        if (url.endsWith('/cancel')) return { state: 'cancelling', phase: 'cancellation requested', progress_pct: 24 };
        statusPolls += 1;
        return statusPolls === 1
            ? { state: 'cancelling', phase: 'cancellation requested', progress_pct: 24 }
            : { state: 'cancelled', phase: 'cancelled', progress_pct: 24 };
    });

    const result = await client.start({}, event => {
        states.push(event);
        if (event.state === 'cancelling' && !event.cancel_requested) void client.cancel();
    });

    assert.equal(result.state, 'cancelled');
    assert.ok(states.some(event => event.state === 'cancelling' && event.cancel_requested));
    assert.equal(client.busy, false);
});

test('context invalidation aborts status polling and requests a stop without claiming success', async () => {
    const states = [];
    let pollSignal;
    let getStarted;
    const getStartedPromise = new Promise(resolve => { getStarted = resolve; });
    const client = createPineBacktestJobClient(async (url, options = {}) => {
        if (url === '/api/pine/backtest') return { job_id: 'job-2' };
        if (url.endsWith('/cancel')) return { state: 'running', phase: 'executing', progress_pct: 31 };
        getStarted();
        return new Promise((resolve, reject) => {
            pollSignal = options.signal;
            options.signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')), { once: true });
        });
    });

    const running = client.start({}, event => states.push(event));
    await getStartedPromise;
    client.invalidate();
    const result = await running;
    await new Promise(resolve => setTimeout(resolve, 0));

    assert.equal(result.state, 'detached');
    assert.equal(pollSignal.aborted, true);
    assert.equal(states.some(event => event.state === 'cancelled'), false);
    assert.equal(states.some(event => event.state === 'running' && event.cancel_requested), true);
    assert.equal(client.busy, true);
});

test('requesting a stop after unavailable status resumes polling for recovery', async () => {
    const states = [];
    let statusPolls = 0;
    let resolveTerminal;
    const terminal = new Promise(resolve => { resolveTerminal = resolve; });
    const client = createPineBacktestJobClient(async url => {
        if (url === '/api/pine/backtest') return { job_id: 'job-recovery' };
        if (url.endsWith('/cancel')) return { state: 'running', phase: 'executing', progress_pct: 41 };
        statusPolls += 1;
        if (statusPolls === 1) throw new Error('temporary status outage');
        return { state: 'cancelled', phase: 'cancelled', progress_pct: 41 };
    });

    const startResult = await client.start({}, event => {
        states.push(event);
        if (event.state === 'cancelled') resolveTerminal(event);
    });
    assert.equal(startResult.state, 'status_unavailable');
    await client.cancel();
    await terminal;

    assert.equal(statusPolls, 2);
    assert.equal(states.some(event => event.state === 'running' && event.cancel_requested), true);
    assert.equal(states.some(event => event.state === 'cancelled'), true);
    assert.equal(client.busy, false);
});

test('a job accepted after its inputs changed is stopped without polling or displaying results', async () => {
    const states = [];
    let finishSubmission;
    const submission = new Promise(resolve => { finishSubmission = resolve; });
    const requests = [];
    const client = createPineBacktestJobClient(async (url, options = {}) => {
        requests.push(url);
        if (url === '/api/pine/backtest') return submission;
        if (url.endsWith('/cancel')) return { state: 'cancelled', progress_pct: 0 };
        throw new Error(`Unexpected request: ${url}`);
    });

    const running = client.start({}, event => states.push(event));
    client.invalidate();
    finishSubmission({ job_id: 'job-3' });
    const result = await running;
    await new Promise(resolve => setTimeout(resolve, 0));

    assert.equal(result.state, 'detached');
    assert.deepEqual(requests, ['/api/pine/backtest', '/api/pine/backtest/jobs/job-3/cancel']);
    assert.equal(states.some(event => event.state === 'cancelled' && event.detached), true);
    assert.equal(client.busy, false);
});
