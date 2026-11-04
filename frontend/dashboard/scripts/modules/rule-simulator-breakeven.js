const BREAKEVEN_PRICE_FORMATTER = new Intl.NumberFormat('en-US', { maximumSignificantDigits: 10 });
const BREAKEVEN_MOVE_FORMATTER = new Intl.NumberFormat('en-US', { maximumSignificantDigits: 6 });

/** Calculate the favorable exit price that offsets entry and exit transaction fees. */
export function feeAdjustedBreakeven(entryPrice, buyFeePct, sellFeePct, direction = 'LONG') {
    const entry = Number(entryPrice);
    const buyFee = Number(buyFeePct);
    const sellFee = Number(sellFeePct);
    const side = String(direction || '').trim().toUpperCase();
    if (!Number.isFinite(entry) || entry <= 0
        || isMissingFeeInput(buyFeePct) || !Number.isFinite(buyFee) || buyFee < 0 || buyFee >= 100
        || isMissingFeeInput(sellFeePct) || !Number.isFinite(sellFee) || sellFee < 0 || sellFee >= 100
        || !['LONG', 'SHORT'].includes(side)) return null;

    const exitPrice = side === 'LONG'
        ? entry * (100 + buyFee) / (100 - sellFee)
        : entry * (100 - sellFee) / (100 + buyFee);
    const requiredMovePct = Math.abs((exitPrice - entry) / entry) * 100;
    if (!Number.isFinite(exitPrice) || exitPrice <= 0 || !Number.isFinite(requiredMovePct)) return null;
    return { exitPrice, requiredMovePct };
}

function isMissingFeeInput(value) {
    return value === null || value === undefined || String(value).trim() === '';
}

export function formatFeeBreakevenPrice(value) {
    return Number.isFinite(value) ? BREAKEVEN_PRICE_FORMATTER.format(value) : 'Unavailable';
}

export function formatFeeBreakevenMove(value) {
    return Number.isFinite(value) ? `${BREAKEVEN_MOVE_FORMATTER.format(value)}%` : 'Unavailable';
}
