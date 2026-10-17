export function encodeWireValue(value) {
    return JSON.stringify(value);
}

export function decodeWireValue(encoded) {
    return JSON.parse(String(encoded || ''));
}

export async function readApiResponse(response) {
    const body = await response.text();
    if (!body.trim()) return null;
    try {
        return JSON.parse(body);
    } catch (error) {
        throw new Error(`The server returned invalid JSON: ${error.message}`);
    }
}

export async function requestJson(url, options = {}) {
    const response = await fetch(url, {
        ...options,
        headers: { 'Content-Type': 'application/json', ...(options.headers || {}) },
    });
    let payload;
    try { payload = await response.json(); }
    catch { throw new Error(`Service returned invalid JSON (${response.status}).`); }
    if (!response.ok) throw new Error(payload?.error || `Request failed (${response.status}).`);
    return payload;
}

const INSTRUMENTS_API_PAGE_SIZE_ROWS = 500;

export async function requestAllInstruments({ signal, onPage } = {}) {
    const instruments = [];
    let offset = 0;
    let lastPage = null;
    do {
        const query = new URLSearchParams({
            limit: String(INSTRUMENTS_API_PAGE_SIZE_ROWS),
            offset: String(offset),
        });
        const page = await requestJson('/api/instruments?' + query.toString(), { signal });
        const rows = Array.isArray(page?.instruments) ? page.instruments : [];
        if (!rows.length && page?.has_more) {
            throw new Error('The instrument registry returned an empty page before the end.');
        }
        instruments.push(...rows);
        lastPage = page;
        onPage?.({
            loaded: instruments.length,
            total: Number(page.registered_count) || instruments.length,
        });
        const nextOffset = Number(page.next_offset);
        if (!page.has_more) break;
        if (!Number.isSafeInteger(nextOffset) || nextOffset <= offset) {
            throw new Error('The instrument registry returned an invalid page offset.');
        }
        offset = nextOffset;
    } while (true);
    return { ...lastPage, instruments };
}
