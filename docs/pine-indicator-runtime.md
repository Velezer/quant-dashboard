# Browser Pine interpreter and strategy runtime

The dashboard builds `quant_pine_runtime` for `wasm32-unknown-unknown` and
loads it in `frontend/dashboard/scripts/workers/pine-worker.js`. The native
service remains native. `tooling/start.bat` and `tooling/harness.bat` build the
WASM asset into Cargo's ignored `target/` directory; the service exposes only
that generated artifact at `/web/wasm/quant_pine_runtime.wasm`.

The editor accepts pasted source or `.pine` files up to 24 KB. `POST
/api/pine/validate` checks source before registration. `POST
/api/pine/indicators` stores indicator source and rights provenance, while
`POST /api/pine/strategies` stores strategy source and the same provenance in
separate tables in the app SQLite database. List responses return at most 100
metadata records; source is fetched only for a selected indicator or run.
`POST /api/pine/backtest` runs a registered strategy against the selected
registered app instrument's bounded SQLite candle history. It returns summary
statistics, an equity point aligned with each loaded candle, up to 200 recent
closed-trade rows, inputs, costs, source quality, and diagnostics. The response
includes structured fill assumptions for slippage ticks, registered instrument
tick size and provenance, and limit-verification ticks. It is transient: it
reports maximum run-up and drawdown from the broker's intrabar OHLC path; the
equity curve remains sampled at each candle close. `max_drawdown_pct` uses
initial capital like TradingView's Strategy Report metric, while
`strategy_max_drawdown_percent` uses the equity-peak denominator of Pine's
`strategy.max_drawdown_percent` built-in. It
does not persist account state, trades, or equity curves. No Pine source or
output is written to browser storage, market history, plans, or execution
evidence.

## Rights and provenance

Register only code you own or have rights to adapt and use. The form reads the
title, author, and license details from recognized source headers and keeps the
provenance fields available for review. Registration still requires required
attribution and an explicit rights confirmation. Source text is stored after
removing chat markup, with its copyright and license notices retained. Public
or protected visibility on TradingView is not treated as permission to reuse
source.

## Current compatibility boundary

This is a bounded interpreter, not a Pine v6 compiler or near-full language
implementation. Both script types accept version 6 assignments, scalar
arithmetic/comparisons, conditional expressions, series history indexing,
indented `if`/`else` and bounded `for` loops, and a selected set of `ta.*`,
`math.*`, `input.*`, `str.*`, and color values. User-defined functions, arrays,
objects, and drawing APIs are unsupported. Unsupported functions, arguments,
or identifiers stop validation or calculation with diagnostics. Input controls
use declared defaults unless a strategy run supplies validated overrides by
input title.

Indicators use the chart runtime and reject `strategy()` source. Strategies use
a separate historical broker model with `strategy.entry()` market and
stop/limit/stop-limit entries, `strategy.cancel()`/`strategy.cancel_all()`,
`strategy.close()`/`strategy.close_all()` market closes, and `strategy.exit()`
brackets using absolute `stop` and/or `limit` prices. `strategy.close()` accepts
partial `qty` or `qty_percent` market closes. `strategy.exit()` also supports
entry-relative `profit` and `loss` tick distances, `qty`, and `qty_percent`;
relative exits require registered instrument tick size. If an absolute and a
relative level are both supplied for one bracket leg, the level expected to be
reached first takes precedence. Reserved quantities reduce later exit capacity,
and one filled bracket leg cancels its sibling. With `from_entry`, exits apply
only to matching entry orders created on or before that call; a later entry with
the same ID does not inherit the old exit unless the script calls it again.
Trailing exits accept `trail_price` or entry-relative `trail_points` together
with `trail_offset`; activation and ratcheting follow the inferred OHLC or
matching Bar Magnifier path, and tick distances require registered instrument
tick metadata. When both activation parameters are present, the earlier
favorable activation level applies.
Partial fills update open size,
realized equity, and the eventual closed-trade row for the entry. It supports
`strategy.closedtrades` accessors for entry/exit bar indexes, prices, net profit,
net percent return, signed size, commission, and entry ID. Size is positive for
long trades and negative for short trades; entry/exit times and Pine's `time`
series use Unix milliseconds although stored candle timestamps use nanoseconds.
`strategy.opentrades` supports entry ID, entry price, entry bar index, entry
time, signed remaining size, commissions, net open profit, and net open return
percent, plus net max run-up, max drawdown, and their percent variants. The excursion values track the
largest favorable and adverse net open P&L observed at the inferred OHLC path
points since the entry, including fees and any realized partial exits. The
matching `strategy.closedtrades.max_runup()`, `.max_drawdown()`,
`.max_runup_percent()`, and `.max_drawdown_percent()` accessors retain those
values on completed trades. Excursion percentages use the original entry
notional. Open return percent uses the original
entry notional, consistent with the closed-trade return basis. Trade accessors expose only the current
transient backtest's open or completed trades. The
strategy namespace also exposes `initial_capital`, `position_entry_name`,
`openprofit`, `openprofit_percent`, `netprofit_percent`, `grossprofit_percent`,
`grossloss_percent`, `max_runup`, `max_runup_percent`, `max_drawdown`, and
`max_drawdown_percent`. Aggregate run-up and drawdown use intrabar per-trade
extremes and the minimum or maximum realized equity from previously closed
trades, following TradingView's historical calculation. Open profit is marked equity minus realized equity;
its percent uses realized equity as the denominator. Net and gross trade
percentages use initial capital. The broker supports fixed, cash-in-chart-currency, or percent-of-equity sizing,
`strategy.commission.percent` or `strategy.commission.cash_per_contract`,
`pyramiding` values from 1 through 100, and
`process_orders_on_close`. Named market closes follow FIFO by default;
`close_entries_rule="ANY"` closes the requested entry ID directly. Market orders
otherwise fill at the next candle open. Price-based exits use matching lower-
timeframe OHLC when the strategy declares `use_bar_magnifier=true` and matching
data is available; otherwise they use chart OHLC. The service currently uses
same-source 15-minute candles for charts above 15 minutes and bounds the
magnifier input at 40,000 candles. TradingView allows up to 200,000 lower-timeframe
bars, so this runtime's available detail is intentionally more limited.
Lower-timeframe bars are applied in timestamp order within each chart candle;
they are accepted only when their aggregate open, high, low, and close match
the chart candle. Uncovered or mismatched chart bars fall back to chart OHLC.
The backtest aligns its bounded 15-minute query to the selected chart sample and
the UI reports aggregate-matched and fallback chart-bar counts. Aggregate OHLC
agreement does not prove every lower-timeframe interval is present, and the
simulator still infers a price path within each lower-timeframe candle. When chart OHLC
is used, price-based exits follow TradingView's inferred historical path:
open-high-low-close when the open is closer to the high, otherwise
open-low-high-close. Gaps fill at the bar open. Multiple price-based entries
that fill at the same inferred path distance can exceed the pyramiding cap, as
TradingView's same-tick exception allows; distinct path distances remain subject
to the configured cap.
Price-entry and bracket events are processed in inferred path order; existing
bracket exits run before a price entry at the same inferred path distance.
Trailing-stop activation, ratcheting, and fills advance only through the next
ordered event, so a later trail fill cannot preempt an earlier intrabar entry.
`strategy.order()` is supported for netting entries and bounded stop/limit
orders. Unfilled `strategy.entry()` and `strategy.order()` orders share an ID
namespace: submitting the same ID modifies the existing pending order, while
already filled orders remain separate trades. Both commands support named OCA groups with
`strategy.oca.none`, `strategy.oca.cancel`, and `strategy.oca.reduce`. Same-
path-point price orders, next-open market orders, and process-on-close fills
execute before OCA cancel/reduce updates, matching TradingView's same-tick
exception. `calc_on_order_fills=true` is supported for historical chart OHLC
market orders: after a market fill, the script recalculates at the next inferred
open/high/low/close point. Each fill-triggered execution rolls ordinary and
`var` series back to the last committed bar state; `varip` values persist
between intrabar executions. The normal bar-close calculation still runs.
This mode rejects price-based `strategy.entry()`/`strategy.order()` calls,
`strategy.exit()` brackets, `use_bar_magnifier`, and
`process_orders_on_close`. The calculation receives the complete historical
bar OHLC on every execution, so logic that reads a later intrabar high, low, or
close can look ahead. Results show this approximation in their UI diagnostics.
Pyramiding above 100, foreign account-currency conversion, margin, and
liquidation are unsupported; unsupported order arguments fail explicitly. The
service uses the app's venue
fee policy when known; when costs are unknown it reports gross trade returns
and labels the basis. An open final position is marked in the returned equity
curve but excluded from closed-trade metrics. Results remain research-only and
are not holdout validated or execution evidence.

