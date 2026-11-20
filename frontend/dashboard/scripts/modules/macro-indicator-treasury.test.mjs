import test from 'node:test';
import assert from 'node:assert/strict';
import { loadTreasurySnapshot, parseTreasuryYearXml, TREASURY_XML_URL } from './macro-indicator-treasury.js';

test('Treasury XML parser skips missing values and keeps negative real yields', () => {
    const parsed = parseTreasuryYearXml(
        '<feed><entry><d:NEW_DATE>2026-11-16T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.92</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-17T00:00:00</d:NEW_DATE><d:TC_10YEAR>.</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>-0.08</d:TC_10YEAR></entry></feed>',
        2026,
    );
    assert.deepEqual(parsed, [
        { date: '2026-11-16', value: 2.92 },
        { date: '2026-11-18', value: -0.08 },
    ]);
});

test('Treasury XML parser rejects malformed, unordered and out-of-year observations', () => {
    assert.throws(() => parseTreasuryYearXml('<html>no feed</html>', 2026), /unexpected XML feed/);
    assert.throws(() => parseTreasuryYearXml('<feed><entry><d:NEW_DATE>2026-13-18</d:NEW_DATE></entry></feed>', 2026), /invalid/);
    assert.throws(() => parseTreasuryYearXml('<feed><entry><d:NEW_DATE>2025-11-18</d:NEW_DATE></entry></feed>', 2026), /invalid/);
    assert.throws(() => parseTreasuryYearXml('<feed><entry><d:NEW_DATE>2026-11-18</d:NEW_DATE><d:TC_10YEAR>NaN</d:TC_10YEAR></entry></feed>', 2026), /invalid 10-year/);
    assert.throws(() => parseTreasuryYearXml('<feed><entry><d:NEW_DATE>2026-11-18</d:NEW_DATE><d:TC_10YEAR>2.8</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-18</d:NEW_DATE><d:TC_10YEAR>2.9</d:TC_10YEAR></entry></feed>', 2026), /unordered/);
});

test('Treasury loader queries an eleven-year history and reports the latest change', async () => {
    const now = Date.parse('2026-11-20T12:00:00Z');
    const requestedYears = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async url => {
        assert.ok(String(url).startsWith(TREASURY_XML_URL));
        const year = Number(new URL(url).searchParams.get('field_tdr_date_value'));
        requestedYears.push(year);
        const rows = year === 2026
            ? '<entry><d:NEW_DATE>2026-11-18T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.89</d:TC_10YEAR></entry><entry><d:NEW_DATE>2026-11-19T00:00:00</d:NEW_DATE><d:TC_10YEAR>2.88</d:TC_10YEAR></entry>'
            : `<entry><d:NEW_DATE>${year}-01-04T00:00:00</d:NEW_DATE><d:TC_10YEAR>1.25</d:TC_10YEAR></entry>`;
        return new Response(`<feed>${rows}</feed>`, { status: 200 });
    };
    try {
        const data = await loadTreasurySnapshot(now);
        assert.equal(data.source, 'U.S. Department of the Treasury');
        assert.equal(data.source_series_id, 'TC_10YEAR');
        assert.equal(data.observation_date, '2026-11-19');
        assert.equal(data.latest_value, 2.88);
        assert.ok(Math.abs(data.daily_change_basis_points + 1) < 1e-9);
        assert.equal(data.freshness, 'current');
        assert.equal(data.observations.length, 12);
        assert.deepEqual(requestedYears.sort((a, b) => a - b), Array.from({ length: 11 }, (_, index) => 2016 + index));
    } finally {
        globalThis.fetch = originalFetch;
    }
});
