import { publishDashboardBindings } from './runtime-context.js';
import { readApiResponse, encodeWireValue } from './api.js';

/** Initializes the instrument registry dashboard feature. */
export function installInstrumentRegistry() {
            const instrumentRegistryPageSizeRows = 120;
            const instrumentRegistryState = {
                rows: [],
                loaded: false,
                totalCount: 0,
                matchingCount: 0,
                nextOffset: 0,
                hasMore: false,
                loading: false,
                loadingAppend: false,
                querySerial: 0,
                controller: null,
                searchTimer: null,
                error: '',
            };
            const instrumentRegistryCategories = {
                stockbit: ['idx_stock'],
                pintu: ['crypto_spot', 'crypto_perpetual'],
                tokocrypto: ['crypto_spot', 'tokenized_stock'],
                pluang: ['crypto_spot', 'crypto_perpetual'],
            };
            const instrumentRegistryExchangeLabels = {
                stockbit: 'Stockbit',
                pintu: 'Pintu',
                tokocrypto: 'Tokocrypto',
                pluang: 'Pluang',
            };
            const instrumentRegistryCategoryLabels = {
                idx_stock: 'IDX Stock',
                crypto_perpetual: 'Crypto Perpetual',
                crypto_spot: 'Crypto Spot',
                tokenized_stock: 'Tokenized Stock',
            };
            const instrumentRegistryStatusLabels = {
                regulator_approved: 'Regulator approved · venue status unknown',
                public_listed_snapshot: 'Public listing snapshot',
                manual_unverified: 'Manual · unverified',
                venue_active: 'App listed',
                venue_delisted: 'No longer in latest app listing',
                classification_pending: 'App listed · asset type pending',
            };
            const instrumentRegistryProducts = {
                all: [],
                stockbit: [['stockbit_regular', 'IDX Listed Stocks']],
                pintu: [['pintu_retail_spot', 'Retail Spot'], ['pintu_pro_spot', 'Pro Spot'], ['pintu_pro_futures', 'Pro Futures']],
                tokocrypto: [['tokocrypto_spot', 'Spot'], ['tokocrypto_tokenized_stock', 'Tokenized Stock']],
                pluang: [['pluang_spot', 'Spot'], ['pluang_perpetual', 'Perpetual']],
            };
            const instrumentRegistrySourceDocs = {
                idx_listed_company_directory: ['IDX listed-company directory', 'Imported from the public IDX listed-company directory.'],
                stockbit_public_symbol_sitemap: ['Legacy Stockbit symbol sitemap', 'Imported from the earlier public Stockbit symbol sitemap; the current refresh uses the IDX listed-company directory.'],
                pintu_retail_market_app_listing: ['Pintu Retail market listing', 'Discovered from Pintu Retail’s public market listing.'],
                pintu_pro_spot_app_listing: ['Pintu Pro Spot market listing', 'Discovered from Pintu Pro Spot’s public market listing.'],
                pintu_pro_futures_app_listing: ['Pintu Pro Futures market listing', 'Discovered from Pintu Pro Futures’ public market listing.'],
                'pintu_regulator_approved_code_list+app_listing': ['Pintu code list + app listing', 'The ticker began in the saved regulator-approved Pintu code list and was later seen in the Pintu app listing.'],
                pintu_regulator_approved_code_list: ['Saved Pintu futures code list', 'Present in the saved regulator-approved futures code list. This alone does not confirm a current Pintu app listing.'],
                tokocrypto_live_symbols_api: ['Tokocrypto public symbols API', 'Imported from Tokocrypto’s public symbols API.'],
                pluang_public_spot_market_snapshot: ['Pluang public Spot snapshot', 'Imported from the saved public Pluang Spot listing snapshot.'],
                pluang_public_futures_market_snapshot: ['Pluang public Perpetual snapshot', 'Imported from the saved public Pluang futures listing snapshot.'],
                dashboard_manual_registration: ['Manual dashboard entry', 'Entered manually here. No venue listing source was supplied; venue verification is pending.'],
                local_market_history: ['Legacy local-history import', 'Imported from local market history; the original venue-listing source was not retained.'],
            };

            function instrumentRegistryEscape(value) {
                return String(value ?? '').replace(/[&<>"']/g, character => ({
                    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
                }[character]));
            }

            function instrumentRegistrySourceDetails(row) {
                const source = String(row.registration_source || '');
                const [label, detail] = instrumentRegistrySourceDocs[source]
                    || ['Unmapped source record', 'Stored source identifier: ' + (source || 'not recorded') + '. Review the source link if one is available.'];
                return { source, label, detail };
            }

            function instrumentRegistrySafeSourceUrl(value) {
                try {
                    const url = new URL(String(value || ''));
                    return url.protocol === 'https:' ? url.href : '';
                } catch (_) {
                    return '';
                }
            }

            function instrumentRegistryDate(value, unixSeconds = false) {
                if (value === null || value === undefined || value === '') return 'Not recorded';
                let date;
                if (unixSeconds) {
                    const seconds = Number(value);
                    if (!Number.isFinite(seconds) || seconds <= 0) return 'Not recorded';
                    date = new Date(seconds * 1000);
                } else {
                    date = new Date(String(value));
                }
                if (!Number.isFinite(date.getTime())) return instrumentRegistryEscape(value);
                return new Intl.DateTimeFormat('en-GB', {
                    timeZone: 'Asia/Jakarta', year: 'numeric', month: 'short', day: '2-digit',
                    ...(unixSeconds ? { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' } : {}),
                }).format(date) + (unixSeconds ? ' WIB' : '');
            }

            function updateInstrumentRegistryProductFilter() {
                const exchange = document.getElementById('instrumentVenueFilter')?.value || 'all';
                const select = document.getElementById('instrumentProductFilter');
                if (!select) return;
                const previous = select.value;
                const options = instrumentRegistryProducts[exchange] || [];
                select.innerHTML = '<option value="all">All products</option>' + options.map(([id, label]) =>
                    '<option value="' + instrumentRegistryEscape(id) + '">' + instrumentRegistryEscape(label) + '</option>').join('');
                select.value = options.some(([id]) => id === previous) ? previous : 'all';
                select.disabled = exchange === 'all' || options.length === 0;
            }

            function updateInstrumentRegistryCategories() {
                const exchange = document.getElementById('instrumentExchangeSelect')?.value || 'stockbit';
                const categorySelect = document.getElementById('instrumentCategorySelect');
                const productSelect = document.getElementById('instrumentProductSelect');
                const note = document.getElementById('instrumentRegistryFormNote');
                if (!categorySelect) return;
                const options = instrumentRegistryCategories[exchange] || [];
                const masterCategories = globalThis.MASTER_ASSET_CATEGORIES || [];
                categorySelect.innerHTML = options.map(id => {
                    const label = masterCategories.find(category => category.id === id)?.label
                        || instrumentRegistryCategoryLabels[id] || id;
                    return '<option value="' + instrumentRegistryEscape(id) + '">' + instrumentRegistryEscape(label) + '</option>';
                }).join('');
                if (productSelect) {
                    const productOptions = exchange === 'pintu'
                        ? [['pintu_retail_spot', 'Retail Spot'], ['pintu_pro_spot', 'Pro Spot'], ['pintu_pro_futures', 'Pro Futures']]
                        : exchange === 'pluang'
                            ? [['pluang_spot', 'Spot'], ['pluang_perpetual', 'Perpetual']]
                            : [['', 'App']];
                    productSelect.innerHTML = productOptions.map(([id, label]) =>
                        `<option value="${id}">${label}</option>`).join('');
                    productSelect.disabled = productOptions.length === 1;
                    productSelect.value = categorySelect.value === 'crypto_perpetual'
                        ? (exchange === 'pintu' ? 'pintu_pro_futures' : exchange === 'pluang' ? 'pluang_perpetual' : '')
                        : productOptions[0][0];
                }
                const notes = {
                    stockbit: 'Saved as a Stockbit IDX ticker with a .JK storage symbol. Registration does not verify a current listing.',
                    pintu: 'Manual additions stay pending until the matching Pintu app product confirms them.',
                    tokocrypto: categorySelect.value === 'tokenized_stock'
                        ? 'Tokenized-stock identity stays with Tokocrypto. Underlying stock history is a research proxy.'
                        : 'Spot is labeled IDR; any stored -USD price history is a research proxy, not a Tokocrypto quote.',
                    pluang: 'Pluang registry entries come from the saved public snapshot shown in the source column. Manual additions stay pending.',
                };
                if (note) note.textContent = notes[exchange] || '';
                const ticker = document.getElementById('instrumentTickerInput');
                if (ticker) {
                    ticker.placeholder = exchange === 'stockbit' ? 'e.g. BBCA or BBCA.JK'
                        : (categorySelect.value === 'crypto_perpetual')
                            ? 'e.g. BTC or BTCUSDT-PERP'
                            : 'e.g. BTC or BTC/IDR';
                }
            }

            function renderInstrumentRegistry() {
                const tbody = document.getElementById('instrumentRegistryRows');
                const status = document.getElementById('instrumentRegistryStatus');
                const more = document.getElementById('instrumentRegistryMore');
                const pageLabel = document.getElementById('instrumentRegistryPageLabel');
                if (!tbody) return;
                const visible = instrumentRegistryState.rows;
                tbody.innerHTML = visible.map(row => {
                    const venue = (instrumentRegistryExchangeLabels[row.exchange_id] || row.exchange_id)
                        + (row.product_id ? ' · ' + row.product_id.replaceAll('_', ' ') : ' · App');
                    const category = instrumentRegistryCategoryLabels[row.asset_category_id] || row.asset_category_id || 'Asset type not recorded';
                    const source = instrumentRegistrySourceDetails(row);
                    const sourceUrl = instrumentRegistrySafeSourceUrl(row.source_url);
                    const listing = instrumentRegistryStatusLabels[row.listing_status] || row.listing_status || 'Status not recorded';
                    const verification = row.venue_verified
                        ? '<span class="registry-secondary registry-verified">Venue listing verified</span>'
                        : '<span class="registry-secondary registry-unverified">Venue listing unverified</span>';
                    const execution = row.execution_supported
                        ? 'Execution support configured'
                        : 'Execution support not configured';
                    const sourceLink = sourceUrl
                        ? '<a class="registry-source-link" href="' + instrumentRegistryEscape(sourceUrl) + '" target="_blank" rel="noopener noreferrer">Open source</a>'
                        : '<span class="registry-secondary">No source URL recorded</span>';
                    const priceSourceLabels = {
                        tokocrypto_native: 'Tokocrypto native candles',
                        pintu_native_then_research_proxy: 'Pintu-native first; proxy fallback',
                        yfinance_research_proxy: 'Yahoo Finance research proxy',
                    };
                    const priceSource = priceSourceLabels[row.price_source_policy]
                        || String(row.price_source_policy || 'Source policy not recorded').replaceAll('_', ' ');
                    return '<tr><td><span class="registry-primary">' + instrumentRegistryEscape(venue) +
                        '</span><span class="registry-secondary">' + instrumentRegistryEscape(category) +
                        '</span></td><td><span class="registry-primary" title="' + instrumentRegistryEscape(row.venue_symbol) + '">' +
                        instrumentRegistryEscape(row.symbol) + '</span><span class="registry-secondary">Venue: ' +
                        instrumentRegistryEscape(row.venue_symbol || 'not recorded') + '</span><span class="registry-secondary">History key: ' +
                        instrumentRegistryEscape(row.storage_symbol || 'not recorded') + (row.quote_currency ? ' · ' + instrumentRegistryEscape(row.quote_currency) : '') +
                        '</span></td><td><span class="registry-primary">' + instrumentRegistryEscape(source.label) +
                        '</span><span class="registry-secondary">' + instrumentRegistryEscape(source.detail) +
                        '</span><span class="registry-secondary">Record: ' + instrumentRegistryEscape(source.source || 'not recorded') +
                        '</span></td><td>' + sourceLink + '<span class="registry-secondary">Snapshot: ' +
                        instrumentRegistryDate(row.source_snapshot_at) + '</span></td><td><span class="registry-status">' +
                        instrumentRegistryEscape(listing) + '</span>' + verification + '<span class="registry-secondary">' +
                        instrumentRegistryEscape(execution) + '</span></td><td><span class="registry-primary">' +
                        instrumentRegistryEscape(priceSource) + '</span><span class="registry-secondary">Policy: ' +
                        instrumentRegistryEscape(row.price_source_policy || 'not recorded') + '</span></td><td>' +
                        instrumentRegistryDate(row.registered_at, true) + '</td></tr>';
                }).join('');
                if (!visible.length && instrumentRegistryState.loaded) {
                    tbody.innerHTML = '<tr><td colspan="7" class="empty-cell">' +
                        'No registered tickers match. Add one above or change the filters.' +
                        '</td></tr>';
                }
                if (status) {
                    status.textContent = instrumentRegistryState.loading
                        ? (instrumentRegistryState.loadingAppend
                            ? 'Loading more registered tickers…'
                            : (instrumentRegistryState.loaded
                                ? 'Updating registry filters…'
                                : 'Loading registered tickers…'))
                        : instrumentRegistryState.error
                        ? 'Could not load the ticker registry: ' + instrumentRegistryState.error
                        : instrumentRegistryState.loaded
                            ? 'Showing ' + visible.length + ' of ' + instrumentRegistryState.matchingCount +
                        ' matching tickers · ' + instrumentRegistryState.totalCount +
                            ' active across all apps. Registration, venue verification, and price history are separate.'
                            : 'Registry data has not loaded.';
                }
                if (pageLabel) {
                    pageLabel.textContent = instrumentRegistryState.loaded
                        ? `Showing ${visible.length} of ${instrumentRegistryState.matchingCount} matching tickers · ${instrumentRegistryState.totalCount} active across all apps.`
                        : instrumentRegistryState.loading ? 'Loading registry…' : 'Registry count unavailable.';
                }
                if (more) {
                    more.classList.toggle('hidden', !instrumentRegistryState.hasMore && !instrumentRegistryState.loading);
                    more.disabled = instrumentRegistryState.loading;
                    more.textContent = instrumentRegistryState.loading
                        ? 'Loading…'
                        : 'Show ' + Math.min(instrumentRegistryPageSizeRows, Math.max(0, instrumentRegistryState.matchingCount - visible.length)) + ' more';
                }
            }

            async function loadInstrumentRegistry({ append = false } = {}) {
                const status = document.getElementById('instrumentRegistryStatus');
                const retry = document.getElementById('instrumentRegistryRetry');
                const tbody = document.getElementById('instrumentRegistryRows');
                const requestId = ++instrumentRegistryState.querySerial;
                instrumentRegistryState.controller?.abort();
                const controller = new AbortController();
                instrumentRegistryState.controller = controller;
                const offset = append ? instrumentRegistryState.nextOffset : 0;
                if (!append) {
                    instrumentRegistryState.rows = [];
                    instrumentRegistryState.loaded = false;
                    instrumentRegistryState.hasMore = false;
                    instrumentRegistryState.nextOffset = 0;
                }
                instrumentRegistryState.loading = true;
                instrumentRegistryState.loadingAppend = append;
                instrumentRegistryState.error = '';
                if (status) status.textContent = append
                    ? 'Loading more registered tickers…'
                    : 'Loading registered tickers…';
                if (retry) retry.classList.add('hidden');
                if (tbody && !append) tbody.innerHTML = '<tr><td colspan="7" class="empty-cell">Loading registry…</td></tr>';
                renderInstrumentRegistry();
                try {
                    const query = new URLSearchParams({
                        limit: String(instrumentRegistryPageSizeRows),
                        offset: String(offset),
                    });
                    const exchange = document.getElementById('instrumentVenueFilter')?.value || 'all';
                    const product = document.getElementById('instrumentProductFilter')?.value || 'all';
                    const search = (document.getElementById('instrumentSearchInput')?.value || '').trim();
                    if (exchange !== 'all') query.set('exchange', exchange);
                    if (product !== 'all') query.set('product', product);
                    if (search) query.set('search', search);
                    const response = await fetch('/api/instruments?' + query.toString(), {
                        cache: 'no-store',
                        signal: controller.signal,
                    });
                    const payload = await readApiResponse(response);
                    if (!response.ok) throw new Error(payload.error || 'HTTP ' + response.status);
                    if (requestId !== instrumentRegistryState.querySerial) return;
                    const pageRows = Array.isArray(payload.instruments) ? payload.instruments : [];
                    instrumentRegistryState.rows = append
                        ? instrumentRegistryState.rows.concat(pageRows)
                        : pageRows;
                    instrumentRegistryState.totalCount = Number(payload.registered_count) || 0;
                    instrumentRegistryState.matchingCount = Number(payload.matching_count) || 0;
                    instrumentRegistryState.nextOffset = Number(payload.next_offset) || 0;
                    instrumentRegistryState.hasMore = payload.has_more === true;
                    instrumentRegistryState.loaded = true;
                    instrumentRegistryState.error = '';
                } catch (error) {
                    if (requestId !== instrumentRegistryState.querySerial || error.name === 'AbortError') return;
                    instrumentRegistryState.error = error.message;
                    if (!append) {
                        instrumentRegistryState.loaded = false;
                        if (tbody) tbody.innerHTML = '<tr><td colspan="7" class="empty-cell error-text">Registry unavailable. Retry after the API is back online.</td></tr>';
                    }
                    if (retry) retry.classList.remove('hidden');
                } finally {
                    if (requestId === instrumentRegistryState.querySerial) {
                        instrumentRegistryState.loading = false;
                        instrumentRegistryState.controller = null;
                        renderInstrumentRegistry();
                    }
                }
            }

            function initializeInstrumentRegistry() {
                const dialog = document.getElementById('instrumentRegistryDialog');
                const opener = document.getElementById('openInstrumentRegistry');
                const form = document.getElementById('instrumentRegisterForm');
                if (!form) return;
                if (dialog && opener) opener.addEventListener('click', () => {
                    if (!dialog.open) dialog.showModal();
                    updateInstrumentRegistryCategories();
                    updateInstrumentRegistryProductFilter();
                    loadInstrumentRegistry();
                });
                document.getElementById('closeInstrumentRegistry')?.addEventListener('click', () => dialog?.close());
                dialog?.addEventListener('click', event => {
                    if (event.target === dialog) dialog.close();
                });
                document.getElementById('instrumentExchangeSelect')?.addEventListener('change', updateInstrumentRegistryCategories);
                document.getElementById('instrumentCategorySelect')?.addEventListener('change', updateInstrumentRegistryCategories);
                document.getElementById('instrumentProductSelect')?.addEventListener('change', event => {
                    const category = document.getElementById('instrumentCategorySelect');
                    if (category) category.value = String(event.target.value).includes('futures')
                        || String(event.target.value).includes('perpetual') ? 'crypto_perpetual' : 'crypto_spot';
                });
                document.getElementById('instrumentSearchInput')?.addEventListener('input', () => {
                    clearTimeout(instrumentRegistryState.searchTimer);
                    instrumentRegistryState.controller?.abort();
                    instrumentRegistryState.querySerial += 1;
                    instrumentRegistryState.controller = null;
                    instrumentRegistryState.loading = true;
                    instrumentRegistryState.loadingAppend = false;
                    const status = document.getElementById('instrumentRegistryStatus');
                    if (status) status.textContent = 'Updating registry search…';
                    renderInstrumentRegistry();
                    instrumentRegistryState.searchTimer = window.setTimeout(() => {
                        void loadInstrumentRegistry();
                    }, 250);
                });
                document.getElementById('instrumentVenueFilter')?.addEventListener('change', () => {
                    updateInstrumentRegistryProductFilter();
                    void loadInstrumentRegistry();
                });
                document.getElementById('instrumentProductFilter')?.addEventListener('change', () => {
                    void loadInstrumentRegistry();
                });
                document.getElementById('instrumentRegistryFilters')?.addEventListener('submit', event => {
                    event.preventDefault();
                    void loadInstrumentRegistry();
                });
                document.getElementById('instrumentRegistryRefresh')?.addEventListener('click', () => {
                    void loadInstrumentRegistry();
                });
                document.getElementById('instrumentRegistryMore')?.addEventListener('click', () => {
                    void loadInstrumentRegistry({ append: true });
                });
                document.getElementById('instrumentRegistryRetry')?.addEventListener('click', () => {
                    void loadInstrumentRegistry({ append: instrumentRegistryState.hasMore });
                });
                updateInstrumentRegistryCategories();
                updateInstrumentRegistryProductFilter();
                if (!dialog) void loadInstrumentRegistry();
                form.addEventListener('submit', async event => {
                    event.preventDefault();
                    const submit = form.querySelector('button[type="submit"]');
                    const note = document.getElementById('instrumentRegistryFormNote');
                    const body = {
                        exchange_id: document.getElementById('instrumentExchangeSelect')?.value || '',
                        asset_category_id: document.getElementById('instrumentCategorySelect')?.value || '',
                        product_id: document.getElementById('instrumentProductSelect')?.value || '',
                        symbol: document.getElementById('instrumentTickerInput')?.value || '',
                    };
                    if (submit) submit.disabled = true;
                    if (note) note.textContent = 'Registering ticker…';
                    try {
                        const response = await fetch('/api/instruments', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json; charset=utf-8' },
                            body: encodeWireValue(body),
                        });
                        const payload = await readApiResponse(response);
                        if (!response.ok) throw new Error(payload.error || 'HTTP ' + response.status);
                        if (note) note.textContent = payload.created
                            ? payload.symbol + ' registered as unverified. It is not enabled for execution.'
                            : payload.symbol + ' is already registered for ' +
                                (instrumentRegistryExchangeLabels[payload.exchange_id] || payload.exchange_id) + '.';
                        document.getElementById('instrumentVenueFilter').value = payload.exchange_id;
                        updateInstrumentRegistryProductFilter();
                        const productFilter = document.getElementById('instrumentProductFilter');
                        if (productFilter && [...productFilter.options].some(option => option.value === payload.product_id)) {
                            productFilter.value = payload.product_id;
                        }
                        document.getElementById('instrumentSearchInput').value = payload.symbol;
                        document.getElementById('instrumentTickerInput').value = '';
                        await loadInstrumentRegistry();
                    } catch (error) {
                        if (note) note.textContent = 'Ticker was not registered: ' + error.message;
                    } finally {
                        if (submit) submit.disabled = false;
                    }
                });
            }

            function parseStockbitFlowJson(text) {
                let payload;
                try {
                    payload = JSON.parse(text);
                } catch (error) {
                    throw new Error(`Enter valid JSON: ${error.message}`);
                }
                if (!payload || typeof payload !== 'object' || Array.isArray(payload)
                    || !Array.isArray(payload.records)) {
                    throw new Error('The JSON object must contain a records array.');
                }
                const records = payload.records;
                if (!records.length) throw new Error('The records array is empty.');
                if (records.length > 2000) throw new Error('Import at most 2,000 rows at a time.');
                if (records.some(record => !record || typeof record !== 'object' || Array.isArray(record))) {
                    throw new Error('Every records entry must be a JSON object.');
                }
                return records;
            }

            async function importStockbitFlowJson(text, statusNode) {
                try {
                    if (statusNode) statusNode.textContent = 'Validating pasted JSON…';
                    const records = parseStockbitFlowJson(text);
                    if (statusNode) statusNode.textContent = `Importing ${records.length} flow rows into SQLite…`;
                    let saved = 0;
                    for (let start = 0; start < records.length; start += 100) {
                        const response = await fetch('/api/stockbit/foreign-flow/import', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json; charset=utf-8' },
                            body: JSON.stringify({ records: records.slice(start, start + 100) }),
                        });
                        const payload = await readApiResponse(response);
                        if (!response.ok) throw new Error(payload.error || `HTTP ${response.status}`);
                        saved += Number(payload.imported_rows) || 0;
                        if (statusNode) statusNode.textContent = `Imported ${saved} of ${records.length} flow rows into SQLite…`;
                    }
                    if (statusNode) statusNode.textContent = `Saved ${saved} dated flow rows to SQLite. Reloading signal context…`;
                    window.setTimeout(() => window.location.reload(), 700);
                } catch (error) {
                    if (statusNode) statusNode.textContent = `Import failed: ${error.message}`;
                }
            }

            function initStockbitForeignFlowImport() {
                document.addEventListener('click', event => {
                    const button = event.target.closest('[data-stockbit-flow-import]');
                    if (!button) return;
                    const panel = button.closest('details');
                    const input = panel?.querySelector('[data-stockbit-flow-json]');
                    const status = panel?.querySelector('[data-stockbit-flow-status]');
                    if (!input) {
                        if (status) status.textContent = 'JSON input is unavailable.';
                        return;
                    }
                    void importStockbitFlowJson(input.value, status);
                });
            }

    publishDashboardBindings('instrument-registry', {
        instrumentRegistryState: { get: () => instrumentRegistryState },
        instrumentRegistryCategories: { get: () => instrumentRegistryCategories },
        instrumentRegistryExchangeLabels: { get: () => instrumentRegistryExchangeLabels },
        instrumentRegistryCategoryLabels: { get: () => instrumentRegistryCategoryLabels },
        instrumentRegistryStatusLabels: { get: () => instrumentRegistryStatusLabels },
        instrumentRegistryEscape: { get: () => instrumentRegistryEscape },
        updateInstrumentRegistryCategories: { get: () => updateInstrumentRegistryCategories },
        renderInstrumentRegistry: { get: () => renderInstrumentRegistry },
        loadInstrumentRegistry: { get: () => loadInstrumentRegistry },
        initializeInstrumentRegistry: { get: () => initializeInstrumentRegistry },
        parseStockbitFlowJson: { get: () => parseStockbitFlowJson },
        importStockbitFlowJson: { get: () => importStockbitFlowJson },
        initStockbitForeignFlowImport: { get: () => initStockbitForeignFlowImport },
    });
}