The declaration property `calc_on_every_tick` is accepted and surfaced as a
diagnostic, but it applies to realtime bars in TradingView. Historical backtests
here have no realtime tick stream, so it does not change the candle replay.
`calc_on_every_history_tick=true` is rejected because stored OHLC candles cannot
reconstruct the underlying historical ticks; `use_bar_magnifier=true` can use
same-source lower-timeframe candles where available, but those remain OHLC bars,
not tick data.

The backtest UI also reports and plots a same-sample buy-and-hold reference,
normalized to the strategy's initial capital. It uses only the first and last
stored close for its raw price-return metric and applies no fees, quantity
rounding, or execution assumptions. It is a context benchmark, not a competing
strategy result. TradingView distinguishes strategy results from benchmark
buy-and-hold returns in its Strategy Report.

## Fixed cash commission decision

1. **Direct effect:** `strategy.commission.cash_per_contract` deducts its
   declared nonnegative chart-currency amount for each unit filled on entries
   and exits. `strategy.commission.cash_per_order` deducts its amount for each
   simulated fill event, regardless of quantity. Both replace the venue-fee
   estimate and are labeled in the report.
2. **Immediate consequence:** Entry fees reduce marked equity and later
   percent-of-equity sizing; exit fees reduce realized net P&L, trade return,
   and the final equity curve. Gross P&L remains unchanged.
3. **Response:** The UI shows the declared commission model and return basis
   with fill assumptions. Users can correct the source value and rerun.
4. **Interactions:** Both modes use the existing transient broker fill events.
   Cash-per-contract scales with each entry/exit quantity, including partial
   exits; cash-per-order is charged once per fill event, including separately
   allocated exit fills. Explicit foreign `currency` conversion remains
   unsupported; fees are expressed in chart currency under default or
   `currency.NONE` behavior.
5. **Accumulation:** Commission settings and results remain transient per run;
   no financial, account, or execution state is persisted.
6. **Failure boundaries:** Negative/non-finite fees fail validation. The
   runtime treats each modeled fill event as a separate per-order transaction;
   consolidated broker handling of multiple exit allocations is not modeled.
7. **Recovery:** The error identifies an invalid fee value or unsupported
   account currency. Remove the commission declaration to use the visible
   venue-fee/unknown-cost basis.
8. **Governance:** This expands historical research calculation only; it does
   not enable order placement, paper trading, account state, or execution
   evidence.
9. **Trajectory (hypothesis):** Supporting a common fee mode may make reported
   net results more useful for unit-priced contracts. It does not establish
   exchange-specific fee schedules or TradingView parity for other costs.
10. **Decision:** Support percentage, fixed per-contract, and fixed per-order
    commission in the existing broker and UI; retain explicit errors for
    foreign currency conversion until historical FX inputs are modeled.

