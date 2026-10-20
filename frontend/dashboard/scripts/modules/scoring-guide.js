/** Adds an illustrative points example and the deployment's existing model catalog. */
export function exampleTotals(points) {
    if (!Array.isArray(points) || points.length !== 4
        || points.some(value => !Number.isFinite(value) || value < 0 || value > 10)) return null;
    const [nwe, dss, volume, delta] = points;
    return { price: nwe + dss, volume: volume + delta, total: nwe + dss + volume + delta };
}

function numericValue(value) {
    if (typeof value !== 'number' && (typeof value !== 'string' || value.trim() === '')) return null;
    const number = Number(value);
    return Number.isFinite(number) ? number : null;
}

export function guideModels(catalog, exchange = '', timeframe = 'all') {
    return (Array.isArray(catalog?.signal_strategies) ? catalog.signal_strategies : [])
        .filter(model => model && typeof model === 'object'
            && typeof model.score_model_id === 'string' && model.score_model_id.trim()
            && numericValue(model.score_scale_max) > 0
            && (!exchange || (Array.isArray(model.exchange_ids) && model.exchange_ids.includes(exchange)))
            && (timeframe === 'all' || model.timeframe === timeframe));
}

export async function fetchGuideCatalog(fetchImpl = globalThis.fetch) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 10_000);
    try {
        const response = await fetchImpl('/api/master-data', {
            headers: { Accept: 'application/json' }, cache: 'no-store', signal: controller.signal,
        });
        if (!response.ok) throw new Error(`Model catalog request failed (${response.status}).`);
        const catalog = await response.json();
        if (!catalog || !Array.isArray(catalog.signal_strategies)) {
            throw new Error('The service returned an invalid model catalog.');
        }
        return catalog;
    } finally {
        clearTimeout(timeout);
    }
}

function setupExample(doc) {
    const form = doc.getElementById('score-example');
    const factors = ['nwe', 'dss', 'volume', 'delta'];
    const inputs = factors.map(factor => doc.getElementById(`example-${factor}`));
    function update() {
        const points = inputs.map(input => Number(input.value));
        const totals = exampleTotals(points);
        if (!totals) return;
        inputs.forEach((input, index) => {
            doc.getElementById(`example-${factors[index]}-value`).value = points[index].toFixed(1);
            input.setAttribute('aria-valuetext', `${points[index]} of ${input.max} points`);
            doc.getElementById(`example-bar-${factors[index]}`).style.width = `${points[index] / 40 * 100}%`;
        });
        doc.getElementById('example-total').value = totals.total.toFixed(1);
        doc.getElementById('example-price').value = totals.price.toFixed(1);
        doc.getElementById('example-flow').value = totals.volume.toFixed(1);
        doc.getElementById('example-mode').textContent = inputs.every(input => input.value === input.defaultValue)
            ? 'Worked example values'
            : 'Hypothetical points · candle example unchanged';
    }
    inputs.forEach(input => input.addEventListener('input', update));
    form.addEventListener('reset', () => setTimeout(update, 0));
    form.addEventListener('submit', event => event.preventDefault());
    doc.getElementById('example-inputs').disabled = false;
    update();
}

function element(doc, tag, className, text) {
    const node = doc.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = String(text);
    return node;
}

function timeframeMinutes(timeframe) {
    const match = /^(\d+)(m|h|d|w)$/.exec(timeframe);
    return match ? Number(match[1]) * ({ m: 1, h: 60, d: 1440, w: 10080 })[match[2]] : Infinity;
}

function replaceOptions(doc, select, options, preferred) {
    const nodes = options.map(([value, label]) => {
        const option = element(doc, 'option', '', label);
        option.value = value;
        return option;
    });
    select.replaceChildren(...nodes);
    select.value = options.some(([value]) => value === preferred) ? preferred : options[0]?.[0] || '';
}

