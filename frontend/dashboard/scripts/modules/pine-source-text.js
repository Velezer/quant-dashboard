export function cleanCopiedPineSource(value) {
    const original = String(value || '');
    const lines = original.split(/\r?\n/);
    const nonemptyLines = lines.filter(line => line.trim());
    const wrappedLines = nonemptyLines.filter(line => /^\s*\*\*.*\*\*\s*$/.test(line) || line.trim() === '**');
    const looksMarkdownWrapped = nonemptyLines.length > 0 && wrappedLines.length / nonemptyLines.length >= 0.6;
    let source = original;
    if (looksMarkdownWrapped) {
        source = lines.map(line => line.trim() === '**' ? '' : line.replace(/^\s*\*\*/, '').replace(/\*\*\s*$/, '')).join('\n');
    }
    source = source
        .replace(/&#x20;/gi, ' ')
        .replace(/&#xA0;/gi, ' ')
        .replace(/\\([*<>])/g, '$1')
        .replace(/\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g, '$2');
    return { source, changed: source !== original };
}

export function pineSourceMetadata(source) {
    return {
        title: source.match(/\bindicator\s*\(\s*["']([^"']+)["']/i)?.[1]?.trim(),
        author: source.match(/©\s*([^\r\n*]+)/)?.[1]?.trim().replace(/\s*\/\/.*/, ''),
        license: source.match(/CC BY-NC-SA 4\.0/i)?.[0],
        licenseUrl: source.match(/https?:\/\/creativecommons\.org\/licenses\/by-nc-sa\/4\.0\/?/i)?.[0]
            || 'https://creativecommons.org/licenses/by-nc-sa/4.0/',
        reference: source.match(/https?:\/\/(?:www\.)?tradingview\.com\/[^\s)]+/i)?.[0]
            || 'Pasted from user-provided TradingView source; original publication URL not supplied.',
    };
}
