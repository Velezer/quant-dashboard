export const FRED_SNAPSHOT_URL = 'https://raw.githubusercontent.com/Velezer/quant-dashboard/main/macrodata/DFII10.csv';

const METADATA_PREFIX = '# retrieved_at_seconds=';
const SNAPSHOT_MAX_AGE_SECONDS = 7 * 24 * 60 * 60;
const MAX_OBSERVATIONS = 3_000;

export function parseFredSnapshotCsv(text, nowMilliseconds = Date.now()) {
    const lines = String(text).replace(/^\uFEFF/, '').split(/\r?\n/);
    const retrievedAtLine = lines.shift() || '';
    const retrievedAt = Number(retrievedAtLine.startsWith(METADATA_PREFIX)
        ? retrievedAtLine.slice(METADATA_PREFIX.length)
        : NaN);
    if (!Number.isSafeInteger(retrievedAt) || retrievedAt <= 0) {
        throw new Error('The FRED snapshot has invalid retrieval metadata.');
    }
    if ((lines.shift() || '').trim().replace(/^\uFEFF/, '') !== 'observation_date,DFII10') {
        throw new Error('FRED returned an unexpected DFII10 CSV header.');
    }

    const observations = [];
    let previousDate = '';
    for (const line of lines) {
        if (!line.trim()) continue;
        const fields = line.trim().split(',');
        if (fields.length !== 2) throw new Error('FRED returned a malformed DFII10 observation row.');
        const [date, rawValue] = fields.map(value => value.trim());
        const dateMilliseconds = Date.parse(`${date}T00:00:00Z`);
        if (!/^\d{4}-\d{2}-\d{2}$/.test(date)
            || !Number.isFinite(dateMilliseconds)
            || new Date(dateMilliseconds).toISOString().slice(0, 10) !== date) {
            throw new Error('FRED returned an invalid DFII10 observation date.');
        }
        if (previousDate && date <= previousDate) {
            throw new Error('FRED returned unordered or duplicate DFII10 dates.');
        }
        previousDate = date;
        if (!rawValue || rawValue === '.') continue;
        const value = Number(rawValue);
        if (!Number.isFinite(value)) throw new Error('FRED returned an invalid DFII10 yield value.');
        observations.push({ date, value });
    }

    const boundedObservations = observations.slice(-MAX_OBSERVATIONS);
    const latest = boundedObservations.at(-1);
    if (!latest) throw new Error('FRED returned no usable DFII10 observations.');

    const nowSeconds = Math.floor(nowMilliseconds / 1_000);
    const today = new Date(nowMilliseconds);
    const todayUtc = Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate());
    const staleCutoff = new Date(todayUtc - SNAPSHOT_MAX_AGE_SECONDS * 1_000)
        .toISOString().slice(0, 10);
    const stale = latest.date < staleCutoff || nowSeconds - retrievedAt > SNAPSHOT_MAX_AGE_SECONDS;
    const previous = boundedObservations.at(-2);

    return {
        id: 'ten_year_tips_real_yield',
        title: '10-Year TIPS real yield',
        description: 'The market yield on 10-year Treasury inflation-indexed securities. This is a real yield, not the nominal 10-year Treasury yield or a direct inflation forecast.',
        unit: '% per year',
        frequency: 'Daily, business days',
        source: 'Federal Reserve Bank of St. Louis · FRED',
        source_series_id: 'DFII10',
        source_url: 'https://fred.stlouisfed.org/series/DFII10',
        freshness: stale ? 'stale' : 'current',
        fetch_status: stale ? 'stale' : 'fresh',
        observation_date: latest.date,
        latest_value: latest.value,
        daily_change_basis_points: previous ? (latest.value - previous.value) * 100 : null,
        retrieved_at_seconds: retrievedAt,
        status_note: stale
            ? 'The latest FRED snapshot or observation is more than seven days old. Check FRED again later.'
            : null,
        refresh_error: null,
        observations: boundedObservations,
    };
}