function renderModel(doc, model) {
    const card = element(doc, 'article', 'catalog-model');
    card.append(element(doc, 'h3', '', model.label || model.short_label || model.id || 'Score model'));
    const metrics = element(doc, 'div', 'model-metrics');
    const scale = numericValue(model.score_scale_max);
    const minimum = numericValue(model[`minimum_score_out_of_${scale}`] ?? model.minimum_score);
    metrics.append(element(doc, 'span', '', `Scale /${scale}`));
    if (minimum !== null) metrics.append(element(doc, 'span', '', `Minimum ${minimum}/${scale}`));
    const history = numericValue(model.minimum_history_bars);
    if (history !== null && history > 0) metrics.append(element(doc, 'span', '', `History ${history} bars`));
    card.append(metrics, element(doc, 'p', 'model-id', model.score_model_id));
    const context = [model.timeframe, ...(Array.isArray(model.product_ids) ? model.product_ids : []),
        ...(Array.isArray(model.asset_category_ids) ? model.asset_category_ids : [])].filter(Boolean);
    card.append(element(doc, 'p', 'model-id', context.join(' · ')));
    const details = element(doc, 'details');
    details.append(element(doc, 'summary', '', 'Read the registered rules'),
        element(doc, 'p', '', model.score_basis_description || model.description || 'No additional description is supplied by this catalog.'));
    card.append(details);
    return card;
}

export function initializeScoringGuide(doc, fetchImpl = globalThis.fetch) {
    if (!doc.getElementById('score-example')) return;
    setupExample(doc);
    const app = doc.getElementById('catalog-app');
    const timeframe = doc.getElementById('catalog-timeframe');
    const status = doc.getElementById('catalog-status');
    const results = doc.getElementById('catalog-models');
    const retry = doc.getElementById('catalog-retry');
    let catalog = null;

    function render() {
        const models = guideModels(catalog, app.value, timeframe.value);
        results.replaceChildren(...models.map(model => renderModel(doc, model)));
        if (!models.length) results.append(element(doc, 'p', 'catalog-empty',
            'No registered score models match this app and timeframe. Choose another filter or reload the catalog.'));
    }

    function updateTimeframes(preferred = timeframe.value) {
        const timeframes = [...new Set(guideModels(catalog, app.value)
            .map(model => model.timeframe).filter(value => typeof value === 'string' && value))]
            .sort((left, right) => timeframeMinutes(left) - timeframeMinutes(right));
        replaceOptions(doc, timeframe, [['all', 'All timeframes'], ...timeframes.map(value => [value, value])], preferred);
        render();
    }

    async function load() {
        retry.disabled = true;
        retry.textContent = 'Loading catalog…';
        status.classList.remove('error');
        status.textContent = 'Loading the model catalog. The guide and example remain available.';
        results.setAttribute('aria-busy', 'true');
        try {
            const loaded = await fetchGuideCatalog(fetchImpl);
            catalog = loaded;
            const models = guideModels(catalog);
            const exchanges = [...new Set(models.flatMap(model => Array.isArray(model.exchange_ids) ? model.exchange_ids : []))]
                .filter(value => typeof value === 'string' && value).sort();
            const options = exchanges.map(id => {
                const exchange = (Array.isArray(catalog.exchanges) ? catalog.exchanges : [])
                    .find(item => item?.id === id);
                return [id, exchange?.label || exchange?.name || id];
            });
            replaceOptions(doc, app, options.length ? options : [['', 'No registered apps']], app.value);
            app.disabled = !exchanges.length;
            timeframe.disabled = !exchanges.length;
            updateTimeframes();
            status.textContent = models.length
                ? 'Catalog loaded. Model versions and requirements below come from this deployment; they are not live price data.'
                : 'Catalog loaded, but no score models are registered. The scoring explanations above remain available.';
            retry.textContent = 'Reload catalog';
        } catch (error) {
            status.classList.add('error');
            const message = error?.name === 'AbortError' ? 'The request timed out.' : 'The service could not supply a valid catalog.';
            status.textContent = catalog
                ? `Could not reload the catalog. Showing the last loaded reference. ${message} Retry to update it.`
                : `Model catalog unavailable. ${message} The guide and example remain available. Retry to load this deployment’s models.`;
            retry.textContent = 'Retry catalog';
        } finally {
            retry.disabled = false;
            results.setAttribute('aria-busy', 'false');
        }
    }
    app.addEventListener('change', () => updateTimeframes());
    timeframe.addEventListener('change', render);
    retry.addEventListener('click', () => { void load(); });
    void load();
}

if (typeof document !== 'undefined') initializeScoringGuide(document);
