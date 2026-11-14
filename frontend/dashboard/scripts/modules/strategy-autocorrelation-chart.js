const SVG_NS = 'http://www.w3.org/2000/svg';
const CHART_WIDTH = 720;
const CHART_HEIGHT = 300;
const PLOT = { left: 58, right: 18, top: 20, bottom: 42 };
const SERIES_STYLES = [
    { key: 'baseline', label: 'Equal-weight baseline', color: '#78dfdf' },
    { key: 'training_candidate', label: 'Training-fitted candidate', color: '#b6a3fa' },
];

function htmlElement(doc, tag, text, className) {
    const node = doc.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
}

function svgElement(doc, tag, attributes = {}, text) {
    const node = doc.createElementNS(SVG_NS, tag);
    for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, String(value));
    if (text !== undefined) node.textContent = text;
    return node;
}

function coefficientText(value) {
    return typeof value === 'number' && Number.isFinite(value)
        ? value.toLocaleString('en-US', { minimumFractionDigits: 3, maximumFractionDigits: 3, signDisplay: 'always' })
        : 'Unavailable';
}

function seriesLabel(series, label) {
    if (!series) return label + ': no training-fitted candidate was selected.';
    const weights = Array.isArray(series.weights_out_of_40) ? series.weights_out_of_40.join('/') : 'unavailable';
    return label + ': ' + (series.qualifying_signal_events || 0) + ' qualifying events · factor weights ' + weights + '/40';
}

function makeChart(doc, data) {
    const svg = svgElement(doc, 'svg', {
        class: 'strategy-autocorrelation-chart',
        viewBox: '0 0 ' + CHART_WIDTH + ' ' + CHART_HEIGHT,
        role: 'img',
        'aria-labelledby': 'strategy-acf-chart-title strategy-acf-chart-description',
    });
    const allLags = SERIES_STYLES.flatMap(style => data[style.key]?.lags || []);
    const maxLag = Math.max(1, ...allLags.map(item => Number(item.lag_bars) || 0));
    const plotWidth = CHART_WIDTH - PLOT.left - PLOT.right;
    const plotHeight = CHART_HEIGHT - PLOT.top - PLOT.bottom;
    const x = lag => PLOT.left + ((lag - 1) / Math.max(1, maxLag - 1)) * plotWidth;
    const y = value => PLOT.top + ((1 - value) / 2) * plotHeight;

    svg.append(
        svgElement(doc, 'title', { id: 'strategy-acf-chart-title' }, 'Strategy return autocorrelation by lag'),
        svgElement(doc, 'desc', { id: 'strategy-acf-chart-description' }, 'Correlation ranges from minus one to plus one. Negative readings show reversal-like dependence between model-aligned outcomes at signal events separated by the labeled number of bars.'),
    );

    for (const value of [-1, -0.5, 0, 0.5, 1]) {
        const line = svgElement(doc, 'line', {
            x1: PLOT.left, x2: CHART_WIDTH - PLOT.right, y1: y(value), y2: y(value),
            class: value === 0 ? 'strategy-acf-zero' : 'strategy-acf-grid',
        });
        svg.append(line, svgElement(doc, 'text', { x: PLOT.left - 10, y: y(value) + 4, 'text-anchor': 'end', class: 'strategy-acf-axis-label' }, value > 0 ? '+' + value : String(value)));
    }
    svg.append(svgElement(doc, 'text', {
        x: 16, y: PLOT.top + plotHeight / 2,
        transform: 'rotate(-90 16 ' + (PLOT.top + plotHeight / 2) + ')',
        class: 'strategy-acf-axis-title', 'text-anchor': 'middle',
    }, 'Correlation r'));

    for (let lag = 1; lag <= maxLag; lag++) {
        const xPos = x(lag);
        svg.append(
            svgElement(doc, 'line', { x1: xPos, x2: xPos, y1: y(0) - 3, y2: y(0) + 3, class: 'strategy-acf-tick' }),
            svgElement(doc, 'text', { x: xPos, y: CHART_HEIGHT - 17, 'text-anchor': 'middle', class: 'strategy-acf-axis-label' }, String(lag)),
        );
    }
    svg.append(svgElement(doc, 'text', {
        x: PLOT.left + plotWidth / 2, y: CHART_HEIGHT - 1,
        'text-anchor': 'middle', class: 'strategy-acf-axis-title',
    }, 'Lag · timeframe bars'));

    const legend = doc.createElementNS(SVG_NS, 'g');
    legend.setAttribute('class', 'strategy-acf-legend');
    let legendX = PLOT.left;
    for (const style of SERIES_STYLES) {
        if (!data[style.key]) continue;
        legend.append(
            svgElement(doc, 'line', { x1: legendX, x2: legendX + 22, y1: 11, y2: 11, stroke: style.color, 'stroke-width': 3 }),
            svgElement(doc, 'text', { x: legendX + 28, y: 15, class: 'strategy-acf-legend-label' }, style.label),
        );
        legendX += style.key === 'baseline' ? 220 : 260;
    }
    svg.append(legend);

    for (const style of SERIES_STYLES) {
        const lags = data[style.key]?.lags;
        if (!Array.isArray(lags)) continue;
        let segment = [];
        const flushSegment = () => {
            if (segment.length > 1) {
                const path = segment.map((point, index) => (index ? 'L' : 'M') + point.x + ',' + point.y).join(' ');
                svg.append(svgElement(doc, 'path', { d: path, class: 'strategy-acf-line', stroke: style.color }));
            }
            segment = [];
        };
        for (const item of lags) {
            const lag = Number(item.lag_bars);
            const value = typeof item.coefficient === 'number' && Number.isFinite(item.coefficient)
                ? Math.max(-1, Math.min(1, item.coefficient)) : null;
            if (lag < 1 || value === null) {
                flushSegment();
                continue;
            }
            const point = { x: x(lag), y: y(value), value, lag, item };
            segment.push(point);
            const circle = svgElement(doc, 'circle', {
                cx: point.x, cy: point.y, r: 4.5,
                class: value < 0 ? 'strategy-acf-point strategy-acf-point-negative' : 'strategy-acf-point strategy-acf-point-positive',
                stroke: style.color,
            });
            circle.append(svgElement(doc, 'title', {}, style.label + ' · lag ' + lag + ' bars · r=' + coefficientText(value) + ' · ' + (item.pairs || 0) + ' pairs'));
            svg.append(circle);
        }
        flushSegment();
    }
    return svg;
}

