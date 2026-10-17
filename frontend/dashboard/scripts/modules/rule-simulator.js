import { getDashboardBinding, publishDashboardBindings } from './runtime-context.js';
import { readApiResponse, encodeWireValue } from './api.js';
import { accountGrowthChartModel, formatUsdBalance } from './rule-simulator-growth-model.js';
import { createRuleSimulatorGrowthChart } from './rule-simulator-growth-chart.js';
import { parseGrowthProjectionInputs, parseRuleInputs } from './rule-simulator-inputs.js';

export { accountGrowthChartModel, formatUsdBalance } from './rule-simulator-growth-model.js';

const DEBOUNCE_MS = 180;
const USD_AXIS_FORMATTER = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    notation: 'compact',
    maximumFractionDigits: 2,
});

/** Installs a saved, fee-aware R:R calculator without adding execution state. */
export function installRuleSimulator() {
    const trigger = document.createElement('button');
    trigger.type = 'button';
    trigger.id = 'openRuleSimulator';
    trigger.textContent = 'Simulate R:R';
    trigger.className = 'rounded-md border border-cyan-800 bg-cyan-950/50 px-2 py-1 text-[10px] font-semibold text-cyan-200 hover:bg-cyan-900 focus:outline focus:outline-2 focus:outline-cyan-400';
    trigger.setAttribute('aria-haspopup', 'dialog');
    trigger.setAttribute('aria-controls', 'ruleSimulatorDialog');
    trigger.dataset.dashboardAction = 'openRuleSimulator';
    document.querySelector('.tv-watchlist-actions')?.append(trigger);

    const dialog = document.createElement('dialog');
    dialog.id = 'ruleSimulatorDialog';
    dialog.className = 'w-[min(920px,96vw)] max-w-none rounded-xl border border-gray-700 bg-gray-950 p-0 text-gray-200 shadow-2xl';
    dialog.setAttribute('aria-labelledby', 'ruleSimulatorTitle');
    dialog.innerHTML = `
        <section class="flex max-h-[92vh] flex-col">
            <header class="flex items-start justify-between gap-4 border-b border-gray-800 px-4 py-3 md:px-6">
                <div>
                    <h2 id="ruleSimulatorTitle" class="text-base font-semibold text-white">R:R rule simulator</h2>
                    <p class="mt-1 text-[11px] leading-relaxed text-gray-400">Edit and save your setup, then review the projection using configured fee assumptions when available. Unknown fees stay excluded unless you enter both rates.</p>
                </div>
                <button id="closeRuleSimulator" type="button" class="rounded-md border border-gray-700 px-2.5 py-1.5 text-xs text-gray-300 hover:bg-gray-800 focus:outline focus:outline-2 focus:outline-cyan-400" aria-label="Close R:R rule simulator" data-dashboard-action="closeRuleSimulator">Close</button>
            </header>
            <div class="flex flex-col gap-4 overflow-y-auto p-4 md:px-6">
                <div id="ruleSimulatorLoadState" class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-gray-800 bg-gray-900/60 px-3 py-2 text-[11px] text-gray-300" role="status" aria-live="polite">Loading saved rule…</div>
                <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-gray-800 bg-gray-900/50 px-3 py-2">
                    <div>
                        <p id="ruleSimulatorVenue" class="text-xs font-semibold text-white">Selected app</p>
                        <p id="ruleSimulatorFeeSource" class="mt-1 text-[10px] leading-relaxed text-gray-400">Loading venue fee policy…</p>
                        <a id="ruleSimulatorFeeRulesLink" class="mt-1 hidden text-[10px] font-semibold text-cyan-300 underline decoration-cyan-800 underline-offset-2 hover:text-cyan-200 focus:outline focus:outline-2 focus:outline-cyan-400" target="_blank" rel="noopener noreferrer">View official fee rules ↗</a>
                    </div>
                    <button id="retryRuleSimulatorLoad" type="button" class="hidden rounded-md border border-amber-800 px-2.5 py-1.5 text-[10px] font-semibold text-amber-200 hover:bg-amber-950 focus:outline focus:outline-2 focus:outline-amber-400" data-dashboard-action="retryRuleSimulatorLoad">Retry load</button>
                </div>
                <form id="ruleSimulatorForm" class="flex flex-col gap-4" data-dashboard-action="simulateRule" data-dashboard-action-event="change">
                    <fieldset class="grid grid-cols-1 gap-3 rounded-lg border border-gray-800 bg-gray-900/40 p-3 sm:grid-cols-2">
                        <legend class="px-1 text-[11px] font-semibold text-gray-300">Saved setup</legend>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorRisk">Risk / stop move (% of trade value)
                            <input id="ruleSimulatorRisk" name="risk_pct" type="number" step="0.01" inputmode="decimal" required class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                        </label>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorReward">Reward / target move (% of trade value)
                            <input id="ruleSimulatorReward" name="reward_pct" type="number" step="0.01" inputmode="decimal" required class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                        </label>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorWinRate">Assumed win rate (%)
                            <input id="ruleSimulatorWinRate" name="win_rate_pct" type="number" step="0.1" inputmode="decimal" required class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                        </label>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorDailyCap">Maximum trades per day
                            <input id="ruleSimulatorDailyCap" name="max_trades_per_day" type="number" step="1" inputmode="numeric" required class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                        </label>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400 sm:col-span-2" for="ruleSimulatorDirection">Scenario direction
                            <select id="ruleSimulatorDirection" name="direction" class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm text-white focus:border-cyan-600 focus:outline-none">
                                <option value="LONG">Long</option>
                                <option value="SHORT">Short</option>
                            </select>
                            <span id="ruleSimulatorDirectionHelp" class="font-normal text-gray-500">Fees are applied to entry and exit values for this direction.</span>
                        </label>
                    </fieldset>
                    <details class="rounded-lg border border-gray-800 bg-gray-900/30">
                        <summary class="cursor-pointer select-none px-3 py-2 text-[11px] font-semibold text-gray-300 focus:outline focus:outline-2 focus:outline-cyan-400">Fee assumptions</summary>
                        <div class="grid grid-cols-1 gap-3 border-t border-gray-800 p-3 sm:grid-cols-2">
                            <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorBuyFee">Buy fee per side (%)
                                <input id="ruleSimulatorBuyFee" name="buy_fee_pct" type="number" min="0" max="100" step="0.0001" inputmode="decimal" placeholder="Fee not configured" class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white placeholder:text-gray-600 focus:border-cyan-600 focus:outline-none">
                            </label>
                            <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorSellFee">Sell fee per side (%)
                                <input id="ruleSimulatorSellFee" name="sell_fee_pct" type="number" min="0" max="100" step="0.0001" inputmode="decimal" placeholder="Fee not configured" class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white placeholder:text-gray-600 focus:border-cyan-600 focus:outline-none">
                            </label>
                            <p id="ruleSimulatorFeeEditNote" class="text-[10px] leading-relaxed text-gray-500 sm:col-span-2">Venue values are starting assumptions. Editing them uses your manual fee assumptions for this simulation; fees are not saved with the R:R rule.</p>
                        </div>
                    </details>
                    <fieldset class="grid grid-cols-1 gap-3 rounded-lg border border-gray-800 bg-gray-900/40 p-3 sm:grid-cols-3">
                        <legend class="px-1 text-[11px] font-semibold text-gray-300">Account growth projection · this view only</legend>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorStartingBalance">Starting balance (USD)
                            <span class="relative">
                                <span aria-hidden="true" class="pointer-events-none absolute inset-y-0 left-2.5 flex items-center text-sm text-gray-400">$</span>
                                <input id="ruleSimulatorStartingBalance" name="starting_balance_units" type="number" step="0.01" inputmode="decimal" required aria-describedby="ruleSimulatorGrowthHelp" class="w-full rounded-md border border-gray-700 bg-gray-950 py-2 pl-7 pr-2.5 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                            </span>
                        </label>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorTradeSize">Trade size (% of current balance)
                            <input id="ruleSimulatorTradeSize" name="trade_size_pct_of_equity" type="number" step="any" inputmode="decimal" required aria-describedby="ruleSimulatorGrowthHelp" class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                        </label>
                        <label class="flex flex-col gap-1 text-[10px] font-semibold text-gray-400" for="ruleSimulatorTradingDays">Modeled trading days
                            <input id="ruleSimulatorTradingDays" name="trading_days" type="number" step="1" inputmode="numeric" required aria-describedby="ruleSimulatorGrowthHelp ruleSimulatorHorizonHelp" class="rounded-md border border-gray-700 bg-gray-950 px-2.5 py-2 text-sm font-mono text-white focus:border-cyan-600 focus:outline-none">
                        </label>
                        <p id="ruleSimulatorHorizonHelp" class="text-[10px] leading-relaxed text-gray-500 sm:col-span-3">Loading supported projection horizon…</p>
                        <p id="ruleSimulatorGrowthHelp" class="text-[10px] leading-relaxed text-gray-500 sm:col-span-3">Balances are shown in USD; currency conversion to the selected venue is not modeled. Assumes the full daily trade cap is filled and outcomes are independent. Only the R:R rule is saved; these projection inputs reset when you reopen this panel.</p>
                    </fieldset>
                    <div class="flex flex-wrap items-center gap-2">
                        <button id="saveRuleSimulator" type="button" class="rounded-md bg-cyan-700 px-3 py-2 text-xs font-semibold text-white hover:bg-cyan-600 focus:outline focus:outline-2 focus:outline-cyan-300 disabled:cursor-not-allowed disabled:opacity-50" data-dashboard-action="saveRuleSimulator" disabled>Save rule</button>
                        <p id="ruleSimulatorSaveState" class="text-[11px] text-gray-400" role="status" aria-live="polite">This draft is not saved.</p>
                    </div>
                </form>
                <section class="rounded-lg border border-gray-800 bg-gray-900/60 p-3" aria-labelledby="ruleSimulatorResultsTitle">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                        <h3 id="ruleSimulatorResultsTitle" class="text-xs font-semibold text-white">R:R scenario and account growth</h3>
                        <span id="ruleSimulatorResultState" class="text-[10px] text-gray-400" role="status" aria-live="polite">Enter a valid rule to calculate.</span>
                    </div>
                    <figure class="mt-3 rounded-lg border border-gray-800 bg-gray-950/50 p-3 sm:p-4" aria-labelledby="ruleSimulatorChartTitle">
                        <figcaption class="flex flex-wrap items-baseline justify-between gap-2">
                            <h4 id="ruleSimulatorChartTitle" class="text-xs font-semibold text-gray-200">Expected account balance over time</h4>
                            <span id="ruleSimulatorChartState" class="text-[11px] text-gray-400" role="status" aria-live="polite">Waiting for valid inputs.</span>
                        </figcaption>
                        <p id="ruleSimulatorEndingBalance" class="mt-3 text-sm font-semibold text-gray-100" aria-live="polite">Expected ending balance: unavailable</p>
                        <div class="mt-2 overflow-x-auto" tabindex="0" role="region" aria-label="Scrollable account growth chart">
                            <svg id="ruleSimulatorGrowthChart" class="hidden w-full min-w-[620px]" viewBox="0 0 720 360" role="img" aria-labelledby="ruleSimulatorChartTitle ruleSimulatorChartDescription">
                                <title id="ruleSimulatorChartAccessibleTitle">Expected account growth projection</title>
                                <desc id="ruleSimulatorChartDescription">The chart will show expected account balance by trading day.</desc>
                                <g id="ruleSimulatorChartPlot"></g>
                            </svg>
                        </div>
                        <p id="ruleSimulatorChartNote" class="mt-2 text-xs leading-relaxed text-gray-400">Expected balance is a probability-weighted arithmetic projection, not a sampled trade path. The USD axis uses a labeled logarithmic scale when the balance range spans at least 100×.</p>
                    </figure>
                    <div class="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3">
                        <div class="rounded-md border border-gray-800 bg-gray-950/70 p-2"><span class="text-[9px] uppercase tracking-wide text-gray-500">Net if target hits</span><p id="ruleSimulatorNetWin" class="mt-1 font-mono text-sm font-semibold text-emerald-300">Unavailable</p></div>
                        <div class="rounded-md border border-gray-800 bg-gray-950/70 p-2"><span class="text-[9px] uppercase tracking-wide text-gray-500">Net if stop hits</span><p id="ruleSimulatorNetLoss" class="mt-1 font-mono text-sm font-semibold text-rose-300">Unavailable</p></div>
                        <div class="rounded-md border border-gray-800 bg-gray-950/70 p-2"><span class="text-[9px] uppercase tracking-wide text-gray-500">Net R:R</span><p id="ruleSimulatorNetRr" class="mt-1 font-mono text-sm font-semibold text-gray-100">Unavailable</p></div>
                        <div class="rounded-md border border-gray-800 bg-gray-950/70 p-2"><span class="text-[9px] uppercase tracking-wide text-gray-500">Break-even win rate</span><p id="ruleSimulatorBreakEven" class="mt-1 font-mono text-sm font-semibold text-gray-100">Unavailable</p></div>
                        <div class="rounded-md border border-gray-800 bg-gray-950/70 p-2 sm:col-span-2"><span class="text-[9px] uppercase tracking-wide text-gray-500">Expected net at daily cap</span><p id="ruleSimulatorDailyExpected" class="mt-1 font-mono text-sm font-semibold text-gray-100">Unavailable</p></div>
                    </div>
                    <p id="ruleSimulatorCaveat" class="mt-3 text-[10px] leading-relaxed text-gray-500">Illustrative arithmetic only. It is not a backtest, forecast, validated win rate, or order instruction.</p>
                    <details class="mt-2 rounded-md border border-gray-800 bg-gray-950/50">
                        <summary class="cursor-pointer select-none px-2.5 py-2 text-[10px] font-semibold text-gray-400 focus:outline focus:outline-2 focus:outline-cyan-400">Calculation assumptions</summary>
                        <p class="border-t border-gray-800 px-2.5 py-2 text-[10px] leading-relaxed text-gray-500">Risk and reward are price moves from entry. Entry and exit fees are applied to their respective values. The daily expectancy card sums same-size trades against one trade's starting value. The growth curve separately compounds a probability-weighted account multiplier using your trade-size percentage. It assumes the daily cap is filled, independent outcomes, and no currency conversion, leverage, funding, slippage, liquidation, or market history. This is not a forecast, backtest, likely path, or performance evidence.</p>
                    </details>
                </section>
            </div>
        </section>`;
    document.body.append(dialog);

    const form = dialog.querySelector('#ruleSimulatorForm');
    const retryButton = dialog.querySelector('#retryRuleSimulatorLoad');
    const buyFeeInput = dialog.querySelector('#ruleSimulatorBuyFee');
    const sellFeeInput = dialog.querySelector('#ruleSimulatorSellFee');
    const growthFields = {
        starting_balance_units: dialog.querySelector('#ruleSimulatorStartingBalance'),
        trade_size_pct_of_equity: dialog.querySelector('#ruleSimulatorTradeSize'),
        trading_days: dialog.querySelector('#ruleSimulatorTradingDays'),
    };
    let feeContext = null;
    let feeEdited = false;
    let simulateTimer = null;
    let saved = false;
    let saveAvailable = false;
    let ruleDirty = false;
    let simulationRevision = 0;
    let tradingDaysPerYear = null;
    const saveButton = dialog.querySelector('#saveRuleSimulator');

    function setFormInputsDisabled(disabled) {
        form.querySelectorAll('input, select').forEach(input => {
            input.disabled = disabled;
        });
    }

    const fields = {
        risk_pct: dialog.querySelector('#ruleSimulatorRisk'),
        reward_pct: dialog.querySelector('#ruleSimulatorReward'),
        win_rate_pct: dialog.querySelector('#ruleSimulatorWinRate'),
        max_trades_per_day: dialog.querySelector('#ruleSimulatorDailyCap'),
    };

    function selectedContext() {
        return {
            exchange_id: getDashboardBinding('activeExchangeFilter') || 'pintu',
            product_id: getDashboardBinding('activeProductFilter') || 'all',
        };
    }

    function parseRule() {
        return parseRuleInputs(fields);
    }

    function parseGrowthProjection() {
        return parseGrowthProjectionInputs(growthFields);
    }

    function responseError(payload, response) {
        if (!response.ok || payload?.error) {
            throw new Error(payload?.error || `Request failed (${response.status})`);
        }
        return payload;
    }

    async function request(url, body) {
        const response = await fetch(url, body ? {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: encodeWireValue(body),
        } : undefined);
        const payload = responseError(await readApiResponse(response), response);
        return payload;
    }

    function setOutput(id, value) {
        dialog.querySelector(`#${id}`).textContent = value;
    }

    function clearGrowthChart(message) {
        dialog.querySelector('#ruleSimulatorGrowthChart').classList.add('hidden');
        dialog.querySelector('#ruleSimulatorChartPlot').replaceChildren();
        setOutput('ruleSimulatorEndingBalance', 'Expected ending balance: unavailable');
        setOutput('ruleSimulatorChartState', message);
        setOutput('ruleSimulatorChartNote', message);
    }

    function formatBalance(value) {
        return formatUsdBalance(value);
    }

    function formatAxisBalance(value, scale) {
        return scale === 'log' && Number.isFinite(value)
            ? USD_AXIS_FORMATTER.format(value)
            : formatBalance(value);
    }

    function formatGrowthHorizon(tradingDays) {
        if (!Number.isInteger(tradingDays) || tradingDays < 1) return 'Unavailable horizon';
        const dayCount = tradingDays.toLocaleString('en-US');
        if (!Number.isInteger(tradingDaysPerYear) || tradingDaysPerYear < 1) {
            return `${dayCount} modeled trading days`;
        }
        const years = tradingDays / tradingDaysPerYear;
        const yearLabel = Number.isInteger(years) ? years.toLocaleString('en-US') : years.toFixed(1);
        return `${yearLabel} years (${dayCount} modeled trading days)`;
    }

    const renderGrowthChart = createRuleSimulatorGrowthChart({
        dialog,
        accountGrowthChartModel,
        formatBalance,
        formatAxisBalance,
        formatGrowthHorizon,
        getTradingDaysPerYear: () => tradingDaysPerYear,
        setOutput,
        clearGrowthChart,
    });

    function clearSimulationOutputs(message) {
        ['ruleSimulatorNetWin', 'ruleSimulatorNetLoss', 'ruleSimulatorNetRr',
            'ruleSimulatorBreakEven', 'ruleSimulatorDailyExpected']
            .forEach(id => setOutput(id, 'Unavailable'));
        setOutput('ruleSimulatorResultState', message);
        clearGrowthChart(message);
    }

    function formatPercent(value, signed = true) {
        if (!Number.isFinite(value)) return 'Unavailable';
        const formatted = `${Math.abs(value).toFixed(2)}% of trade value`;
        return signed && value > 0 ? `+${formatted}` : value < 0 ? `-${formatted}` : formatted;
    }

    function formatDailyPercent(value) {
        if (!Number.isFinite(value)) return 'Unavailable';
        const formatted = `${Math.abs(value).toFixed(2)}% of one trade value`;
        return value > 0 ? `+${formatted}` : value < 0 ? `-${formatted}` : formatted;
    }

    function formatRatio(value) {
        return Number.isFinite(value) ? `${value.toFixed(2)}:1` : 'Unavailable';
    }

    function feeInputs() {
        const buy = buyFeeInput.value.trim() === '' ? null : Number(buyFeeInput.value);
        const sell = sellFeeInput.value.trim() === '' ? null : Number(sellFeeInput.value);
        return {
            buy_fee_pct: Number.isFinite(buy) && Number.isFinite(sell) ? buy : null,
            sell_fee_pct: Number.isFinite(buy) && Number.isFinite(sell) ? sell : null,
        };
    }

    function updateFeeDescription() {
        const source = dialog.querySelector('#ruleSimulatorFeeSource');
        const rulesLink = dialog.querySelector('#ruleSimulatorFeeRulesLink');
        if (!feeContext) {
            source.textContent = 'Fee policy unavailable. Retry loading, or enter both fee rates manually.';
            rulesLink.removeAttribute('href');
            rulesLink.classList.add('hidden');
            return;
        }
        const policy = feeContext.fee_policy;
        const provenance = feeEdited ? 'Manual fee assumption; not verified by the selected venue.' : policy.fee_model;
        const policyState = policy.status === 'known'
            ? 'Selected venue estimate'
            : policy.status === 'unknown'
                ? 'Venue fee unavailable'
                : 'Venue fee policy unsupported';
        const category = feeContext.asset_category_label ? ` · ${feeContext.asset_category_label}` : '';
        source.textContent = `${feeContext.exchange_label}${category} · ${policyState}. ${provenance}`;
        const rulesUrl = policy.official_fee_rules_url;
        if (typeof rulesUrl === 'string' && rulesUrl.startsWith('https://')) {
            rulesLink.href = rulesUrl;
            rulesLink.setAttribute('aria-label', `View ${feeContext.exchange_label} official fee rules in a new tab`);
            rulesLink.classList.remove('hidden');
        } else {
            rulesLink.removeAttribute('href');
            rulesLink.classList.add('hidden');
        }
        const isLongOnly = feeContext.direction_policy !== 'long_and_short';
        const shortOption = dialog.querySelector('#ruleSimulatorDirection option[value="SHORT"]');
        shortOption.disabled = isLongOnly;
        if (isLongOnly && dialog.querySelector('#ruleSimulatorDirection').value === 'SHORT') {
            dialog.querySelector('#ruleSimulatorDirection').value = 'LONG';
        }
        dialog.querySelector('#ruleSimulatorDirectionHelp').textContent = isLongOnly
            ? 'This app product is long only. Fees are applied to the long entry and exit values.'
            : 'Fees are applied to entry and exit values for the selected direction.';
    }

    function updateSaveState(text) {
        dialog.querySelector('#ruleSimulatorSaveState').textContent = text;
    }

    function updateSaveButton() {
        saveButton.textContent = !saved ? 'Save rule' : ruleDirty ? 'Save changes' : 'Update rule';
        saveButton.disabled = !saveAvailable;
    }

    function writeSimulation(simulation, projection, rule, growthSettings) {
        setOutput('ruleSimulatorNetWin', simulation.fee_adjusted
            ? formatPercent(simulation.net_reward_pct)
            : `${formatPercent(simulation.gross_reward_pct)} gross · fee required`);
        setOutput('ruleSimulatorNetLoss', simulation.fee_adjusted
            ? formatPercent(simulation.net_loss_pct)
            : `${formatPercent(simulation.gross_loss_pct)} gross · fee required`);
        setOutput('ruleSimulatorNetRr', formatRatio(simulation.reward_risk_after_fees));
        setOutput('ruleSimulatorBreakEven', Number.isFinite(simulation.break_even_win_rate_pct)
            ? `${simulation.break_even_win_rate_pct.toFixed(2)}%`
            : 'Unavailable');
        setOutput('ruleSimulatorDailyExpected', simulation.fee_adjusted
            ? `${formatDailyPercent(simulation.expected_return_at_daily_cap_pct)} · ${simulation.expected_wins_at_daily_cap.toFixed(2)} expected wins`
            : `${formatDailyPercent(simulation.gross_expected_return_at_daily_cap_pct)} gross · fee required`);
        renderGrowthChart(projection, rule, growthSettings);
    }

    async function simulate() {
        clearTimeout(simulateTimer);
        const revision = ++simulationRevision;
        const rule = parseRule();
        const growthSettings = parseGrowthProjection();
        if (!rule || !growthSettings) {
            const message = !rule ? 'Enter valid R:R rule values to calculate.' : 'Enter valid growth settings to calculate.';
            clearSimulationOutputs(message);
            return;
        }
        const context = selectedContext();
        const fees = feeInputs();
        const body = {
            action: 'simulate',
            ...context,
            rule,
            growth_projection: growthSettings,
            direction: dialog.querySelector('#ruleSimulatorDirection').value,
            ...fees,
        };
        clearSimulationOutputs('Calculating current inputs…');
        try {
            const payload = await request('/api/rule-simulator', body);
            if (revision !== simulationRevision) return;
            feeContext = payload.context;
            updateFeeDescription();
            writeSimulation(payload.simulation, payload.account_growth, rule, growthSettings);
            const feeReady = payload.simulation.fee_adjusted;
            setOutput('ruleSimulatorResultState', feeReady
                ? `Assumption-based · ${payload.simulation.direction} · daily expectancy uses fixed trade value; growth curve compounds current balance`
                : 'Gross-only results shown; enter both fee rates to calculate fee-adjusted results.');
        } catch (error) {
            if (revision !== simulationRevision) return;
            clearSimulationOutputs(`Calculation unavailable: ${error.message}`);
        }
    }

    async function loadRule() {
        clearTimeout(simulateTimer);
        const loadRevision = ++simulationRevision;
        const loadState = dialog.querySelector('#ruleSimulatorLoadState');
        loadState.textContent = 'Loading saved rule and selected app fee policy…';
        retryButton.classList.add('hidden');
        setFormInputsDisabled(true);
        saveAvailable = false;
        updateSaveButton();
        const context = selectedContext();
        const query = new URLSearchParams(context);
        clearSimulationOutputs('Waiting for the saved rule and fee policy…');
        try {
            const payload = await request(`/api/rule-simulator?${query}`);
            if (loadRevision !== simulationRevision) return;
            feeContext = payload.context;
            const ruleLimits = payload.rule_limits;
            fields.risk_pct.min = ruleLimits.min_risk_reward_move_pct;
            fields.risk_pct.max = ruleLimits.max_risk_reward_move_pct;
            fields.reward_pct.min = ruleLimits.min_risk_reward_move_pct;
            fields.reward_pct.max = ruleLimits.max_risk_reward_move_pct;
            fields.win_rate_pct.min = ruleLimits.min_win_rate_pct;
            fields.win_rate_pct.max = ruleLimits.max_win_rate_pct;
            fields.max_trades_per_day.min = ruleLimits.min_trades_per_day;
            fields.max_trades_per_day.max = ruleLimits.max_trades_per_day;
            Object.entries(fields).forEach(([name, input]) => {
                input.value = payload.rule[name];
            });
            const growthDefaults = payload.growth_projection_defaults;
            const growthLimits = payload.growth_projection_limits;
            tradingDaysPerYear = growthLimits.trading_days_per_year;
            growthFields.starting_balance_units.min = growthLimits.min_starting_balance_units;
            growthFields.starting_balance_units.value = growthDefaults.starting_balance_units;
            growthFields.trade_size_pct_of_equity.min = growthLimits.min_trade_size_pct_of_equity;
            growthFields.trade_size_pct_of_equity.max = growthLimits.max_trade_size_pct_of_equity;
            growthFields.trade_size_pct_of_equity.value = growthDefaults.trade_size_pct_of_equity;
            growthFields.trading_days.min = growthLimits.min_trading_days;
            growthFields.trading_days.max = growthLimits.max_trading_days;
            growthFields.trading_days.value = growthDefaults.trading_days;
            dialog.querySelector('#ruleSimulatorHorizonHelp').textContent =
                `Up to ${formatGrowthHorizon(growthLimits.max_trading_days)}; the daily trade cap is assumed to be reached every modeled day.`;
            feeEdited = false;
            buyFeeInput.value = payload.context.fee_policy.buy_fee_pct ?? '';
            sellFeeInput.value = payload.context.fee_policy.sell_fee_pct ?? '';
            saved = Boolean(payload.saved);
            saveAvailable = true;
            setFormInputsDisabled(false);
            ruleDirty = false;
            updateSaveButton();
            loadState.textContent = saved
                ? 'Saved rule loaded from app SQLite.'
                : 'No rule is saved yet. The displayed starting values are an unsaved draft.';
            updateSaveState(saved ? 'Saved in app SQLite.' : 'This draft is not saved.');
            updateFeeDescription();
            await simulate();
        } catch (error) {
            if (loadRevision !== simulationRevision) return;
            feeContext = null;
            saved = false;
            saveAvailable = false;
            ruleDirty = false;
            updateSaveButton();
            retryButton.classList.remove('hidden');
            loadState.textContent = `Could not load saved rule: ${error.message}. Retry before saving.`;
            updateFeeDescription();
            updateSaveState('Save is unavailable until the app database responds.');
            clearSimulationOutputs('No current calculation. Retry loading the rule and fee policy.');
        }
    }

    async function saveRule() {
        if (!saveAvailable) {
            updateSaveState('Save is unavailable until the saved rule and app database load.');
            return;
        }
        const rule = parseRule();
        if (!rule) {
            Object.values(fields).find(input => !input.checkValidity())?.reportValidity();
            updateSaveState('Correct the highlighted values before saving.');
            return;
        }
        updateSaveState('Saving rule to app SQLite…');
        try {
            const payload = await request('/api/rule-simulator', { action: 'save', rule });
            saved = Boolean(payload.saved);
            ruleDirty = false;
            updateSaveButton();
            updateSaveState('Saved in app SQLite.');
            dialog.querySelector('#ruleSimulatorLoadState').textContent = 'Saved rule is ready for fee-adjusted scenarios.';
            await simulate();
        } catch (error) {
            updateSaveState(`Save failed: ${error.message}. Your draft is still here; try Save rule again.`);
        }
    }

    function scheduleSimulation(event) {
        if (Object.values(fields).includes(event.target)) {
            ruleDirty = saved;
            updateSaveButton();
            if (saved) updateSaveState('Unsaved rule changes. Save changes to keep them.');
            else updateSaveState('This draft is not saved.');
        }
        clearTimeout(simulateTimer);
        simulateTimer = setTimeout(() => void simulate(), DEBOUNCE_MS);
    }

    form.addEventListener('input', scheduleSimulation);
    form.addEventListener('submit', event => event.preventDefault());
    buyFeeInput.addEventListener('input', () => {
        feeEdited = true;
        updateFeeDescription();
    });
    sellFeeInput.addEventListener('input', () => {
        feeEdited = true;
        updateFeeDescription();
    });

    function openRuleSimulator() {
        if (!dialog.open) dialog.showModal();
        void loadRule();
    }

    function closeRuleSimulator() {
        dialog.close();
    }

    function retryRuleSimulatorLoad() {
        void loadRule();
    }

    publishDashboardBindings('rule-simulator', {
        openRuleSimulator: { get: () => openRuleSimulator },
        closeRuleSimulator: { get: () => closeRuleSimulator },
        retryRuleSimulatorLoad: { get: () => retryRuleSimulatorLoad },
        saveRuleSimulator: { get: () => saveRule },
        simulateRule: { get: () => simulate },
    });
}