TradingView reference: [commission types and strategy declaration properties](https://www.tradingview.com/pine-script-docs/language/declaration-statements/#commission_type-and-commission_value).

## Same-sample benchmark decision and effects

1. **Direct effect:** The Strategy Tester view shows a buy-and-hold return and
   equity reference over the exact candles used by the strategy. Acceptance is
   the same sample bounds and initial capital on both lines.
2. **Immediate consequence:** Users can compare active strategy behavior with
   raw continuous exposure to the selected instrument.
3. **Response:** A benchmark gap can prompt users to inspect inputs and rerun the
   backtest on the same or another UI-selected date range.
4. **Interactions:** The benchmark uses the selected app instrument's existing
   candle source and follows the same date and history-cap selection. It is
   displayed beside strategy equity, not included in Pine `strategy.*` values.
5. **Accumulation:** Both series are calculated in memory for the current run;
   no history or benchmark results are persisted.
6. **Failure boundaries:** Missing, nonpositive, nonfinite prices or invalid
   initial capital produce no benchmark result. Price return excludes fees,
   slippage, funding, lot rounding, dividends, and executable fill assumptions.
7. **Recovery:** The UI labels the reference as raw price movement with no costs;
   users can rerun after selecting another stored range or source.
8. **Governance:** This remains research display and creates no account,
   position, order, trade-history, or execution-evidence state.
9. **Trajectory (hypothesis):** A visible baseline may help users identify
   strategies that underperform passive exposure; verify this through UI use
   before expanding benchmark metrics.
10. **Decision:** Keep the comparison ephemeral and limited to the exact
    historical sample, initial-capital normalization, and plainly disclosed
    raw-price basis.

TradingView reference: [Strategy Tester overview and buy-and-hold comparison](https://www.tradingview.com/pine-script-docs/concepts/strategies/#strategy-tester).

## Full-sample trade breakdown decision and effects

1. **Direct effect:** The backtest report shows closed-trade counts, wins,
   losses, breakeven trades, win rate, average winning/losing return, and
   largest winning/losing P&amp;L for all trades, long trades, and short trades.
2. **Immediate consequence:** Direction-specific performance is visible even
   when only the latest 200 trade details are rendered.
3. **Response:** Users can inspect whether a side or outlier trade explains the
   aggregate result, then change source inputs in the UI and rerun.
4. **Interactions:** Counts and averages use every closed trade in the current
   transient output. Profit classification uses net P&amp;L when configured fees
   are known, otherwise gross P&amp;L; winning/losing return uses that same basis.
5. **Accumulation:** The summaries are calculated per request and are not
   stored in SQLite or browser storage.
6. **Failure boundaries:** Open positions are excluded. Missing sides show zero
   counts and unavailable averages; no account-currency or risk-adjusted metric
   is inferred from these price/quantity-unit values.
7. **Recovery:** If the analysis payload is missing, the UI explicitly reports
   that the full-sample breakdown is unavailable while preserving the run's
   other response diagnostics.
8. **Governance:** These remain transient research statistics, not validated
   execution evidence or account/trade-history state.
9. **Trajectory (hypothesis):** Long/short splits can help expose asymmetric
   strategy behavior; compare against fixed source and sample before drawing a
   conclusion.
10. **Decision:** Compute statistics from the full in-memory closed-trade set;
    keep the visible detail table capped at 200 recent rows and disclose the
    difference.

## Candle-close drawdown decision and effects

1. **Direct effect:** The UI adds the close-to-close equity drawdown curve,
   maximum percentage drawdown, peak-to-trough bar duration, recovery status,
   current drawdown duration, and average duration of completed drawdown
   periods. Acceptance uses deterministic equity/time arrays with known peaks,
   troughs, and recovery points.
2. **Immediate consequence:** Users can distinguish a drawdown that recovered
   from one that remained active at the sample end.
3. **Response:** Users can compare the sampled curve with the existing
   intrabar max marked-to-market drawdown and inspect the same selected range.
4. **Interactions:** This metric uses the returned per-candle equity close
   series. The existing broker max drawdown continues to include intrabar OHLC
   excursions and is labeled separately.
5. **Accumulation:** The drawdown series is derived in memory from the current
   backtest output and is not persisted.
6. **Failure boundaries:** Invalid or unaligned equity/time arrays produce an
   unavailable state. The average includes only periods that recovered by the
   sample end. Close-to-close sampling can miss a drawdown that occurs and
   recovers within one candle; the intrabar metric covers OHLC excursions.
7. **Recovery:** A drawdown is recovered only after a candle-close equity value
   reaches its prior peak; otherwise it is labeled unrecovered by sample end.
8. **Governance:** This is transient research output and does not model margin,
   liquidation, or account equity.
9. **Trajectory (hypothesis):** Pairing close-based duration with intrabar
   maximum drawdown may make risk interpretation clearer; revisit if users
   confuse the two sampling bases.
10. **Decision:** Preserve both measures and disclose their sampling basis in
    the UI instead of merging close and intrabar extrema.

## Full-sample trade-return distribution decision and effects

1. **Direct effect:** The backtest UI plots a histogram of every finite closed-
   trade return in the selected sample. Acceptance verifies all bins sum to the
   full finite-return count and use the selected gross or net basis.
2. **Immediate consequence:** Users can see outcome concentration and tail
   trades without relying on the capped recent-trade detail table.
3. **Response:** Users can inspect the full-sample distribution alongside the
   long/short statistics and adjust strategy inputs in the UI before rerunning.
4. **Interactions:** Returns come from the runtime's closed-trade outputs; net
   returns are selected when configured fees are known, otherwise gross
   returns are selected. Equal-width bins use a square-root count heuristic
   bounded to 1 through 12 bins.
5. **Accumulation:** Histogram bins are calculated in memory per request and
   are not persisted.
6. **Failure boundaries:** No finite returns, invalid bin payloads, or an
   unrepresentable numeric return span show an unavailable state. The histogram
   groups equal-width ranges and can hide the exact values of individual trades.
7. **Recovery:** Inspect the full-sample side summary and capped recent trades;
   rerun with a different source input or date range to see the updated sample.
8. **Governance:** The chart is transient research output and does not create
   account, order, position, or execution-evidence state.
9. **Trajectory (hypothesis):** Showing the tail distribution may reveal
   concentration hidden by mean returns; validate that interpretation against
   trade details before changing strategy decisions.
10. **Decision:** Keep the histogram tied to all closed returns, disclose the
    binning rule and cost basis, and never treat the plotted distribution as a
    probability forecast.

## Full-sample trade-excursion decision and effects

1. **Direct effect:** Add all/long/short average maximum favorable and adverse
   excursion amounts and percentages to the full-sample trade summary, and add
   each recent closed trade's excursion values to its detail row.
2. **Immediate consequence:** Users can inspect how far simulated trades moved
   in their favor or against them before exit, even when the closed result was
   small or opposite in sign.
3. **Response:** Users can inspect each recent trade beside its entry, exit,
   P&L, and return, then compare the full-sample side summaries. Excursions
   remain descriptive statistics, not stop or target recommendations.
4. **Interactions:** Reuse each runtime `StrategyTrade` excursion. Amounts are
   simulator price and quantity units; excursion percentages use entry
   notional, with adverse percentages retaining their negative sign. The
   all/long/short groups and sample align with the existing closed-trade
   analysis.
5. **Accumulation:** Summaries are calculated from transient backtest output
   and are not persisted.
6. **Failure boundaries:** Non-finite excursion values are omitted and their
   included count is returned. The UI shows unavailable values for legacy or
   partial responses. Intrabar path assumptions affect the excursions.
7. **Recovery:** Rerun with a different date range or declared input to refresh
   the sample; the existing simulator fill-assumption disclosure remains next
   to the result.
8. **Governance:** These values create no account, position, order, paper-trade,
   or execution-evidence state.
9. **Trajectory (hypothesis):** Excursion summaries may help identify exits
   that surrender large favorable moves or tolerate large adverse moves; this
   does not establish that a revised exit would improve out-of-sample returns.
10. **Decision:** Expose only full-sample descriptive excursion summaries in
    the existing UI and include per-trade values in the existing capped detail
    table. Retain each simulator amount and percentage's units.

## Monthly marked-equity decision and effects

1. **Direct effect:** The backtest details show UTC calendar-month marked
   strategy equity change and return beside same-period underlying price return.
2. **Immediate consequence:** Users can see whether overall sample performance
   was concentrated in a few months or changed direction over time.
3. **Response:** Users can compare monthly sequence with the full equity curve,
   drawdown, and trades, then rerun another range or input from the UI.
4. **Interactions:** Use the existing candle-close equity and same-sample
   buy-and-hold series. Each month closes at its last sampled candle; its base
   is the prior observed equity, except the first sampled month uses initial
   capital. Open-position mark-to-market is included.
5. **Accumulation:** Buckets are derived in the browser from the bounded
   response and never persisted. The sample permits fewer than twelve months;
   first and last calendar buckets may be partial.
6. **Failure boundaries:** Missing, unordered, non-finite equity data shows an
   unavailable state. Monthly percentage is unavailable when its base equity
   is nonpositive, but the simulator-unit equity change remains visible. Missing
   benchmark points remain unavailable rather than being inferred.
7. **Recovery:** Rerun the strategy on a valid stored history range; the normal
   backtest status reports the actual sample dates, source, and truncation.
8. **Governance:** Results are transient research output, not account returns,
   persistent performance, or execution evidence.
9. **Trajectory (hypothesis):** Monthly sequence may expose sample dependence
   hidden by aggregate return; it is not out-of-sample validation and does not
   establish an edge.
10. **Decision:** Add a read-only monthly view derived from existing bounded
    equity points, retaining the UI's source, cost, benchmark, and research-only
    disclosures.

## Historical fill-recalculation decision and effects

1. **Direct effect:** `calc_on_order_fills=true` reruns the selected Pine
   strategy after eligible market fills at the next inferred chart OHLC point.
   Acceptance requires same-bar follow-up market orders to fill at the expected
   point, with matching `var` rollback and `varip` persistence.
2. **Immediate consequence:** A strategy can create a market order during a
   fill-triggered calculation and have it fill at the next inferred point in
   the same historical candle.
3. **Response:** Multiple market fills can cause multiple executions on one
   candle. They share the existing cumulative strategy evaluation-step budget.
4. **Interactions:** This mode excludes price-based entry/order calls,
   `strategy.exit()` brackets, Bar Magnifier, and `process_orders_on_close`;
   those interactions use different fill paths and are rejected explicitly.
5. **Accumulation:** Each run remains transient and uses the selected bounded
   candle history. No trade, account, or order state is persisted.
6. **Failure boundary:** Every historical execution can read the full candle
   OHLC even when its modeled fill point is earlier. That can create lookahead
   bias, so results disclose this limitation in the backtest UI.
7. **Recovery:** Unsupported combinations return a visible diagnostic; remove
   the incompatible source property or order call and run the strategy again.
8. **Governance:** The UI reports informational research results only. The
   feature does not add paper trading or order placement.
9. **Trajectory (hypothesis):** More fill types may improve TradingView
   similarity, but should be added only when their event ordering can be
   validated against primary TradingView references and deterministic cases.
10. **Decision:** Model only next inferred OHLC-point market fills in this mode
    and reject unverified combinations. Reconsider if deterministic comparison
    against TradingView shows a difference in fill timing, rollback, or
    `varip` behavior.

`strategy.slippage` is modeled as an adverse price adjustment by the configured
number of ticks on market and stop fills; limit fills remain at their expected
price. The selected registered instrument must have a positive `price_tick` in
its contract metadata. Nonzero slippage is rejected when that metadata is
missing; the runtime does not infer tick size from candle precision. This is a
broker fill-cost approximation and does not model spread, market impact, queue
position, or venue liquidity. If the configured adverse adjustment would create
a non-finite or nonpositive simulated fill price, the runtime rejects the run
so it cannot return incomplete trade accounting.

TradingView reference: [strategy slippage and limit-fill behavior](https://www.tradingview.com/pine-script-docs/language/declaration-statements/#slippage-and-backtest_fill_limits_assumption).

`strategy.exit()` relative `profit` and `loss` values are whole, nonnegative
tick distances from each matching entry fill. The selected registered
instrument must supply a positive tick size. When a call supplies both a
relative and absolute price for the same bracket leg, the closer level in the
trade direction is selected, matching Pine v6 behavior.

TradingView reference: [strategy.exit relative and absolute levels](https://www.tradingview.com/pine-script-docs/faq/strategies/).

`backtest_fill_limits_assumption` is supported from 0 through 10,000 ticks.
When nonzero, price must move the configured number of ticks beyond an entry or
exit limit in the favorable direction. The order fills at its requested limit
after intrabar verification, while a favorable gap at the next available price
may fill at that better price. Nonzero verification requires the same registered
positive instrument tick size; candle precision is never used as a substitute.

## Strategy slippage decision and effects

1. **Direct effect:** Market and stop fills move adversely by the declared
   `slippage` ticks; limit fills remain at their modeled price.
2. **Immediate consequence:** Entry/exit prices, transaction fees, realized
   profit, and trade returns all use the adjusted fill prices.
3. **Response:** Users can include a fixed cost assumption in a Pine strategy
   and compare the resulting trades with a zero-slippage run.
4. **Interactions:** Tick size comes from the selected registered instrument's
   contract metadata. Candle precision and research-proxy labels do not supply
   a tick-size fallback.
5. **Accumulation:** Slippage affects only the transient backtest calculation;
   it creates no persisted market or trade state.
6. **Failure boundaries:** Nonzero slippage without a finite positive registered
   tick size is rejected. Limit and stop-limit fills do not receive this
   market/stop adjustment.
7. **Recovery:** Results state the configured cost model and carry the usual
   fill approximation warning; selecting an instrument with registered tick
   metadata or using zero slippage allows a rerun.
8. **Governance:** The change affects informational research only; it does not
   place or authorize paper or live orders.
9. **Trajectory (hypothesis):** Explicit tick costs may reduce overstatement
   from idealized market/stop fills; fixed ticks still cannot represent dynamic
   spread, liquidity, or market impact. Fixed-history comparison can test the
   cost effect.
10. **Decision:** Use the instrument registry's price increment and reject
    nonzero slippage without it. Reconsider if a verified TradingView comparison
    shows the selected chart instrument's tick metadata is not the applicable
    Pine `syminfo.mintick`.

## Limit verification decision and effects

1. **Direct effect:** A limit order needs the configured favorable price
   excursion before the broker marks it filled; zero retains touch-based fills.
2. **Immediate consequence:** Limit order timing and whether a position opens or
   closes can change, while verified fills retain the requested limit price.
3. **Response:** Users can test stricter limit assumptions to reduce guaranteed
   touch fills and compare against the default zero-tick behavior.
4. **Interactions:** Entry limits, stop-limit entries after activation, and
   bracket limit legs share the same threshold. Stop orders and market orders
   remain outside this rule; market/stop slippage remains a separate setting.
5. **Accumulation:** The assumption affects only transient backtest execution;
   it adds no order or fill persistence.
6. **Failure boundaries:** Nonzero verification needs registered finite positive
   `price_tick` metadata. Missing metadata rejects the run; gaps and OHLC path
   resolution remain bounded approximations.
7. **Recovery:** The output diagnostic states the threshold and tick size; set
   the parameter to zero or select an instrument with valid metadata to compare.
8. **Governance:** The behavior remains historical research and does not enable
   order placement or execution evidence.
9. **Trajectory (hypothesis):** Verification may reduce optimistic limit
   fills, but it cannot establish available queue liquidity. Fixed-history
   comparison with TradingView can reveal remaining order-time differences.
10. **Decision:** Use the existing instrument registry tick size and OHLC path
    to enforce the threshold without changing the storage or broker boundaries.

Profit factor uses gross winning and losing trade P&L. It is unbounded when
gross losses are zero and gross profits are positive, and undefined when both
are zero; the result uses an explicit unbounded flag instead of a finite
sentinel value.

`request.security()` supports confirmed higher or same timeframe series for
the selected app instrument when stored 15m, 1h, 4h, and 1d candles are
available. Cross-symbol, lower-timeframe, nested, and lookahead-on requests
stop with diagnostics. Higher-timeframe alignment uses bar-close timestamps;
session gaps and historical/realtime edge cases may differ from TradingView.
`barstate.isrealtime` is false for stored history.
Calculations use only the selected app's stored candles, retain the app,
provider, and source context in the UI, and remain informational. No indicator
output enters plan, signal, or execution-evidence paths.

One source-specific compatibility profile recognizes the supplied LuxAlgo
Nadaraya-Watson Envelope Pine v5 reference. It runs the existing bounded Rust
NWE calculation with the source's declared defaults for bandwidth, multiplier,
close input, and repaint mode; generic Pine v5 execution remains unsupported.
This profile reduces drawing objects to bounded chart lines and triangle
markers, and omits the source's 1x1 table and exact drawing styling. The chart
turns off its duplicate built-in NWE overlay when this source is selected for
registration/use. The profile retains the CC BY-NC-SA attribution and reports
its noncommercial and share-alike conditions.

A second source-specific profile recognizes the supplied HPotter DSS Bressert
Pine v4 reference. Its saved source remains unchanged and retains the HPotter
copyright and TradingView publication link. TradingView Terms of Use section
22 says a published script without a license notice in its comments is licensed
under MPL 2.0; the source page contains no separate license notice. The
registration records that basis and the MPL 2.0 license link. The profile uses
only the source's declared defaults (10/9/5 lengths, 80/20 levels, and 60-minute
resolution), and reads approved stored 1h candles. It aligns confirmed values
to 15m charts and samples the last available 1h value at the close of each 4h
or 1d chart bar, matching historical `security()` with `lookahead_off`. Missing
1h history and changed executable logic stop evaluation. Its DSS and Trigger
lines appear in the existing 0–100 momentum pane while selected; session gaps,
exact EMA warm-up edges, and TradingView realtime behavior may differ.

Known approximations include default-only input controls, separate-pane plots
for generic sources not being drawn, shape/character options reduced to
close-price markers, plot style/offset/transparency options not preserved, and
recursive-average warm-up edges that can differ from TradingView. The chart
labels the selected app and stored history context. The UI explicitly reports
that realtime is unavailable until an approved live feed is connected.

Resource limits are enforced in the runtime: 24 KB source, 4,096 lexer tokens,
256 top-level statements, 1,000 indicator candles, up to 20,000 strategy
candles per selected timeframe, 100,000 retained series values and 1 MiB of
retained strings per evaluator, 512 bytes per retained string value, two
distinct `request.security()` history caches, 16 plots, 10,000 history offset,
2,000,000 indicator expression steps, and 20,000,000 strategy steps. These
limits bound browser worker and service execution. Strategy runs expose recent
history at the indicator limit and a deep-history option at the strategy cap;
only bars returned by SQLite are evaluated, so the cap does not guarantee a
full sample of that size.

## Strategy backtest decision and effects

The following records the bounded strategy-engine decision and its material
effects through the project's ten-order review:

1. **Direct effect:** Registered supported `strategy()` scripts can run on
   selected app history. Acceptance is a validated source, chronological
   bounded candles, and an explicitly labeled summary response.
2. **Immediate consequence:** Strategy registration and backtesting use their
   own SQLite table and API routes; indicator output continues through its
   indicator runtime.
3. **Response:** Users can reuse registered sources and override typed Pine
   inputs by title; an invalid override or unsupported operation returns a
   diagnostic instead of a partial result.
4. **Interactions:** The selected exchange, category, product, ticker, source,
   freshness, and timeframe remain attached. Unknown costs are visible as
   gross-only results; proxy history is never described as venue execution.
5. **Accumulation:** Only source rights metadata persists in the app database.
   Backtest trades and equity curves remain transient, so repeated runs do not
   create financial history or additional storage growth.
6. **Failure boundaries:** Malformed OHLC, unordered timestamps, unavailable
   history, stale data, non-finite fees, unsupported broker properties, and
   over-budget strategies fail explicitly or retain a visible stale label.
7. **Recovery:** Users can correct source/inputs and rerun against the same
   registered instrument; diagnostics identify unsupported syntax and the
   summary resets when chart context changes.
8. **Governance:** The engine is read-only and has no account, order placement,
   paper-trading, or execution-evidence path. With explicit user approval on
   2026-10-12, this route may transiently return a bounded simulated strategy
   equity curve and up to 200 recent closed trades. It stores none of them and
   does not create account, position, or order state. Registration still
   requires rights confirmation and provenance.
9. **Trajectory (hypothesis):** Supporting more Pine syntax may broaden
   research workflows, but compatibility and predictive quality are not
   implied. Revisit as actual registered strategies expose concrete gaps.
10. **Decision:** Keep the interpreter and broker in the existing Pine runtime,
    app registry, service route, and dashboard. Reconsider if users see results
    presented as TradingView-equivalent, if source context is lost, or if a
    supported order feature is silently ignored.

## Deep-history strategy window decision and effects

1. **Direct effect:** Strategy runs can request a recent sample or select an
   optional inclusive UTC calendar-date range, still bounded by the runtime's
   declared strategy candle cap. The response reports requested dates and the
   actual first/last stored candle timestamps.
2. **Immediate consequence:** SQLite applies time bounds before the candle cap,
   so an older window is reachable without loading the latest history first.
   Derived 2h/4h candles use the matching bounded source rows. The evaluator
   continues to enforce its candle and expression-step budgets.
3. **Response:** Users can rerun the same strategy on a date window; changing a
   date, depth, or chart context clears the prior summary. Leaving both dates
   empty preserves the latest-bar-count behavior.
4. **Interactions:** Strategy context and Bar Magnifier inputs use the same
   requested UTC range, selected app, and source. The actual sample period and
   source/stale labels remain visible. A bar cap can truncate a wider request.
5. **Accumulation:** Results stay transient; no history cache or backtest record
   is added. Canonical chart history remains in SQLite `bars`.
6. **Failure boundaries:** A partial date range, reversed range, or timestamp
   overflow is rejected. A valid range with no stored candles returns an
   explicit unavailable result; a wider request cannot recover data the
   provider never stored.
7. **Recovery:** After history or runtime recovery, users can rerun. Changing
   dates or chart context clears old results, and the returned actual sample
   dates reveal truncation or gaps.
8. **Governance:** Date selection remains read-only research and creates no
   account, position, order, paper-trading, or execution-evidence state.
9. **Trajectory (hypothesis):** More precise sample selection can reveal
   warm-up sensitivity, but neither a long sample nor date-range parity proves
   predictive quality. The UI job exposes progress and cancellation for longer
   runs; collect service memory and evaluation-time evidence before widening the
   cap again.
10. **Decision:** Keep date bounds in the existing backtest API and range-aware
    SQLite loaders. Raise the Pine cap to 20,000 candles with 100,000 retained
    series values and 1 MiB retained string bytes per evaluator, at most two
    distinct security caches, one active in-process job, bounded result storage,
    and the existing evaluation-step budget. Reconsider these limits only with
    measured service memory and evaluation-time evidence.

## Strategy timeframe aggregation decision and effects

1. **Direct effect:** Deep strategy requests can roll up up to 20,000 2h/4h
   bars from as many as 40,120/80,120 canonical hourly bars. Existing chart and
   research aggregation stays at its 12,000 source-row cap.
2. **Immediate consequence:** The extra source rows are read transiently from
   the selected SQLite symbol history and released after aggregation; no new
   cache or persisted dataset is introduced.
3. **Response:** Strategy runs may return more of the requested sample when
   hourly history is available. The response continues to report actual bars,
   so missing or short history remains explicit.
4. **Interactions:** Selection still follows the same registered instrument,
   timeframe, source, and provenance labels. Direct higher-timeframe history is
   merged with derived history using the existing freshness rule.
5. **Accumulation:** Repeated runs reread canonical bars and do not accumulate
   stored data. An 80,120-row transient source batch is approximately a few
   megabytes, with actual request memory depending on other live buffers.
6. **Failure boundaries:** The source cap remains finite; unavailable or
   malformed stored history retains existing loader behavior. A larger cap
   cannot recover hourly rows that were never stored.
7. **Recovery:** If service memory or latency regresses, lower the strategy-only
   cap and rely on the returned actual bar count; chart aggregation is unaffected.
8. **Governance:** This changes only read-only research coverage and does not
   create account, order, position, paper-trading, or execution-evidence state.
9. **Trajectory (hypothesis):** Broader 2h/4h samples may improve warm-up and
   change results; this does not establish TradingView parity or predictive
   quality. Measure request memory and latency before considering wider use.
10. **Decision:** Keep a separate bounded strategy source cap in the existing
    candle-history module. Reconsider if measurements threaten the 16 MB
    steady-state target or if source selection/provenance differs from chart
    history.

## Bounded strategy pyramiding decision and effects

1. **Direct effect:** `pyramiding` permits a bounded number of same-direction
   `strategy.entry()` trades, up to 100 open entries in this runtime.
2. **Immediate consequence:** Open entries remain distinct so their entry bars,
   average position price, closed-trade results, and open-trade count can be
   reported consistently.
3. **Response:** Further same-direction market entries are ignored once the
   configured count is reached; an opposite-direction entry closes the current
   position before opening the new direction.
4. **Interactions:** `strategy.close(id)` sizes the market close from the
   matching entry ID, then closes FIFO from the oldest open trade by default.
   `close_entries_rule="ANY"` closes matching entries first.
   `strategy.close_all()` closes the full position. Brackets continue to match
   their `from_entry` IDs.
5. **Accumulation:** Memory use grows with open entries but is bounded by the
   configured ceiling. Each completed entry emits its own closed-trade record.
6. **Failure boundaries:** Zero, fractional, negative, or over-limit
   pyramiding values fail explicitly. `close_entries_rule` accepts only `FIFO`
   or `ANY`. `strategy.order()` bypasses the pyramiding cap and has a separate
   netting path; partial exits are supported by quantity.
7. **Recovery:** Reduce pyramiding to the supported bound or use one open entry;
   unsupported order paths continue to report diagnostics.
8. **Governance:** This remains an in-memory historical broker model and creates
   no account, position, order, or persisted trade-history state in the app.
9. **Trajectory (hypothesis):** More entries can materially change exposure,
   fees, and drawdown; results remain approximate and require strategy-specific
   review.
10. **Decision:** Keep a finite cap in the existing broker. Revisit only if
    verified TradingView comparisons show a need for more entries or finer
    per-order exit behavior.

## Market close sizing decision and effects

1. **Direct effect:** `strategy.close(id)` can close the matching entry in full
   or submit a partial market close with `qty` or `qty_percent`. If both are
   supplied, `qty` takes precedence. Acceptance checks remaining size, realized
   profit, and the final aggregated trade row.
2. **Immediate consequence:** The market close follows the configured order
   delay or `immediately` setting. It closes no more than the available
   position quantity.
3. **Response:** Strategies can scale out at a script condition without
   expressing a price bracket.
4. **Interactions:** Partial close quantities are allocated using the strategy
   declaration's `close_entries_rule`: FIFO by default or matching-entry-first
   under `ANY`. `strategy.close_all()` still closes the full net position.
5. **Accumulation:** Each partial fill updates realized equity and is folded
   into the eventual closed-trade row for its entry.
6. **Failure boundaries:** `qty` must be a finite positive number; `qty_percent`
   must be greater than zero and at most 100. Tick-level execution remains
   unavailable, so market closes fill at the next modeled tick.
7. **Recovery:** Compare expected size and exit price against the returned
   closed-trade row; the runtime reports chart-OHLC approximation limits.
8. **Governance:** This changes only transient historical simulation and does
   not create paper or live trading capability.
9. **Trajectory (hypothesis):** Partial market closes can change trade duration,
   costs, and reported returns; fixed reference cases can reveal parity gaps.
10. **Decision:** Keep sizing in the existing transient broker and preserve
    `strategy.close_all()` as an unambiguous full-position close.

TradingView reference: [strategy.close() market closes](https://www.tradingview.com/pine-script-docs/concepts/strategies/#strategyclose-and-strategyclose_all).

## Chart-currency cash sizing decision and effects

1. **Direct effect:** `default_qty_type=strategy.cash` converts the declared
   cash amount to instrument units using the signal bar's close price.
2. **Immediate consequence:** The next-open entry quantity is fixed when the
   order is created, matching the existing fixed and percent sizing lifecycle.
3. **Response:** Explicit `strategy.entry(qty=...)` remains higher priority than
   the default sizing mode.
4. **Interactions:** The mode uses `currency.NONE` (the default chart quote
   currency); explicit account currencies that require FX conversion are
   rejected rather than mixed with quote-denominated prices and equity.
5. **Accumulation:** Position size and equity remain in chart-quoted units, with
   no currency conversion cache or persisted configuration.
6. **Failure boundaries:** Nonpositive/nonfinite cash amounts and non-default
   currency declarations fail validation. Fractional quantities remain because
   no instrument lot-size policy is supplied to this runtime.
7. **Recovery:** Remove the unsupported currency declaration or use one of the
   supported fixed or percent-of-equity sizing modes.
8. **Governance:** This affects only the informational backtest and does not
   add account, order, position, or paper-trading state.
9. **Trajectory (hypothesis):** Cash sizing can make more published Pine
   strategies executable, but exact parity still depends on currency and
   instrument contract rules.
10. **Decision:** Support the same-chart-currency path in the existing broker;
    revisit foreign account currencies only when validated historical FX
    conversion evidence is available.

## Pine percentage commission decision and effects

1. **Direct effect:** Strategies may declare `commission_type=strategy.commission.percent`
   and a finite nonnegative `commission_value`; each filled side is charged
   against its transaction notional.
2. **Immediate consequence:** Explicit Pine commission replaces the app's
   venue-fee estimate so the same transaction is not charged twice.
3. **Response:** The result's fee model identifies the Pine override and uses
   after-commission returns. Strategies without an override retain the app's
   venue-fee policy.
4. **Interactions:** The runtime owns Pine declaration parsing; service response
   metadata consumes the returned override instead of reinterpreting source.
5. **Accumulation:** Commission affects net P&L, equity, win/loss classification,
   and percent-of-equity sizing. It adds no persisted state.
6. **Failure boundaries:** Negative/non-finite values fail explicitly.
   Cash-per-contract and cash-per-order commission are covered by the newer
   fixed-commission decision below. Foreign account-currency conversion remains
   unsupported.
7. **Recovery:** Users can remove the override to return to app venue fees or
   select a supported percentage value.
8. **Governance:** This remains historical research and does not assert actual
   venue fees or authorize orders.
9. **Trajectory (hypothesis):** Source-declared percentage costs make replay
   behavior closer to Pine settings, though broker and data parity still differ.
10. **Decision:** Support the notional-based percent mode in the existing broker.
    Use the fixed-per-contract decision below for its later bounded extension.

## Price-based entry and cancellation decision and effects

1. **Direct effect:** `strategy.entry()` accepts absolute `stop`, `limit`, or
   combined stop-limit prices. `strategy.cancel(id)` removes unfilled orders
   with that ID; `strategy.cancel_all()` removes all unfilled orders. Acceptance
   is evidenced by exact stop/limit OHLC fills or no fill after cancellation.
2. **Immediate consequence:** Price orders survive across candles and normally
   fill from the next available tick. With `process_orders_on_close`, a newly
   created order can fill only when the closing price satisfies it; the broker
   does not reuse earlier high/low movement from before order submission.
   Marketable limits can receive the opening price. Stop-limit orders activate
   their limit only after the stop triggers; if that limit is already
   marketable, the order fills at the activation price, otherwise it waits for
   a later favorable move to the limit.
3. **Response:** Strategies may leave entries pending, cancel them by ID, or
   cancel all pending orders before later bars reach their prices.
4. **Interactions:** Orders stay inside the existing broker and use the
   selected app instrument's candles, fee rules, and quantity model.
   `strategy.entry()` observes the pyramiding cap; `strategy.order()` nets
   transaction quantity against the position and bypasses that cap.
   Cancellation runs during script evaluation after that bar's
   opening fills, matching the historical script/broker sequence. Close-tick
   processing also checks newly created bracket exits against the close only.
5. **Accumulation:** Pending orders live only for the backtest run; they add no
   database tables, browser storage, or persistent account/order state.
6. **Failure boundaries:** Invalid prices and unsupported order arguments fail
   explicitly. Price orders reached at the same inferred path point, next-open
   market orders, and process-on-close fills execute before OCA cancel/reduce
   updates. Live tick data remains unavailable. Historical
   `calc_on_order_fills` is supported only for next-point market orders, using
   inferred chart OHLC and the limitations described above.
7. **Recovery:** Results carry an approximation diagnostic. Users can change
   order levels or cancellation conditions and rerun on the same bounded
   candle history to compare outcomes.
8. **Governance:** The change expands only historical research behavior. It
   does not enable paper or live order placement or persistent account state.
9. **Trajectory (hypothesis):** Pending entries cover more breakout and
   pullback strategies; broader fidelity still depends on tick and lower-time
   frame evidence.
10. **Decision:** Keep the feature in the transient Pine broker and explicitly
    disclose its OHLC-path and intrabar-resolution limits. Compare fixed examples
    with TradingView when investigating future divergence.

TradingView references: [limit and stop-limit behavior](https://www.tradingview.com/pine-script-docs/concepts/strategies/#order-types), [order cancellation](https://www.tradingview.com/pine-script-docs/concepts/strategies/#strategycancel-and-strategycancel_all), and [pyramiding exceptions](https://www.tradingview.com/pine-script-docs/concepts/strategies/#pyramiding).

## Net order decision and effects

1. **Direct effect:** `strategy.order()` submits a market or stop/limit/stop-limit
   transaction that nets against the open position. Acceptance checks that an
   opposite order first reduces exposure and opens a position only for any
   excess quantity; the `pyramiding` setting does not cap this call.
2. **Immediate consequence:** A netting fill closes matching exposure at the
   broker fill price, realizes fees and P&L, and creates a new entry row only
   for quantity crossing through flat.
3. **Response:** Scripts can build or reduce exposure with explicit transaction
   quantities while `strategy.entry()` keeps its separate auto-reversal rules.
4. **Interactions:** OCA cancel removes unfilled siblings; OCA reduce subtracts
   filled quantity from group siblings. Order IDs remain available to
    `strategy.cancel()` and `strategy.cancel_all()`. New entry/net submissions
    with an existing unfilled ID replace that pending order across
    `strategy.entry()` and `strategy.order()`. Fill candidates are
   processed by distance along TradingView's inferred OHLC path.
5. **Accumulation:** Orders and groups are transient within a backtest. They do
   not create persisted account, position, order, or trade-history state.
6. **Failure boundaries:** Empty OCA names for cancel/reduce are rejected.
   An OCA group is identified by both its name and type, so same-name orders
   with different types act independently. Orders reached at the same inferred
   path point resolve by submission order, and next-open or process-on-close
   orders reached together fill before OCA updates. Tick fills and
   lower-timeframe execution remain unsupported. Chart OHLC can
   still change same-bar ordering.
7. **Recovery:** Read the runtime approximation warning and compare a fixed
   TradingView example when group ordering affects the result.
8. **Governance:** This is historical research behavior only and cannot place,
   persist, or qualify real or paper orders.
9. **Trajectory (hypothesis):** Net orders expand strategy coverage, while OCA
   and OHLC assumptions can materially change exposure and fills.
10. **Decision:** Keep netting and OCA behavior inside the existing transient
    broker. Expand fidelity only when example comparisons identify a specific
    mismatch.

TradingView reference: [strategy.order and OCA behavior](https://www.tradingview.com/pine-script-docs/concepts/strategies/#strategyorder).

## Price-based exit decision and effects

1. **Direct effect:** A supported `strategy.exit()` can create full or partial
   absolute or entry-relative stop, limit, bracket, or trailing exits by
   `qty` or `qty_percent`. Acceptance includes expected trail activation,
   ratcheting, stop execution, and closed-trade accounting for long and short
   positions.
2. **Immediate consequence:** Exit quantities reserve available entry size in
   call order. A fill reduces remaining exposure and realized equity; the
   sibling leg for that exit ID is canceled. Repeated source calls can update
   the same exit ID on later calculations.
3. **Response:** Strategies can express staged exits while position size,
   average entry price, realized profit, and marked equity reflect partial
   fills.
4. **Interactions:** Multiple entries with the same ID are matched by
   `from_entry`; fees are allocated to each fill and the eventual closed-trade
   metrics aggregate by entry. Trailing state is independent per entry and
   follows chart OHLC or accepted Bar Magnifier bars in path order.
5. **Accumulation:** Exit orders remain transient broker state; no simulated
   account, order, position, or trade history is persisted.
6. **Failure boundaries:** Invalid prices and unsupported arguments fail.
   A trail requires `trail_offset` plus `trail_price` or `trail_points`, and
   tick distances require registered tick metadata. Tick-by-tick execution
   remains unsupported. OHLC path and
   Bar Magnifier coverage can change activation and fill sequence.
7. **Recovery:** Runtime results carry the OHLC approximation warning. Review
   source quantity assumptions and rerun against the same candle history.
8. **Governance:** This remains historical research only and cannot place,
   persist, or qualify real or paper orders.
9. **Trajectory (hypothesis):** Staged exits improve strategy coverage, but
   candle-only fills and the bounded broker do not establish full TradingView
   parity. Differential comparison on fixed histories can identify remaining
   semantic gaps.
10. **Decision:** Keep trailing state inside the existing transient broker and
    use the established inferred price path. A TradingView comparison that
    diverges on activation, ratcheting, or stop fill should trigger correction.

TradingView reference: [trailing stop orders](https://www.tradingview.com/pine-script-docs/concepts/strategies/#trailing-stops).

## Deep backtest entry and uncertainty decision

1. **Direct effect:** The backtest panel provides a direct path to register a
   strategy from the existing Pine source editor and adds 95% Wilson intervals
   to chronological training and validation-tail win rates.
2. **Immediate consequence:** Empty registry state points to the editor, while
   registered source still passes the existing parser, runtime, rights, and
   provenance checks before it can be saved.
3. **Response:** Users can vary declared inputs, history, date range, and split
   through the UI, but repeated tuning on the validation tail contaminates it.
4. **Interactions:** The interval is calculated from closed trades using the
   same fee-aware winner classification as the displayed sample. Monthly
   marked returns use each run's own transient candle-aligned equity; validation
   does not inherit training positions or equity.
5. **Accumulation:** No result, uncertainty interval, or browser cache is
   persisted; the UI reports sample counts alongside monthly stability.
6. **Failure boundaries:** Empty, malformed, short, or unavailable samples do
   not manufacture intervals or stability comparisons. A Wilson interval
   assumes independent Bernoulli outcomes, while serially dependent trades can
   make it too narrow.
7. **Recovery:** Retry refreshes the strategy list; unsupported source, stale
   history, and runtime errors remain visible with their existing correction
   or data-recovery paths.
8. **Governance:** Registration retains the separate Pine strategy registry,
   rights confirmation, SQLite-only source storage, and research-only boundary.
9. **Trajectory (hypothesis):** Explicit uncertainty and easy strategy setup
   may improve research interpretation; fresh UI use and independent
   TradingView comparisons would be needed to support that hypothesis.
10. **Decision:** Keep registration in the established Pine editor, compute
    Wilson intervals transiently, and label every inference. If paired
    strategies with few trades look overly certain, withdraw the interval or
    add dependence-aware methods before relying on it.
