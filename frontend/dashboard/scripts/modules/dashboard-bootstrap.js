import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';

/** Initializes the dashboard bootstrap dashboard feature. */
export function installDashboardBootstrap() {
            async function initApp() {
                setCognitiveFocus(true);
                getDashboardBinding('setChartStatus')?.('no_selection', 'Choose a ranked signal in the screener to load its stored candles here.');
                const momentumPane = document.getElementById('momentum-pane');
                document.body.classList.toggle('momentum-chart-open', Boolean(momentumPane?.open));
                momentumPane?.addEventListener('toggle', () => {
                    document.body.classList.toggle('momentum-chart-open', momentumPane.open);
                    if (momentumPane.open && selectedSignal) {
                        requestAnimationFrame(() => drawCandleChart(selectedSignal));
                    }
                });
                setupLayoutResizers();
                initChartTimeframeControls();
                initializeInstrumentRegistry();
                initStockbitForeignFlowImport();
            
                startCountdown();
                initEventSource();
                fetchDaemonDiagnostics();
                setInterval(fetchDaemonDiagnostics, 15000);
                // Markov analytics are intentionally lazy: they are loaded only when
                // the inline signal modal is opened, not as a separate page or during
                // dashboard bootstrap. This keeps the normal signal payload small.
                await fetchFreshSignals();
            }
            initApp();

    publishDashboardBindings('dashboard-bootstrap', {
        initApp: { get: () => initApp },
    });
}
