import test from 'node:test';
import assert from 'node:assert/strict';
import { missingPineSecurityTimeframes, runPineWithStoredTimeframes } from './pine-security-history.js';

const missingFourHour = {
    valid: false,
    diagnostics: [{
        message: 'No approved stored 4h candle series is available for this selected app instrument.',
    }],
};

test('missing stored request.security history maps only to an available chart interval', () => {
    const result = {
        diagnostics: [
            ...missingFourHour.diagnostics,
            { message: 'No approved stored 4H candle series is available for this selected app instrument.' },
            { message: 'No approved stored 3m candle series is available for this selected app instrument.' },
        ],
    };

    assert.deepEqual(missingPineSecurityTimeframes(result, ['15m', '4h', '1d']), ['4h']);
});

test('Pine retries once after loading the missing stored timeframe', async () => {
    let runs = 0;
    const loaded = [];
    const result = await runPineWithStoredTimeframes(
        async () => {
            runs += 1;
            return loaded.includes('4h') ? { valid: true, requested_timeframes: ['4h'] } : missingFourHour;
        },
        async timeframe => {
            loaded.push(timeframe);
            return true;
        },
        ['15m', '4h'],
    );

    assert.equal(runs, 2);
    assert.deepEqual(loaded, ['4h']);
    assert.deepEqual(result.requested_timeframes, ['4h']);
});

test('Pine stops retrying when the requested history is empty or already attempted', async () => {
    let runs = 0;
    const loaded = [];
    const result = await runPineWithStoredTimeframes(
        async () => { runs += 1; return missingFourHour; },
        async timeframe => { loaded.push(timeframe); return true; },
        ['4h'],
    );

    assert.equal(runs, 2);
    assert.deepEqual(loaded, ['4h']);
    assert.equal(result.valid, false);
});
