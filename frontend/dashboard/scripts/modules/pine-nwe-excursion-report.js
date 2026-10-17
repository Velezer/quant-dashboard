import { getDashboardBinding } from './runtime-context.js';
import { CHART_BUNDLE_HISTORY_BARS, CHART_INDICATOR_IDS } from './chart-data.js';
import { NWE_LOWER_SOURCE_TITLE, NWE_MAX_OUTSIDE_DISTANCE_SOURCE, NWE_UPPER_SOURCE_TITLE } from './pine-sources.js';

function plotPeak(output, title, candles) {
    const plot = output?.plots?.find(item => item.title === title);
    if (!Array.isArray(plot?.values)) return null;
    let peak = null;
    let index = -1;
    plot.values.forEach((value, candidateIndex) => {
        if (!Number.isFinite(value)) return;
        if (peak === null || value > peak) {
            peak = value;
            index = candidateIndex;
        }
    });
    return peak === null ? null : { value: peak, index, candle: candles[index] };
}

export function excursionSummary(output, candles) {
    const upper = plotPeak(output, 'Max above upper band (%)', candles);
    const lower = plotPeak(output, 'Max below lower band (%)', candles);
    if (!upper && !lower) return null;
    const largest = !lower || (upper && upper.value >= lower.value) ? upper : lower;
    return {
        upper,
        lower,
        maximum: largest?.value ?? null,
        side: largest === upper ? 'Above upper band' : 'Below lower band',
        peakCandle: largest?.candle || null,
    };
}


