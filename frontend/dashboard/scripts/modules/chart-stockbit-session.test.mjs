import test from 'node:test';
import assert from 'node:assert/strict';
import { stockbitExpectedSessionDate } from './chart-stockbit-session.js';

test('Stockbit session date applies the intraday open boundary to shared timeframe classes', () => {
    const beforeOpenJakarta = new Date('2024-01-08T01:00:00Z');
    const afterOpenJakarta = new Date('2024-01-08T03:00:00Z');
    assert.equal(stockbitExpectedSessionDate('2h', beforeOpenJakarta), '2024-01-05');
    assert.equal(stockbitExpectedSessionDate('2h', afterOpenJakarta), '2024-01-08');
    assert.equal(stockbitExpectedSessionDate('5m', afterOpenJakarta), null);
});

test('Stockbit daily history uses the daily close boundary and skips weekends', () => {
    const beforeDailyCloseJakarta = new Date('2024-01-08T08:00:00Z');
    const afterDailyCloseJakarta = new Date('2024-01-08T09:00:00Z');
    const sunday = new Date('2024-01-07T10:00:00Z');
    assert.equal(stockbitExpectedSessionDate('1d', beforeDailyCloseJakarta), '2024-01-05');
    assert.equal(stockbitExpectedSessionDate('1d', afterDailyCloseJakarta), '2024-01-08');
    assert.equal(stockbitExpectedSessionDate('1h', sunday), '2024-01-05');
});
