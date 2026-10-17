import { publishDashboardBindings } from './runtime-context.js';
import { QuantChart } from './chart-math.js';
import { readApiResponse } from './api.js';

/** Initializes the chart diagnostics dashboard feature. */
export function installChartDiagnostics() {
            async function fetchChartStorageStats(signal = selectedSignal) {
                const details = document.getElementById('chartDataDiagnostics');
                if (!details?.open || !signal) return;

                const requestId = Number(window.chartStorageStatsRequestId || 0) + 1;
                window.chartStorageStatsRequestId = requestId;
                const requestKey = signal ? QuantChart.key(signal) : null;
                const query = new URLSearchParams();
                query.set('symbol', marketSymbolForSignal(signal));
                query.set('exchange', marketVenueForSignal(signal));
                query.set('asset_class', signal.class || signal.asset_class || signal.Class || '');
                const endpoint = `/api/chart/storage/stats?${query.toString()}`;
                const runList = document.getElementById('chart-diagnostics-run-list');
                if (runList) runList.textContent = 'Loading recent sync details…';
                try {
                    const res = await fetch(endpoint, { cache: 'no-store' });
                    if (!res.ok) throw new Error(`storage diagnostics returned HTTP ${res.status}`);
                    const data = await readApiResponse(res);
                    const selectedKey = selectedSignal ? QuantChart.key(selectedSignal) : null;
                    if (requestId !== window.chartStorageStatsRequestId || selectedKey !== requestKey) return;

                    CHART_STORAGE_DIAGNOSTICS = data;
                    updateChartDataDiagnostics(selectedSignal);
                    const symCount = data.total_unique_symbols ?? data.unique_symbols ?? 0;
                    const chartCount = data.summary_series_count ?? data.total_chart_files ?? data.total_charts ?? data.ticker_database_count ?? 0;
                    const sizeMb = data.total_disk_usage_mb ?? data.total_size_mb ?? 0;
                    const el = document.getElementById('chart-storage-status');
                    const series = Array.isArray(data.series)
                        ? data.series
                        : (data.series && typeof data.series === 'object' ? Object.values(data.series) : []);
                    const unresolved = data.sync?.latest_failure || series
                        .filter(item => item && item.last_error)
                        .sort((a, b) => String(b.last_attempt || '').localeCompare(String(a.last_attempt || '')))[0];
                    if (el) {
                        if (data.status === 'missing' || data.status === 'error') {
                            el.className = 'inline-flex items-center px-2 py-0.5 rounded border border-amber-700/80 bg-amber-950/70 text-amber-300 text-[10px] font-bold';
                            el.innerText = data.status === 'missing' ? '⚠ HISTORY · NOT AVAILABLE' : '⚠ HISTORY · READ ERROR';
                            el.title = data.recovery || data.error || 'Retry after ticker history is restored.';
                        } else if (unresolved) {
                            el.className = 'inline-flex items-center px-2 py-0.5 rounded border border-amber-700/80 bg-amber-950/70 text-amber-300 text-[10px] font-bold';
                            el.innerText = `⚠ SYNC · ${syncFailureLabel(unresolved.error)}`;
                            el.title = syncFailureTitle(unresolved);
                        } else {
                            el.className = 'hidden';
                            el.innerText = '';
                            el.removeAttribute('title');
                        }
                    }
                    const chartScope = data.summary_truncated
                        ? `in the first ${data.summary_limit} ticker databases`
                        : `across ${symCount} symbols`;
                    console.log(`[Storage] Market history: ${chartCount} series ${chartScope} (${sizeMb} MB)`);
                    if (unresolved) console.warn(`[Storage] ${syncFailureTitle(unresolved)}`);
                } catch (e) {
                    console.warn("[Storage] Failed to fetch chart storage stats:", e);
                    const selectedKey = selectedSignal ? QuantChart.key(selectedSignal) : null;
                    if (requestId === window.chartStorageStatsRequestId
                        && details.open
                        && selectedKey === requestKey
                        && runList) {
                        runList.textContent = 'Stored sync details are unavailable. Chart freshness above remains available; close and reopen Data details to retry.';
                    }
                }
            }

            const chartDataDiagnosticsDetails = document.getElementById('chartDataDiagnostics');
            if (chartDataDiagnosticsDetails && !chartDataDiagnosticsDetails.dataset.storageStatsBound) {
                chartDataDiagnosticsDetails.dataset.storageStatsBound = 'true';
                chartDataDiagnosticsDetails.addEventListener('toggle', () => {
                    if (chartDataDiagnosticsDetails.open && selectedSignal) {
                        void fetchChartStorageStats(selectedSignal);
                    }
                });
            }

            // Start App
            // Keep diagnostics optional: the dashboard must still load signals and
            // charts when a status payload is unavailable or partially populated.
            async function fetchDaemonDiagnostics() {
                try {
                    const res = await fetch('/api/status', { cache: 'no-store' });
                    if (!res.ok) return;
                    const status = await readApiResponse(res);
                    const intervalSeconds = Number(status.scan_interval_minutes) * 60;
                    if (Number.isFinite(intervalSeconds) && intervalSeconds > 0) countdownSeconds = intervalSeconds;
                    setNextScanEpoch(status.next_scan_epoch);
                    const badge = document.getElementById('daemon-health-badge');
                    const state = document.getElementById('daemonModalProcessState');
                    if (badge) {
                        badge.textContent = status.is_scanning
                            ? 'Scanner is running. Signal freshness is shown in the screener.'
                            : 'Scanner is idle. Signal freshness is shown in the screener.';
                    }
                    if (state && status.status) state.textContent = `🟢 ${String(status.status).toUpperCase()}`;
                } catch (error) {
                    console.warn('[Diagnostics] Status unavailable:', error);
                }
            }

            function closeDaemonDiagnosticModal() {
                document.getElementById('daemonDiagnosticModal')?.classList.add('hidden');
            }

    publishDashboardBindings('chart-diagnostics', {
        fetchChartStorageStats: { get: () => fetchChartStorageStats },
        fetchDaemonDiagnostics: { get: () => fetchDaemonDiagnostics },
        closeDaemonDiagnosticModal: { get: () => closeDaemonDiagnosticModal },
    });
}
