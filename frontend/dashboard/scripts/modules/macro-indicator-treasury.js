export const TREASURY_XML_URL = 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/pages/xml?data=daily_treasury_real_yield_curve&field_tdr_date_value=';
const TREASURY_TEXT_URL = 'https://home.treasury.gov/resource-center/data-chart-center/interest-rates/TextView?type=daily_treasury_real_yield_curve&field_tdr_date_value=';
const MAX_OBSERVATIONS = 3_000;
const MAX_AGE_SECONDS = 7 * 24 * 60 * 60;

function xmlProperty(entry, name) {
    const escapedName = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const match = entry.match(new RegExp(`<d:${escapedName}\\b[^>]*>([^<]*)<\\/d:${escapedName}>`));
    return match?.[1];
}

function validDate(date) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return false;
    const milliseconds = Date.parse(`${date}T00:00:00Z`);
    return Number.isFinite(milliseconds) && new Date(milliseconds).toISOString().slice(0, 10) === date;
}

export function parseTreasuryYearXml(xml, expectedYear) {
    const text = String(xml);
    if (!/<feed\b/.test(text)) throw new Error(`Treasury returned an unexpected XML feed for ${expectedYear}.`);
    const observations = [];
    for (const match of text.matchAll(/<entry\b[^>]*>([\s\S]*?)<\/entry>/g)) {
        const entry = match[1];
        const rawDate = xmlProperty(entry, 'NEW_DATE')?.trim();
        const date = rawDate?.slice(0, 10);
        if (!date || !validDate(date) || Number(date.slice(0, 4)) !== expectedYear) {
            throw new Error(`Treasury returned an invalid real-yield observation date for ${expectedYear}.`);
        }
        const rawValue = xmlProperty(entry, 'TC_10YEAR')?.trim();
        if (rawValue && rawValue !== '.') {
            const value = Number(rawValue);
            if (!Number.isFinite(value)) throw new Error('Treasury returned an invalid 10-year real yield.');
            observations.push({ date, value });
        }
    }
    if (observations.some((row, index) => index > 0 && observations[index - 1].date >= row.date)) {
        throw new Error(`Treasury returned unordered or duplicate observations for ${expectedYear}.`);
    }
    return observations;
}

async function fetchYear(year) {
    const response = await fetch(`${TREASURY_XML_URL}${year}`, {
        cache: 'no-store',
        headers: { Accept: 'application/atom+xml, application/xml, text/xml' },
    });
    if (!response.ok) throw new Error(`Treasury returned HTTP ${response.status} for ${year}.`);
    return parseTreasuryYearXml(await response.text(), year);
}

export async function loadTreasurySnapshot(nowMilliseconds = Date.now()) {
    const currentYear = new Date(nowMilliseconds).getUTCFullYear();
    const years = Array.from({ length: 11 }, (_, index) => currentYear - 10 + index);
    const yearlyRows = await Promise.all(years.map(fetchYear));
    const observations = yearlyRows.flat().sort((left, right) => left.date.localeCompare(right.date)).slice(-MAX_OBSERVATIONS);
    if (!observations.length) throw new Error('Treasury returned no 10-year real-yield observations.');
    if (observations.some((row, index) => index > 0 && observations[index - 1].date >= row.date)) {
        throw new Error('Treasury returned unordered or duplicate real-yield observations.');
    }

    const latest = observations.at(-1);
    const previous = observations.at(-2);
    const nowSeconds = Math.floor(nowMilliseconds / 1_000);
    const cutoff = new Date(nowMilliseconds - MAX_AGE_SECONDS * 1_000).toISOString().slice(0, 10);
    const stale = latest.date < cutoff;
    return {
        id: 'ten_year_tips_real_yield',
        title: '10-Year TIPS real yield',
        description: 'The market yield on 10-year Treasury inflation-indexed securities. This is a real yield, not the nominal 10-year Treasury yield or a direct inflation forecast.',
        unit: '% per year',
        frequency: 'Daily, business days',
        source: 'U.S. Department of the Treasury',
        source_series_id: 'TC_10YEAR',
        source_url: `${TREASURY_TEXT_URL}${currentYear}`,
        freshness: stale ? 'stale' : 'current',
        fetch_status: 'fresh',
        observation_date: latest.date,
        latest_value: latest.value,
        observation_change: previous ? latest.value - previous.value : null,
        change_period: 'from the prior business day',
        daily_change_basis_points: previous ? (latest.value - previous.value) * 100 : null,
        limitations: 'Daily market pricing for an inflation-linked Treasury security; it is not a direct inflation forecast.',
        retrieved_at_seconds: nowSeconds,
        status_note: stale ? 'The latest Treasury observation is more than seven days old. Check the Treasury feed again later.' : null,
        refresh_error: null,
        observations,
    };
}