function lagTable(doc, data) {
    const details = htmlElement(doc, 'details', undefined, 'autocorrelation-lag-details');
    details.append(htmlElement(doc, 'summary', 'Show lag coefficients and pair counts'));
    const scroll = htmlElement(doc, 'div', undefined, 'table-scroll');
    scroll.tabIndex = 0;
    scroll.setAttribute('role', 'region');
    scroll.setAttribute('aria-label', 'Autocorrelation coefficient and sample by lag');
    const table = doc.createElement('table');
    const caption = htmlElement(doc, 'caption', 'Training-only strategy-return autocorrelation');
    caption.className = 'sr-only';
    const head = doc.createElement('thead');
    const headerRow = doc.createElement('tr');
    for (const label of ['Series', 'Lag · bars', 'Coefficient r', 'Pairs']) {
        const th = htmlElement(doc, 'th', label);
        th.scope = 'col';
        headerRow.append(th);
    }
    head.append(headerRow);
    const body = doc.createElement('tbody');
    for (const style of SERIES_STYLES) {
        const lags = data[style.key]?.lags;
        if (!Array.isArray(lags)) continue;
        for (const item of lags) {
            const row = doc.createElement('tr');
            row.append(
                htmlElement(doc, 'td', style.label),
                htmlElement(doc, 'td', String(item.lag_bars ?? '—')),
                htmlElement(doc, 'td', coefficientText(item.coefficient), 'metric-value'),
                htmlElement(doc, 'td', String(item.pairs ?? 0), 'metric-value'),
            );
            body.append(row);
        }
    }
    table.append(caption, head, body);
    scroll.append(table);
    details.append(scroll);
    return details;
}

function strongestNegativeLag(data, seriesKey) {
    const lags = data[seriesKey]?.lags;
    if (!Array.isArray(lags)) return null;
    return lags.filter(item => typeof item.coefficient === 'number' && item.coefficient < 0)
        .sort((left, right) => left.coefficient - right.coefficient)[0] || null;
}

export function renderStrategyAutocorrelation(root, diagnostic) {
    if (!root) return;
    const doc = root.ownerDocument;
    root.replaceChildren();
    if (!diagnostic || !diagnostic.baseline) {
        root.append(htmlElement(doc, 'p', 'This saved report predates the strategy autocorrelation indicator. Rerun the ticker to calculate it.', 'small-text'));
        return;
    }

    const meta = htmlElement(doc, 'div', undefined, 'strategy-acf-series-summary');
    meta.append(
        htmlElement(doc, 'p', seriesLabel(diagnostic.baseline, 'Equal-weight baseline')),
        htmlElement(doc, 'p', seriesLabel(diagnostic.training_candidate, 'Training-fitted candidate')),
    );
    const baselineNegative = strongestNegativeLag(diagnostic, 'baseline');
    const candidateNegative = strongestNegativeLag(diagnostic, 'training_candidate');
    const interpretations = [];
    if (baselineNegative) interpretations.push('Baseline’s most negative available lag is ' + baselineNegative.lag_bars + ' bars (r=' + coefficientText(baselineNegative.coefficient) + ', ' + baselineNegative.pairs + ' pairs)');
    if (candidateNegative) interpretations.push('candidate’s is ' + candidateNegative.lag_bars + ' bars (r=' + coefficientText(candidateNegative.coefficient) + ', ' + candidateNegative.pairs + ' pairs)');
    const headline = interpretations.length
        ? interpretations.join('; ') + '.'
        : 'No negative autocorrelation coefficient met the minimum sample requirement.';
    meta.append(htmlElement(doc, 'p', headline, 'strategy-acf-highlight'));

    const chartData = {
        baseline: diagnostic.baseline,
        training_candidate: diagnostic.training_candidate,
    };
    const chart = makeChart(doc, chartData);
    const chartScroll = htmlElement(doc, 'div', undefined, 'strategy-acf-scroll');
    chartScroll.tabIndex = 0;
    chartScroll.setAttribute('role', 'region');
    chartScroll.setAttribute('aria-label', 'Strategy autocorrelation chart; scroll horizontally to inspect all lags');
    chartScroll.append(chart);
    const basis = htmlElement(doc, 'p', (diagnostic.return_horizon_bars || 1) + '-bar signed signal outcomes · ' + (diagnostic.return_basis || 'return basis unavailable') + ' · training split only.', 'small-text strategy-acf-basis');
    const explanation = htmlElement(doc, 'p', diagnostic.interpretation || 'Negative coefficients are reversal-like dependence clues, not flip instructions.', 'small-text strategy-acf-explanation');
    root.append(meta, chartScroll, basis, explanation, lagTable(doc, chartData));
}
