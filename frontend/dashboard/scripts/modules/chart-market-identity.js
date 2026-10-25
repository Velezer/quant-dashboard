/** Resolves the app market venue for a signal using its registered identity metadata. */
export function marketVenueForSignal(signal) {
    if (!signal) return 'app';
    const exchange = String(signal.exchange || signal.Exchange || signal.venue || signal.Venue || '').toLowerCase();
    const declared = String(signal.class || signal.asset_class || signal.Class || '').toLowerCase();
    if (exchange.includes('stockbit') || exchange === 'idx') return 'idx';
    if (exchange.includes('tokocrypto')) return 'tokocrypto';
    if (exchange.includes('pluang')) return 'pluang';
    if (exchange === 'pintu') return 'pintu';
    if (declared.includes('stockbit') || declared.includes('idx') || String(signal.asset || '').toUpperCase().endsWith('.JK')) return 'idx';
    if (declared.includes('tokocrypto') || declared.includes('tokenized')) return 'tokocrypto';
    return 'app';
}

/** Maps a signal's display symbol to the symbol expected by its app market adapter. */
export function marketSymbolForSignal(signal) {
    const venue = marketVenueForSignal(signal);
    let symbol = String(signal?.asset || signal?.symbol || '').trim().toUpperCase()
        .replace(/_(FUT|SPOT)$/i, '');
    if (venue === 'idx' && !symbol.endsWith('.JK')) symbol += '.JK';
    if (venue === 'pluang') {
        const assetClass = String(signal?.class || signal?.asset_class || signal?.Class || '').toLowerCase();
        if (assetClass.includes('idx') || assetClass.includes('stock') || symbol.endsWith('.JK')) {
            return symbol.endsWith('.JK') ? symbol : `${symbol}.JK`;
        }
        return symbol.endsWith('-USD') ? symbol : `${symbol}-USD`;
    }
    if (venue === 'tokocrypto') {
        const storageSymbol = String(
            signal?.storage_symbol || signal?.data_symbol || signal?.Data_Symbol || '',
        ).trim().toUpperCase();
        if (storageSymbol.startsWith('TOKOCRYPTO:')) {
            const venueSymbol = storageSymbol.split(':').at(-1)?.replaceAll('_', '');
            const quote = ['USDT', 'IDR'].find(currency => venueSymbol?.endsWith(currency));
            if (quote && venueSymbol.length > quote.length) {
                return `${venueSymbol.slice(0, -quote.length)}-${quote}`;
            }
        }
        const assetClass = String(signal?.class || signal?.Class || '').toLowerCase();
        return assetClass === 'tokenized_stock'
            ? symbol.replace(/-USD$/i, '')
            : (symbol.endsWith('-USD') ? symbol : `${symbol}-USD`);
    }
    if (venue !== 'idx' && !symbol.endsWith('-USD')) symbol += '-USD';
    return symbol;
}
