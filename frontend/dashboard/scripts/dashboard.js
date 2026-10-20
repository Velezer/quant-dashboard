const status = document.createElement('div');
status.id = 'dashboard-runtime-status';
status.setAttribute('role', 'status');
status.setAttribute('aria-live', 'polite');
status.setAttribute('aria-atomic', 'true');
status.style.cssText = 'position:fixed;top:12px;right:12px;z-index:2147483000;max-width:calc(100vw - 24px);padding:10px 14px;border:1px solid #363a45;border-radius:8px;background:#131722;color:#d1d4dc;box-shadow:0 8px 24px rgba(0,0,0,.32);font:13px/1.4 sans-serif';
document.body.append(status);

const dashboardModules = [
    ['dashboard-foundation.js', 'installDashboardFoundation'],
    ['chart-data.js', 'installChartData'],
    ['chart-data-diagnostics.js', 'installChartDataDiagnostics'],
    ['signal-refresh.js', 'installSignalRefresh'],
    ['signal-alerts.js', 'installSignalAlerts'],
    ['signal-app-readiness.js', 'installSignalAppReadiness'],
    ['signal-screener.js', 'installSignalScreener'],
    ['rule-simulator.js', 'installRuleSimulator'],
    ['chart-inspection.js', 'installChartInspection'],
    ['chart-renderer.js', 'installChartRenderer'],
    ['pine-indicators.js', 'installPineIndicators'],
    ['pine-strategies.js', 'installPineStrategies'],
    ['markov-modal.js', 'installMarkovModal'],
    ['markov-analytics.js', 'installMarkovAnalytics'],
    ['chart-diagnostics.js', 'installChartDiagnostics'],
    ['instrument-registry.js', 'installInstrumentRegistry'],
    ['dashboard-bootstrap.js', 'installDashboardBootstrap'],
];

function showLoading() {
    status.dataset.state = 'loading';
    status.setAttribute('role', 'status');
    status.setAttribute('aria-live', 'polite');
    status.replaceChildren(document.createTextNode('Loading dashboard modules…'));
}

function showFailure() {
    status.dataset.state = 'error';
    status.setAttribute('role', 'alert');
    status.setAttribute('aria-live', 'assertive');
    const reload = document.createElement('button');
    reload.type = 'button';
    reload.textContent = 'Reload dashboard';
    reload.style.cssText = 'margin-left:12px;padding:4px 8px;border:1px solid #787b86;border-radius:5px;background:#242835;color:#fff;cursor:pointer';
    reload.addEventListener('click', () => window.location.reload());
    status.replaceChildren(
        document.createTextNode('Dashboard modules could not be initialized. '),
        reload,
    );
}

function bindDashboardActions(getDashboardBinding) {
    document.querySelectorAll('[data-dashboard-action]').forEach(control => {
        const action = control.dataset.dashboardAction;
        if (action !== 'stop-propagation' && typeof getDashboardBinding(action) !== 'function') {
            throw new Error(`Dashboard action ${action} is not exported by a feature module.`);
        }
    });

    const dispatch = event => {
        const source = event.target instanceof Element ? event.target : event.target?.parentElement;
        const control = source?.closest('[data-dashboard-action]');
        if (!control || !document.body.contains(control)) return;
        if ((control.dataset.dashboardActionEvent || 'click') !== event.type) return;

        const actionName = control.dataset.dashboardAction;
        if (actionName === 'stop-propagation') {
            event.stopPropagation();
            return;
        }

        const action = getDashboardBinding(actionName);
        if (actionName === 'filterByStrategy') action(control.value);
        else if (actionName === 'zoomChart') action(Number(control.dataset.dashboardActionArgument));
        else if (actionName === 'onMarkovModalBackdropClick') action(event);
        else action();
    };

    document.body.addEventListener('click', dispatch);
    document.body.addEventListener('change', dispatch);
}

async function loadDashboardModules() {
    showLoading();
    try {
        const imports = await Promise.all([
            import('./modules/runtime-context.js'),
            ...dashboardModules.map(([fileName]) => import(`./modules/${fileName}`)),
        ]);
        const [context] = imports;
        const featureModules = imports.slice(1, 1 + dashboardModules.length);

        featureModules.forEach((feature, index) => feature[dashboardModules[index][1]]());

        bindDashboardActions(context.getDashboardBinding);
        status.remove();
    } catch (error) {
        console.error('Dashboard module initialization failed:', error);
        showFailure();
    }
}

void loadDashboardModules();
