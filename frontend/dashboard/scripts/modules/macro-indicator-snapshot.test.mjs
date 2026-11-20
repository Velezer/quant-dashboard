import test from 'node:test';
import assert from 'node:assert/strict';
import { parseFredSnapshotCsv } from './macro-indicator-snapshot.js';

const NOW = Date.parse('2026-11-20T12:00:00Z');

test('FRED snapshot parser preserves retrieval time and builds the indicator contract', () => {
    const parsed = parseFredSnapshotCsv(
        '# retrieved_at_seconds=1795164809\nobservation_date,DFII10\n2026-11-16,2.92\n2026-11-17,.\n2026-11-18,2.88\n',
        NOW,
    );
    assert.equal(parsed.retrieved_at_seconds, 1_795_164_809);
    assert.equal(parsed.observation_date, '2026-11-18');
    assert.equal(parsed.latest_value, 2.88);
    assert.ok(Math.abs(parsed.daily_change_basis_points + 4) < 1e-9);
    assert.equal(parsed.freshness, 'current');
    assert.equal(parsed.observations.length, 2);
});

test('FRED snapshot parser flags old retrievals and old observations as stale', () => {
    const parsed = parseFredSnapshotCsv(
        '# retrieved_at_seconds=1795164809\nobservation_date,DFII10\n2026-11-18,2.88\n',
        Date.parse('2026-11-30T12:00:00Z'),
    );
    assert.equal(parsed.freshness, 'stale');
    assert.equal(parsed.fetch_status, 'stale');
    assert.match(parsed.status_note, /more than seven days old/);
});

test('FRED snapshot parser rejects invalid metadata, rows and date order', () => {
    assert.throws(() => parseFredSnapshotCsv('observation_date,DFII10\n2026-11-18,2.88\n', NOW), /retrieval metadata/);
    assert.throws(() => parseFredSnapshotCsv('# retrieved_at_seconds=0\nobservation_date,DFII10\n2026-11-18,2.88\n', NOW), /retrieval metadata/);
    assert.throws(() => parseFredSnapshotCsv('# retrieved_at_seconds=1795164809\ndate,DFII10\n2026-11-18,2.88\n', NOW), /header/);
    assert.throws(() => parseFredSnapshotCsv('# retrieved_at_seconds=1795164809\nobservation_date,DFII10\n2026-11-19,2.88\n2026-11-18,2.89\n', NOW), /unordered/);
});
