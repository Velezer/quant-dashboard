import test from 'node:test';
import assert from 'node:assert/strict';
import { cleanCopiedPineSource, pineSourceMetadata } from './pine-source-text.js';

test('Pine source cleaner unwraps markdown copies and removes copied formatting', () => {
    const result = cleanCopiedPineSource('**//@version=6**\n**indicator("Example")**\n**plot(close)**');
    assert.deepEqual(result, {
        source: '//@version=6\nindicator("Example")\nplot(close)',
        changed: true,
    });
});

test('Pine source cleaner leaves normal source and escapes markdown artifacts', () => {
    const input = 'indicator("A\\*B") // &#x20; [docs](https://example.test/doc)';
    const result = cleanCopiedPineSource(input);
    assert.equal(result.source, 'indicator("A*B") //   https://example.test/doc');
    assert.equal(result.changed, true);
    assert.deepEqual(cleanCopiedPineSource('plot(close)'), { source: 'plot(close)', changed: false });
});

test('Pine source metadata keeps declared attribution and explicit fallback text', () => {
    assert.deepEqual(pineSourceMetadata([
        'indicator("NWE", overlay=true)',
        '© Research Author // copyright',
        'CC BY-NC-SA 4.0',
        'https://www.tradingview.com/script/abc/',
        'https://creativecommons.org/licenses/by-nc-sa/4.0/',
    ].join('\n')), {
        title: 'NWE',
        author: 'Research Author',
        license: 'CC BY-NC-SA 4.0',
        licenseUrl: 'https://creativecommons.org/licenses/by-nc-sa/4.0/',
        reference: 'https://www.tradingview.com/script/abc/',
    });
    assert.deepEqual(pineSourceMetadata('plot(close)'), {
        title: undefined,
        author: undefined,
        license: undefined,
        licenseUrl: 'https://creativecommons.org/licenses/by-nc-sa/4.0/',
        reference: 'Pasted from user-provided TradingView source; original publication URL not supplied.',
    });
});