export function installNweExcursionReport({ status: reportStatus, ensureWorker, callWorker, requestJson, requestAllInstruments, finiteNumber, displayedHistorySource }) {
    const reportRows = document.getElementById("nweExcursionReportRows");
    const reportTimeframeSelect = document.getElementById("nweExcursionReportTimeframe");
    const reportButton = document.getElementById("runNweExcursionReport");
    const cancelReportButton = document.getElementById("cancelNweExcursionReport");
    let reportSerial = 0;
    let reportCancelled = false;
    let reportAbortController;
    const reportDefinition = document.getElementById("nweExcursionReportDefinition");
    if (reportDefinition) {
        reportDefinition.textContent = `Largest candle-wick excursion beyond the upper or lower NWE after a candle touches that band, divided by the touched band price. The all-ticker report defaults to 1H; the chart pane follows the chart timeframe. Uses up to ${CHART_BUNDLE_HISTORY_BARS} stored bars and the current NWE repaint mode. Historical research only; proxy and stale sources stay labeled.`;
    }

    function reportCell(row, text, classes = '') {
        const cell = document.createElement('td');
        cell.className = `border-t border-gray-800 px-2 py-1.5 ${classes}`;
        cell.textContent = text;
        return cell;
    }

    function reportPercent(value) {
        return Number.isFinite(value)
            ? `${new Intl.NumberFormat('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value)}%`
            : '—';
    }

    function reportBarTime(candle) {
        const raw = Number(candle?.time);
        if (!Number.isFinite(raw) || raw <= 0) return candle?.date || '—';
        const milliseconds = raw > 1e15 ? raw / 1e6 : raw > 1e12 ? raw : raw * 1000;
        return new Intl.DateTimeFormat('id-ID', {
            timeZone: 'Asia/Jakarta',
            dateStyle: 'medium',
            timeStyle: 'short',
        }).format(new Date(milliseconds));
    }

    function renderReportRows(rows) {
        if (!reportRows) return;
        const ordered = [...rows].sort((left, right) => {
            if (left.maximum === null) return right.maximum === null ? left.ticker.localeCompare(right.ticker) : 1;
            if (right.maximum === null) return -1;
            return right.maximum - left.maximum;
        });
        if (!ordered.length) {
            reportRows.replaceChildren();
            const row = document.createElement('tr');
            const cell = reportCell({ maximum: null }, 'No registered tickers are available for this scope.', 'text-center text-gray-500');
            cell.colSpan = 7;
            row.append(cell);
            reportRows.append(row);
            return;
        }
        const fragment = document.createDocumentFragment();
        for (const item of ordered) {
            const row = document.createElement('tr');
            row.append(
                reportCell(item, `${item.ticker} · ${item.appCategory}`, 'max-w-48 whitespace-normal break-words font-semibold text-gray-200'),
                reportCell(item, item.maximum === null ? '—' : `${reportPercent(item.maximum)} · ${item.side}`, 'whitespace-nowrap font-mono text-violet-200'),
                reportCell(item, reportPercent(item.upper?.value), 'whitespace-nowrap font-mono text-teal-300'),
                reportCell(item, reportPercent(item.lower?.value), 'whitespace-nowrap font-mono text-rose-300'),
                reportCell(item, item.maximum === null ? '—' : reportBarTime(item.peakCandle), 'whitespace-nowrap text-gray-300'),
                reportCell(item, item.source || '—', 'max-w-40 break-words text-gray-400'),
                reportCell(item, item.history || '—', 'whitespace-nowrap text-gray-400'),
            );
            row.title = item.error || item.dataContext || '';
            fragment.append(row);
        }
        reportRows.replaceChildren(fragment);
    }

    function reportCandle(raw) {
        return {
            open: Number(raw?.open),
            high: Number(raw?.high),
            low: Number(raw?.low),
            close: Number(raw?.close),
            volume: Number(raw?.volume ?? 0),
            time: raw?.time == null || !Number.isFinite(Number(raw.time)) ? null : Number(raw.time),
        };
    }

    function reportAssetClass(category) {
        return ({
            idx_stock: 'idx',
            crypto_spot: 'crypto_spot',
            crypto_perpetual: 'crypto_futures',
            crypto_futures: 'crypto_futures',
            tokenized_stock: 'tokenized_stock',
        })[String(category || '').toLowerCase()] || String(category || '');
    }

    function reportAppCategory(instrument) {
        const app = ({
            stockbit: 'Stockbit',
            tokocrypto: 'Tokocrypto',
            pintu_futures: 'Pintu Futures',
            pintu: 'Pintu',
            pluang: 'Pluang',
        })[String(instrument.exchange_id || '').toLowerCase()] || instrument.exchange_id || 'App unavailable';
        const productId = String(instrument.product_id || '').toLowerCase();
        const product = productId && !productId.startsWith(`${String(instrument.exchange_id || '').toLowerCase()}_`)
            ? productId
            : productId.split('_').slice(1).join('_');
        const category = ({
            idx_stock: 'IDX Stock',
            crypto_spot: 'Crypto Spot',
            crypto_perpetual: 'Crypto Perpetual',
            crypto_futures: 'Crypto Futures',
            tokenized_stock: 'Tokenized Stock',
        })[String(instrument.asset_category_id || '').toLowerCase()] || instrument.asset_category_id || 'Category unavailable';
        const productLabel = product && product.replaceAll('_', ' ') !== category.toLowerCase()
            ? product.split('_').map(word => word.charAt(0).toUpperCase() + word.slice(1)).join(' ')
            : '';
        return `${app}${productLabel ? ` · ${productLabel}` : ''} · ${category}`;
    }

    function reportRequestContext(instrument, timeframe) {
        const originalExchange = String(instrument.exchange_id || '').toLowerCase();
        const exchange = originalExchange === 'pintu_futures' ? 'pintu' : originalExchange;
        const assetClass = reportAssetClass(instrument.asset_category_id);
        let product = String(instrument.product_id || '');
        if (exchange === 'pintu' && !product && assetClass === 'crypto_futures') product = 'pintu_pro_futures';
        if (exchange === 'tokocrypto') product = '';
        const sourceExchange = exchange === 'pintu' && product === 'pintu_pro_futures'
            ? 'pintu_futures'
            : exchange === 'pintu' && ['pintu_retail_spot', 'pintu_pro_spot'].includes(product)
                ? product
                : exchange === 'pluang' && ['pluang_spot', 'pluang_perpetual'].includes(product)
                    ? product
                    : exchange;
        const query = new URLSearchParams({
            symbol: String(instrument.storage_symbol || instrument.symbol || '').toUpperCase(),
            exchange: sourceExchange,
            asset_class: assetClass,
            timeframe,
            limit: String(CHART_BUNDLE_HISTORY_BARS),
        });
        return query;
    }

    function reportHistoryRequest(instrument, timeframe, signal) {
        const query = reportRequestContext(instrument, timeframe);
        return fetch(`/api/chart/candles?${query}`, {
            cache: 'no-store',
            signal,
            headers: { Accept: 'application/json' },
        });
    }

    function reportIndicatorsRequest(instrument, timeframe, signal) {
        const query = reportRequestContext(instrument, timeframe);
        return fetch(`/api/compute/indicators?${query}`, {
            cache: 'no-store',
            signal,
            headers: { Accept: 'application/json' },
        });
    }

    async function loadTickerForReport(instrument, timeframe, mode) {
        reportAbortController = new AbortController();
        const timeout = setTimeout(() => reportAbortController?.abort(), 15000);
        try {
            const [historyResponse, indicatorResponse] = await Promise.all([
                reportHistoryRequest(instrument, timeframe, reportAbortController.signal),
                reportIndicatorsRequest(instrument, timeframe, reportAbortController.signal),
            ]);
            const [payload, indicatorPayload] = await Promise.all([
                historyResponse.json().catch(() => null),
                indicatorResponse.json().catch(() => null),
            ]);
            if (!historyResponse.ok) throw new Error(payload?.error || `Chart history unavailable (HTTP ${historyResponse.status}).`);
            if (!indicatorResponse.ok) throw new Error(indicatorPayload?.error || `NWE indicator history unavailable (HTTP ${indicatorResponse.status}).`);
            const rawBars = Array.isArray(payload?.candles) ? payload.candles : [];
            const indicatorPoints = Array.isArray(indicatorPayload?.points) ? indicatorPayload.points : null;
            if (!indicatorPoints) throw new Error('NWE indicator response did not contain chart points.');
            const bandsByTime = new Map(indicatorPoints.map(point => [
                String(point?.time),
                point?.indicators?.[CHART_INDICATOR_IDS.nadarayaWatsonEnvelope]?.[mode] || null,
            ]));
            const pairs = rawBars.map(raw => ({ raw, candle: reportCandle(raw) }))
                .map(pair => ({ ...pair, bands: bandsByTime.get(String(pair.raw?.time)) || null }))
                .filter(({ candle }) => [candle.open, candle.high, candle.low, candle.close, candle.volume].every(Number.isFinite));
            if (pairs.length && pairs.some(pair => !bandsByTime.has(String(pair.raw?.time)))) {
                throw new Error('NWE indicator points did not align with the stored candle timestamps. Retry after chart data recovers.');
            }
            const candles = pairs.map(({ candle }) => candle);
            const input_sources = {
                [NWE_UPPER_SOURCE_TITLE]: pairs.map(pair => finiteNumber(pair.bands?.upper)),
                [NWE_LOWER_SOURCE_TITLE]: pairs.map(pair => finiteNumber(pair.bands?.lower)),
            };
            const freshness = payload?.series || indicatorPayload?.series || {};
            return {
                candles,
                input_sources,
                mode,
                source: displayedHistorySource(freshness),
                stale: freshness.is_stale === true,
            };
        } finally {
            clearTimeout(timeout);
            reportAbortController = null;
        }
    }

    function instrumentMatchesReportScope(instrument) {
        const exchangeFilter = String(getDashboardBinding('activeExchangeFilter') || 'all').toLowerCase();
        const productFilter = String(getDashboardBinding('activeProductFilter') || 'all').toLowerCase();
        const exchange = String(instrument.exchange_id || '').toLowerCase();
        const exchangeMatches = exchangeFilter === 'all'
            || exchange === exchangeFilter
            || (exchange === 'pintu' && ['pintu_futures', 'pintu_spot'].includes(exchangeFilter));
        if (!exchangeMatches) return false;
        return productFilter === 'all' || String(instrument.product_id || '').toLowerCase() === productFilter;
    }

    async function runNweExcursionReport() {
        if (!reportButton || !reportRows || !reportStatus) return;
        const thisRun = ++reportSerial;
        reportCancelled = false;
        reportButton.disabled = true;
        if (reportTimeframeSelect) reportTimeframeSelect.disabled = true;
        cancelReportButton?.classList.remove('hidden');
        reportRows.replaceChildren();
        reportStatus.textContent = 'Loading active registered tickers and browser WASM…';
        const rows = [];
        try {
            if (!await ensureWorker()) throw new Error('Browser WASM runtime is unavailable. Use Retry WASM after the runtime recovers.');
            reportAbortController = new AbortController();
            const payload = await requestAllInstruments({
                signal: reportAbortController.signal,
                onPage: ({ loaded, total }) => {
                    if (thisRun === reportSerial) {
                        reportStatus.textContent = `Loading active registered tickers… ${loaded} of ${total}.`;
                    }
                },
            });
            reportAbortController = null;
            const instruments = (Array.isArray(payload?.instruments) ? payload.instruments : [])
                .filter(instrumentMatchesReportScope);
            if (!instruments.length) {
                reportStatus.textContent = 'No active registered tickers match the selected app and product scope.';
                renderReportRows([]);
                return;
            }
            const timeframe = String(reportTimeframeSelect?.value || '1h').toLowerCase();
            const mode = getDashboardBinding('nadarayaWatsonRepaintingEnabled')?.(getDashboardBinding('selectedSignal')) === true
                ? 'repainting' : 'endpoint';
            reportStatus.textContent = `Calculating ${instruments.length} registered tickers · ${timeframe.toUpperCase()} · ${mode}.`;
            for (let index = 0; index < instruments.length; index += 1) {
                if (reportCancelled || thisRun !== reportSerial) break;
                const instrument = instruments[index];
                const ticker = String(instrument.symbol || instrument.storage_symbol || 'Unknown ticker');
                const base = {
                    ticker,
                    appCategory: reportAppCategory(instrument),
                    source: '—',
                    history: '—',
                    maximum: null,
                    side: '—',
                    upper: null,
                    lower: null,
                    peakCandle: null,
                };
                try {
                    const loaded = await loadTickerForReport(instrument, timeframe, mode);
                    if (reportCancelled || thisRun !== reportSerial) break;
                    if (!loaded.candles.length) {
                        rows.push({ ...base, history: 'No stored bars', source: loaded.source });
                    } else {
                        const output = await callWorker({
                            type: 'run',
                            source: NWE_MAX_OUTSIDE_DISTANCE_SOURCE,
                            candles: loaded.candles,
                            timeframe,
                            timeframes: { [timeframe]: loaded.candles },
                            input_sources: loaded.input_sources,
                        });
                        if (!output?.valid) throw new Error(output?.diagnostics?.[0]?.message || 'Browser WASM returned no output.');
                        const summary = excursionSummary(output, loaded.candles);
                        const bandCount = Math.min(
                            loaded.input_sources[NWE_UPPER_SOURCE_TITLE].filter(Number.isFinite).length,
                            loaded.input_sources[NWE_LOWER_SOURCE_TITLE].filter(Number.isFinite).length,
                        );
                        const noMetricHistory = bandCount === 0 ? 'NWE warming up' : 'No band touch in history';
                        rows.push({
                            ...base,
                            ...summary,
                            source: loaded.source,
                            history: summary ? `${loaded.candles.length} bars` : noMetricHistory,
                        });
                    }
                } catch (error) {
                    if (reportCancelled || thisRun !== reportSerial) break;
                    rows.push({ ...base, history: 'Unavailable', error: String(error?.message || error) });
                }
                renderReportRows(rows);
                reportStatus.textContent = `Calculated ${index + 1} of ${instruments.length} tickers · ${timeframe.toUpperCase()} · ${mode}${reportCancelled ? ' · cancelling…' : ''}.`;
            }
            if (reportCancelled || thisRun !== reportSerial) {
                reportStatus.textContent = `Report cancelled after ${rows.length} ticker${rows.length === 1 ? '' : 's'}. Run it again to restart for the current app and product scope.`;
            } else {
                renderReportRows(rows);
                reportStatus.textContent = `Report complete · ${rows.length} active registered tickers · ${timeframe.toUpperCase()} · ${mode}. Values use stored history only and are not execution evidence.`;
            }
        } catch (error) {
            reportStatus.textContent = `Report unavailable: ${error.message}. Retry after the registry or browser runtime recovers.`;
            renderReportRows(rows);
        } finally {
            if (thisRun === reportSerial) {
                reportButton.disabled = false;
                if (reportTimeframeSelect) reportTimeframeSelect.disabled = false;
                cancelReportButton?.classList.add('hidden');
                reportAbortController = null;
            }
        }
    }

    reportButton?.addEventListener('click', () => { void runNweExcursionReport(); });
    cancelReportButton?.addEventListener('click', () => {
        reportCancelled = true;
        reportAbortController?.abort();
        if (reportStatus) reportStatus.textContent = 'Cancelling after the current ticker request…';
    });
}
