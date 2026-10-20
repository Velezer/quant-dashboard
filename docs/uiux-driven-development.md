# UI/UX-driven development

UI work is complete only when the user can understand the current state, choose
the next safe action, and recover from missing or stale data. The interface is
part of the market-data contract, so changes must preserve app identity, data
provenance, the read-only execution boundary, and chart meaning.

## UI is the product path

Every user-facing product capability must be reachable from the normal
application UI. A backend endpoint, CLI command, direct database operation, or
hidden/debug screen may support the capability, but cannot be its only user
workflow. Explicit maintainer operations remain documented commands when the
storage or operations contract requires them.

For each capability, the UI must let the user discover and perform its intended
task, provide any required inputs, and inspect the resulting value or outcome.
It must also show the relevant status, data source or provenance, limitations,
and a safe recovery action. Do not expose actions that conflict with project
boundaries, such as order placement or paper trading. Verify the complete flow
in a fresh browser session, including applicable loading, empty, stale, error,
and recovery states and the keyboard-accessible path.

## Working loop

1. **State the user outcome.** Translate the request into one primary task, one
   decision the user needs to make, and one observable success condition.
2. **Write the UX contract before implementation.** Define the primary state,
   context shown beside it, feedback during loading, stale or unavailable
   states, the recovery action, and the keyboard or screen-reader path.
3. **Design the hierarchy.** Put the selected app, product/category, signal
   timeframe, direction, score basis, current price, and data freshness beside
   the selected signal and chart. Keep evidence and venue diagnostics behind
   progressive disclosure unless they change the immediate interpretation.
4. **Implement the smallest coherent slice.** Keep domain meaning in the data
   contract. A score ranks signals; it is not probability or validation. Do not
   use color or a technical badge as the only explanation of a market state.
5. **Verify the running surface.** Run relevant automated checks, open a fresh
   browser view, inspect the accessibility tree, exercise the changed controls,
   inspect the chart, and check the console for errors.
6. **Report evidence.** State what passed, what remains unknown, and the exact
   recovery signal if the behavior regresses.

## Indicator scores, browser alerts, and app readiness

Signal cards and selected-chart summaries show four independent indicator
values out of 10, each with its equal 25% maximum weight. The combined /40
value remains an internal ordering value; the interface and scoring guide
present the four factors instead. Verify all four values remain visible on
selected and unselected cards without hover, including narrow screens; old
compact-layout rules can hide nested score sections. Missing components remain unavailable;
legacy score models retain their original basis. This presentation change does
not change signal eligibility, source identity, formulas, or stored scores.

The Notifications panel offers Enable browser alerts, Disable, and a test
notification. Permission is requested only from the enable-button gesture.
Delivery applies to new server signal events from all apps while this dashboard
is open, including a background tab; reloading or closing it stops delivery. Unsupported,
insecure, denied, and disconnected states explain recovery. Fetched inbox
history is never replayed. Keep duplicate IDs only in bounded session memory,
group each incoming batch, and focus the existing inbox on notification click.
Actual display/sound remains subject to browser and operating-system settings.

Each selected app has an in-context readiness explanation and listing-retry
control. Missing listings, provider failures, gathering history, stale prices,
and no qualifying setups are distinct. No app receives fabricated signals.

1. **Direct effect:** Show four /10 values and deliver opted-in new alerts.
2. **Immediate consequence:** A reader can identify each factor and receive a
   browser notice without watching the current app's filter.
3. **Response:** Batch/deduplicate notifications and disable controls while
   requesting permission or refreshing listings to prevent repeated work.
4. **Interactions:** Wire existing scan SSE events to existing alert consumers;
   keep app/product/source/timeframe identity and freshness gates intact.
5. **Accumulation:** Do not persist financial events in browser storage or
   create a push service, duplicate market cache, or new database.
6. **Failure boundaries:** Preserve unavailable scores and explicit empty-app
   reasons; permission denial and provider errors do not become success.
7. **Recovery:** Native browser settings, test delivery, listing retry, and the
   existing scan refresh expose the recovery path in the application.
8. **Governance:** Use existing frontend, SSE, registry, and SQLite boundaries;
   no execution behavior or provider-policy bypass is introduced.
9. **Trajectory:** Hypothesis: explicit factors and readiness reduce misleading
   zeros and blank screens; reconsider if users confuse a held value for fresh.
10. **Decision:** Correct existing delivery/presentation paths, verify all four
    app identities and permission states, and retain honest qualification rules.

## Scoring guide UX contract

The primary task is to understand a signal's displayed score and decide whether
its model, timeframe, source, and freshness make it comparable to another row.
The dashboard links to `/scoring`, a single page with a return link, section
navigation, four-factor explanations, an illustrative points example, status
definitions, and the separate strategy/confluence and Tokocrypto voting rules.
The example displays supplied factor points separately; it does not evaluate market candles,
qualify a setup, or produce a signal. Current model identities and thresholds
come from the existing master-data API. Explanations remain readable while that
catalog loads or is unavailable; the catalog area exposes loading, empty, error,
and Retry states. All controls use native links, labels, and keyboard behavior.

1. **Direct effect:** One discoverable page explains the score and its factors.
   Acceptance: dashboard → Scoring guide → dashboard works in a fresh browser.
2. **Immediate consequence:** Readers can inspect four displayed factors and their weights and distinguish a retained value from a current setup.
3. **Response:** Illustrative inputs update only the example; navigation or
   Retry never requests a market scan or changes stored scores.
4. **Interactions:** Strategy source owns formulas and catalog metadata owns
   active model labels, scales, and floors. Model-specific DSS rules stay distinct.
5. **Accumulation:** No application data, preferences, or example inputs persist.
   Keep the explanation synchronized with the owned scoring definitions.
6. **Failure boundaries:** Missing catalog data has an explicit recovery state;
   stale, provisional, non-qualifying, unavailable, and legacy scores stay distinct.
7. **Recovery:** Retry reloads the catalog; the return link reaches the screener
   and its existing data-refresh controls.
8. **Governance:** This adds explanation within the existing frontend and Rust
   routing boundaries, without changing scoring, execution, or storage policy.
9. **Trajectory:** Hypothesis: one reference page reduces confusion about zero
   and held scores; reconsider if readers mistake the example for a real signal.
10. **Decision:** Add one small page using existing asset serving and metadata.
    Check arithmetic, content provenance, keyboard use, mobile layout, and errors.

## Screener product contract

- **Primary task and decision:** Find current, ranked LONG or SHORT signals for
  an app, product/category, and timeframe; decide which row merits further
  inspection. A signal row is informational and never creates an order.
- **App first:** Stockbit, Pintu, Pluang, or Tokocrypto is selected before its
  product and category. Tokocrypto crypto spot and tokenized stocks remain
  separate visible categories. Unverified listings stay pending.
- **Independent timeframes:** Generate separate signal rows for 15m, 1h, 2h,
  4h, 1d, and 1w. Apply the same owned strategy and scoring rules separately to
  each timeframe. The timeframe is part of signal identity; one timeframe must
  not overwrite another.
- **Refresh and expiry:** Sync market data and regenerate the screener every
  15 minutes. Non-retained signals use completed candles, expire after two
  hours, and hide after expiry. Stockbit regular IDX NWE/DSS models have
  independent strategy identities for 15m, 1h, 2h, 4h, 1d, and 1w. Each row
  scores its own timeframe's latest fresh same-source bar on each successful
  scan. Label a forming-bar score provisional; when a model no longer
  qualifies, keep that timeframe's row with score 0 and a `not_qualifying`
  state. If its fresh inputs are missing, retain its last score and label it
  stale. Failed scans retain the last good rows with a visible stale/error
  state and retry action.
- **Score basis:** Show the score components used for ranking, including DSS
  Bressert near/cross states and extreme-zone evidence, plus Nadaraya-Watson
  near-line/cross states when available. The NWE/DSS research ranking also
  shows its four equal-weight factors: NWE, DSS Bressert, 20-bar relative
  volume, and the explicitly estimated OHLCV volume-delta proxy. Distinguish
  each indicator as its own `/10` score with 25% weight; retain the sum only
  for internal ranking. Avoid a combined `/40` score in the current model UI.
  Keep component values, direction/alignment, and unavailable states visible so
  a score is interpretable. Missing components are unavailable, not silently
  treated as agreement. The candle-derived delta is not aggressor-side trade
  data. Keep point-based ranking models distinct from Tokocrypto's separate
  two-of-four vote gate and retained legacy score scales. See
  [signal scoring rules](signal-scoring.md) for the model distinctions and
  calculation basis. A score is not a probability, backtest, or execution
  recommendation.
- **Data and validation:** Show current price, signal candle time, price data
  source/provider, quality, and matching validation status as separate facts.
  Stockbit NWE/DSS row state and score-bar time for each timeframe must
  distinguish provisional, non-qualifying, and stale retained scores from a
  current qualifying setup.
  Proxy-origin market rows remain visibly labeled and never count as
  venue-native execution evidence. Missing or stale candles do not produce a
  current signal.
- **Strategy identity:** Display the active profile's strategy label beside
  the signal. Tokocrypto crypto-spot and tokenized-stock profiles use the
  research-only 2-of-4 model; a missing or neutral component contributes no
  vote, and the label must not imply that all four factors aligned.
- **Filters and recovery:** App, product/category, timeframe, and search filters
  must leave a clear empty state when no rows match. Loading, unavailable,
  stale, and recovery states must be explicit. Retry fetches fresh data without
  erasing the last good snapshot on failure.
- **Chart truth:** Selecting a screener row shows actual stored candles and
  wicks, exact OHLC context, app, symbol, and chart timeframe. The signal's
  generation timeframe stays visible when the chart timeframe changes. Missing
  or stale history is described with a retry path. The screener chart has no
  entry, stop, target, or position overlays.
- **No trade-plan lifecycle:** The screener has no trade-plan generation,
  review, execution status, entry/stop/target levels, or plan API. Existing
  SQLite plan-history tables and records are retained only because the project
  storage contract requires the app database to preserve them; the screener
  does not read or update those records. Do not run cleanup against that history
  as part of a UI change.
- **Signal alerts:** Indicator-combination alerts are informational signal
  history only. They expire after two hours and do not attach entry zones,
  plans, accounts, positions, orders, or trade history.
- **Evidence honesty:** Win rate, CAGR, profit factor, trade count, and quality
  are labeled with their sample and validation status. Never infer CAGR or a
  guaranteed return from another metric.
- **Financial semantics:** Green, red, amber, and neutral states retain their
  trading meaning; direction-adjusted return, raw price change, margin ROI,
  funding, and unknown values remain distinct.
- **Read-only boundary:** The dashboard has no paper-trading or order-placement
  flow. Under the user's explicit approval on 2026-10-01, the existing R:R
  simulator may display an illustrative expected account-equity curve from
  editable assumptions. It must not create simulated account, position, order,
  or trade-history state. Under the user's explicit approval on 2026-10-12, the
  registered Pine strategy backtest may show its transient simulated strategy
  equity curve and up to 200 recent closed trades from bounded stored history.
  The user may select an optional inclusive UTC date range; show actual candle
  sample bounds and disclose when the runtime cap may truncate that range.
  Those results are research output only: they are not persisted, do not create
  account, position, or order state, and never qualify as execution evidence or
  paper trading. No browser check may place a live order.
  The service runs one bounded in-process backtest job at a time, retains its
  transient result for no more than 30 minutes, and exposes progress, stop, and
  recovery states in the dashboard. Strategy history is capped at 20,000 bars;
  each evaluator is bounded to 100,000 retained series values, 1 MiB of retained
  strings, and two distinct higher-timeframe request caches. The returned curve
  is sampled to a capped number of points while summary metrics use the full
  candle sequence.
  The chronological tail check uses a selected 50%–90% training share over the
  candle sample. It evaluates Pine series over the full sequence for indicator
  warm-up, suppresses broker side effects before the split, and runs the
  validation tail from flat with initial capital. Training trades that cross
  the boundary are excluded from training closed-trade metrics. This is one
  tail split; choosing source code or inputs based on its results contaminates
  the tail, so it is never described as an untouched holdout or execution
  validation. Report 95% Wilson intervals for train and validation win rates,
  labeled as descriptive uncertainty that assumes independent closed trades;
  trading serial dependence may make those intervals too narrow. Compare the
  mean marked monthly return over up to three recent training months with
  validation months computed from the flat-reset validation run, disclose
  observation counts, and exclude a separable validation boundary month.
  Neither statistic is predictive evidence.
- **Performance:** The steady-state memory target remains 16 MB. An 8 MB mode
  may reduce optional history or diagnostics but must not hide signal freshness,
  source, app, or chart context required to interpret a row.

## Decision record: change the active workspace to a screener

The evidence at implementation time is the current `/api/signals` snapshot and
its SQLite-backed rows; that API already contains timeframe-keyed signal rows,
current prices, score components, provenance, and evidence state. The old
workspace exposed plan review and entry/stop/target concepts, and the old
`/api/compute/plan` endpoint returned a generated plan object.

1. **First order:** The active workspace becomes a screener. Acceptance: rows
   show app/product, timeframe, direction, rank score/basis, current price, and
   freshness without trade levels or review actions.
2. **Second order:** Users filter and inspect a signal before deciding whether
   it is relevant. Acceptance: filters and selection update list and chart
   context together.
3. **Third order:** The 15-minute scheduler must refresh each enabled timeframe;
   six independent rows per symbol may increase scan work. Keep existing bounded
   candle windows and expose scan progress and status.
4. **Fourth order:** The screener reads its dedicated signal endpoint and
   market-history API. The legacy plan compute path is removed from routing, so
   no neighboring UI consumer can recreate the removed workflow.
5. **Fifth order:** Two-hour cleanup bounds stale screener and alert records.
   SQLite market history remains canonical and durable; do not duplicate candle
   data in the screener snapshot.
6. **Sixth order:** A failed refresh must not replace good rows with partial
   results; stale/error status and retry expose recovery. Missing or malformed
   rows are rejected or withheld rather than ranked as current.
7. **Seventh order:** Observe last attempt/success times, error text, row count,
   and the manual retry action. Revert the UI/API route changes if the new
   endpoint cannot return the stored signal snapshot or its provenance.
8. **Eighth order:** The change removes active plan behavior without changing
   app identity, provider ownership, stored history, or the read-only boundary.
   Preserve existing plan-history tables because the supplied storage contract
   names them as app-owned data.
9. **Ninth order:** A simpler workspace may make timeframe-specific changes
   easier to notice; treat that as a hypothesis and reconsider if fresh-session
   use shows users cannot distinguish signal timeframe from chart timeframe.
10. **Tenth order:** The smallest path is to expose only screener rows, remove
    plan actions/compute routing, and keep canonical data and the allowed R:R
    simulator. If a fresh browser session shows plan levels or stale data looks
    current, restore the last known-good interface and correct the leaking
    consumer before continuing.

## Decision record: recover an empty stale signal snapshot

1. **Direct effect:** Retry on a stale, failed, or never-scanned snapshot queues
   the existing background scan. Acceptance: POST `/api/refresh`, followed by a
   new authoritative snapshot when scanning completes.
2. **Immediate consequence:** A snapshot-fetch error on otherwise fresh data
   retries the read; scan failures retry the scanner. Preserve the last good rows.
3. **Response:** Repeated retry uses the existing running-scan conflict response;
   it does not create another synchronous scan or weaken freshness checks.
4. **Interactions:** The market scheduler remains the candle owner. The retry
   message distinguishes queued scanning from background candle refresh.
5. **Accumulation:** Hypothesis: skipping unused timeframe metadata when the
   current price is absent reduces repeated scan cost; measure a live recovery.
6. **Failure boundaries:** Stale/missing prices still emit no signal. Retain the
   daily history check so both intraday and daily recovery priorities survive.
7. **Recovery:** Existing status, progress, scan-completed/failed events, and
   periodic snapshot reads expose recovery. A failed POST keeps retry available.
8. **Governance:** No changes to storage, provider identity, signal policy,
   evidence, execution, or architecture are authorized or introduced.
9. **Trajectory:** Hypothesis: reliable retry reduces repeated refresh attempts;
   reconsider if a fresh browser still shows stale data after scan completion.
10. **Decision:** Correct the existing retry handler and omit unusable metadata
    reads. Verify stale retry versus read retry, existing freshness/priority
    tests, and a fresh browser scan. Revert if recovery priorities disappear or
    stale signals become visible as current.

The recovery UX retains the last successful scan time and expired-row hiding.
The existing scan banner shows progress; an empty fresh result remains valid
when current completed candles do not satisfy the owned signal rules. Retry is
a keyboard-accessible button and does not promise that a signal will exist.

## Definition of done for a UI change

- The primary task and acceptance checks are recorded in the change or final
  report.
- Automated checks cover the changed behavior and pass.
- A fresh browser session verifies the changed flow, selected-state behavior,
  loading/stale/empty/recovery states, and console health.
- Changed controls have accessible names and keyboard behavior where applicable.
- The chart is checked for candles, wicks, OHLC, app label, signal timeframe,
  zoom, pan, fit/reset, and recovery when the change touches chart behavior.
- The final report distinguishes verified facts from assumptions and names any
  data source or venue limitation.

## Decision record: consolidate screening into Signal screener

1. **Direct effect:** Remove the IDX screener button and dialog; Signal screener is the sole screening surface, including Stockbit. Acceptance: no IDX dialog or entry point remains.
2. **Immediate consequence:** Stockbit users use the existing app/category/timeframe filters and signal selection.
3. **Response:** Hypothesis: a single entry point reduces navigation confusion; verify the Stockbit filter in a fresh browser.
4. **Interactions:** Retire the dedicated daily-price screening script and API route; preserve the signal endpoint, registry, provider ownership, and chart flow.
5. **Accumulation:** Remove unused styles and daily-price calculations rather than maintain a hidden legacy path.
6. **Failure boundaries:** Existing signal loading, stale, empty, error, and retry states remain authoritative; removing this dialog must not prevent dashboard initialization.
7. **Recovery:** Check module initialization and filtering; revert this removal if Stockbit screening becomes inaccessible.
8. **Governance:** The user authorized removal of the redundant screener; SQLite history, evidence, and the read-only boundary remain unchanged.
9. **Trajectory:** Hypothesis: future screening improvements use the existing signal surface instead of adding venue-specific dialogs.
10. **Decision:** Delete the isolated UI/module/route without reorganizing architecture. Reconsider if fresh-session filtering cannot reach Stockbit signals.

The retained UX uses keyboard-accessible app and timeframe filters, its existing signal/chart context, and explicit empty and stale recovery states. No database cleanup accompanies this change.

## Decision record: repair retired strategy bindings in the registered universe

Observed: the successful scan returned zero rows across all apps. SQLite's catalog
still bound Tokocrypto, Pintu Spot, and Pluang profiles to engine IDs the current
engine rejects. Checking BTC alone did not establish universe health.

1. Direct effect: the explicit catalog migration binds inherited profiles to
   their existing authoritative strategy templates. Acceptance: no migrated
   profile retains an unsupported engine binding.
2. Immediate consequence: fresh, sufficiently populated histories can reach
   strategy evaluation; neutral results remain absent from the screener.
3. Response: repeated migration returns zero changes after validating its hash.
4. Interactions: retain app/product/category identity and source provenance;
   read algorithm and evidence version fields from the owning catalog template.
5. Accumulation: preserve old evidence and market history. A new migration marker
   allows databases with the earlier migration to receive this repair explicitly.
6. Failure boundaries: missing/stale candles and insufficient history still
   withhold signals. This repair does not claim all apps have available prices.
7. Recovery: transactional migration, content hashes, row counts, dedicated tests,
   and a subsequent scan establish whether repaired bindings produce valid rows.
8. Governance: restore existing product profiles to their intended owned engine;
   no execution, provider, storage, or architecture boundary changes.
9. Trajectory: hypothesis: preserving inherited template bindings prevents silent
   empty scans when retired engine versions are removed.
10. Decision: extend the existing maintainer-run migration, never restore retired
    aliases or migrate on startup. Reconsider if migrated IDs remain unsupported
    or a fresh scan still excludes an independently verified directional result.

The same explicit command, `tooling\migrate-tokocrypto-confluence.bat`, also
repairs product profiles cloned from the spot and futures templates. Stop the
service before running it, then restart with `tooling\start.bat`. The
`registered_profile_strategy_bindings_v2` SQLite marker records hashes and
changed catalog row counts for this combined catalog repair.
Fresh clones bind inherited profiles to current engines when seeding their catalog.
This repair does not relax candle freshness, confirmation rules, or validation.

## Decision record: explain an empty screener when prices are unavailable

Observed: provider requests failed and fresh 15-minute bars were unavailable
across the registered universe. The safety gate correctly emitted no current
signals, but the UI presented the result as an ordinary zero-row scan.

The scan now records whether fresh price data is ready, partially stale, or
unavailable, together with the affected registered-instrument count. The API
and dashboard expose that state and keep Retry available. Directional signal
rules, freshness gates, provider ownership, and read-only behavior are unchanged.

Acceptance: a completed scan with no fresh prices reports `price_data_status`
`unavailable`, the count is visible in the screener empty state, and the
existing retry path remains available. A scan with fresh prices and no strategy
match continues to show a normal qualifying-signal empty state.

## Superseded decision record: Stockbit 4h band-position score

This historical score model was superseded on 2026-10-07 by the completed-bar
NWE crossover rule recorded below. Its coverage figures describe the earlier
score model only and do not establish crossover frequency or performance.

Observed: a completed-bar scan returned no Stockbit rows even though the
existing event-only model required a same-bar upper-band cross. A read-only
calculation over current database history found 819 Stockbit symbols with
sufficient fresh 4h data under the existing IDX freshness rule; 106 had a
bullish causal NWE band-position score of at least 5/10. This is coverage
evidence only, not strategy-performance validation. The 5/10 cutoff means the
close has moved at least halfway from the causal midline to the upper band.

1. **Direct effect:** Score the latest completed Stockbit IDX 4h state. Emit
   LONG only when the close is above the causal midline, the band width is
   positive and finite, at least 500 completed bars are available, and the
   score is at least 5/10. Acceptance: current fresh history with that state
   creates a screener row, and the live-data distribution includes qualifying
   rows.
2. **Immediate consequence:** Use the 4h close, bar time, source, and existing
   completed-bar freshness policy; do not require a fresh 15m quote. The row
   shows price-bar age so a prior-session state is apparent.
3. **Response:** Each scheduled scan re-evaluates the current state. The
   screener snapshot still expires after two hours without a successful scan;
   event-alert history separately retains its two-hour candle-age policy.
4. **Interactions:** Keep `stockbit`, `stockbit_regular`, `idx_stock`, and `4h`
   as the strategy scope. Preserve source and provider labels; proxy rows remain
   unvalidated research and cannot count as venue-native execution evidence.
5. **Accumulation:** Reuse the existing endpoint envelope configuration and
   per-symbol SQLite series. Scanning 500 bars remains inside the 720-bar
   existing scan bound; no derived chart cache or new store is introduced.
6. **Failure conditions:** Missing, stale, incomplete, malformed, insufficient,
   non-finite, zero-width, below-midline, or below-threshold values produce no
   row. No SHORT is emitted. Fetch and scan failures remain distinguishable from
   a valid empty result.
7. **Recovery:** Keep the existing hourly proxy warm-up and scan retry paths.
   Refreshing signals requests a scan; the market-data worker owns candle
   ingestion. A missed scan can recover on the next successful cycle.
8. **Governance:** The score ranks distance toward the upper band; it is not a
   probability, validation claim, plan, account, position, order, or paper-trade
   state. The previous crossing strategy identity is retired, not aliased.
9. **Trajectory:** Hypothesis: ranking current band positions may make the
   Stockbit screener useful between rare crossing events. Chronological
   out-of-sample evidence is still required before describing predictive value.
10. **Decision:** Choose the smallest reversible change: a named score model,
    validated 5/10 default, existing bounded scanner and storage, catalog-backed
    UI explanation, and focused tests. Reconsider if browser rows lack current
    bar/source labels, if the computed threshold disagrees with the displayed
    basis, or if fresh qualifying states continue to return an empty snapshot.

## Superseded decision record: Stockbit 4h score input and empty state

This historical score-specific empty state was superseded on 2026-10-07 by the
cross-specific rule below. The existing stale, error, retry, source, and
completed-bar freshness behavior remains applicable.

The user's task is to find a current Stockbit IDX signal to inspect. A LONG row
is eligible when a completed 4h close is above the causal NWE midline and its
band-position score is at least the catalog's configured minimum (5/10 by
default), with at least 500 completed bars. Acceptance includes a nonempty scan
when real SQLite history meets those conditions. The row preserves the app,
product, category, timeframe, research source, price-bar age, score basis, and
unvalidated status.

1. **Direct effect:** Re-evaluate current 4h band position, not just a new
   upper-band event. Empty copy states the current threshold and required
   history from strategy catalog metadata.
2. **Immediate consequence:** 4h scoring remains independent of fresh 15m
   prices. A valid signal can persist across scans while the 4h state and
   existing bar-freshness policy remain valid.
3. **Response:** The keyboard-accessible Retry requests a scan; hourly market
   ingestion remains separate. The UI does not promise that a retry will create
   a qualifying score or synchronously fetch data.
4. **Interactions:** Keep app/product/timeframe filters clear. Preserve proxy
   origin, the existing bar freshness limit, and the two-hour scan snapshot
   expiry; chart candles still read canonical SQLite history.
5. **Accumulation:** The snapshot stores only screener rows, not duplicate
   candle history. The existing warm-up bound and per-symbol database remain
   the history boundary.
6. **Failure boundaries:** Scan errors and stale snapshots take precedence over
   the score-specific empty message. Short history, stale bars, invalid
   envelope width, or below-threshold score must not appear as active rows.
7. **Recovery:** Last-success and scan status remain visible; ingestion
   diagnostics expose warm-up errors; retry requests another scan while the
   scheduled worker handles candle updates.
8. **Governance:** The view is read-only and research-only. Scores rank signals,
   not probability, and no execution or account state is exposed.
9. **Longer-term hypothesis:** The score-qualified empty state should help
   distinguish an ordinary low-score universe from unavailable history; a
   fresh browser session checks that the changed copy and recovery path render.
10. **Decision:** Use catalog metadata for the empty-state threshold and history
    count. Reconsider if the UI implies validation or if a successful fresh scan
    still omits a qualifying source row.

## Superseded decision record: Stockbit 4h NWE band-cross direction

The Stockbit 4h signal uses completed causal endpoint-envelope bars. A close
crossing below the lower band emits LONG; a close crossing above the upper band
emits SHORT. Each event gets the existing 10-point signal-strength ceiling as a
binary event score, not as probability or validation. The signal remains
research-only; IDX plan and execution policy stays long-only. The current
500-bar endpoint configuration needs 501 completed bars to compare the current
band value with the prior completed band's value.

1. **Direct effect:** Emit LONG on a lower-band downward cross and SHORT on an
   upper-band upward cross. Acceptance: each direction appears only on its
   matching completed 4h crossing candle.
2. **Immediate consequence:** Price, bar time, source, provider, quality, and
   freshness continue to come from that 4h candle; a fresh 15m quote is not
   required for this standalone model.
3. **Response:** A later candle without a new cross emits no row. A normal empty
   result explains both mappings, required history, and the existing refresh
   action.
4. **Interactions:** Keep the model scoped to Stockbit, `stockbit_regular`,
   `idx_stock`, and `4h`. Proxy labels remain visible; screener direction does
   not change long-only plan or execution rules.
5. **Accumulation:** Reuse the configured endpoint envelope, bounded 4h history,
   scanner, and per-symbol SQLite history. No duplicate candle cache or store is
   added.
6. **Failure boundaries:** Missing, stale, incomplete, malformed, non-finite,
   or insufficient input and bars without a confirmed cross produce no signal.
   Cross flags are mutually exclusive; an ambiguous state emits nothing.
7. **Recovery:** The existing hourly proxy warm-up supplies history, scan status
   distinguishes failure from a valid empty result, and Refresh signals requests
   the existing scan path.
8. **Governance:** SHORT is an informational research direction. It does not
   enable IDX short plans, accounts, positions, paper trading, or orders. Old
   strategy identities are retired while stored history keeps its provenance.
9. **Trajectory:** Hypothesis: showing both envelope extremes may make current
   directional pressure easier to inspect. No predictive value is implied;
   chronological out-of-sample evidence is required for validation.
10. **Decision:** The binary score was replaced by the confirmation-ranked model
    below. The direction, freshness, source, and research-only decisions remain.

## Decision record: align NWE chart markers with cross direction

The chart's endpoint cross fields identify closes crossing above the upper band
and below the lower band. Their marker directions now match the Stockbit 4h
research signal mapping.

1. **Direct effect:** Lower-band crosses point up and use the LONG color; upper-
   band crosses point down and use the SHORT color. Acceptance: each crossing
   renders once with the signal's direction.
2. **Immediate consequence:** A user can compare the 4h signal row with the
   selected chart's crossing without an opposing marker.
3. **Response:** Users may inspect the marked candle and stored OHLC context;
   markers remain study events and do not create orders.
4. **Interactions:** Keep the existing chart data, indicator calculation,
   repainting selection, and app/category policy; change only marker mapping.
5. **Accumulation:** One marker pass preserves one visual direction per crossing
   as other study overlays render.
6. **Failure boundaries:** Missing or stale band values still draw no marker;
   the status label continues to show chart timeframe and repainting mode.
7. **Recovery:** If a marker points opposite its signal row, restore the prior
   marker mapping and correct the event-direction translation.
8. **Governance:** This is chart annotation for research. Stockbit IDX plan and
   execution rules remain long-only.
9. **Trajectory:** Hypothesis: matching marker and signal directions reduces
   interpretation errors; reconsider if fresh-session inspection shows a
   mismatch between the marker and completed-bar signal.
10. **Decision:** Keep a single renderer pass with lower-up/LONG and upper-down/
    SHORT. Reconsider if the displayed study mode is mistaken for causal signal
    evidence.

## Superseded decision record: score Stockbit 4h NWE crosses with a 70% floor

The score is a ranking aid, not a probability or a claim of validated returns.
The completed NWE cross supplies the existing 70% strategy weight. A
direction-aligned completed 4h DSS cross or near-cross and a recent same-direction
DSS extreme turn contribute the remaining 30%, using the existing DSS point
rules. The 7/10 minimum is published in the strategy catalog and enforced before
a screener row is stored. A cross without DSS confirmation still scores 7/10;
the extra evidence ranks it above the floor.

1. **Direct effect:** LONG still requires a downward cross below the lower band;
   SHORT still requires an upward cross above the upper band. A valid cross is
   scored from 7/10 to 10/10, and only scores at or above 7/10 enter the screener.
   Acceptance: each stored Stockbit 4h row shows its score, 70% floor, and NWE/DSS
   components.
2. **Immediate consequence:** DSS confirmation can raise ranking strength but
   cannot reverse direction or remove a valid NWE cross at the 7/10 floor. Other
   strategy scores and app/timeframe filters are unchanged.
3. **Response:** Repeated scheduled scans recompute the score from completed
   candles. A non-cross emits no row; a matching DSS cross or recent extreme
   turn raises the score without creating an additional signal event.
4. **Interactions:** The core strategy owns score components and minimum. The
   catalog publishes that policy to the backend and dashboard. SQLite remains
   the canonical screener snapshot; no browser or file-backed scoring state is
   introduced.
5. **Accumulation:** Rows retain the new strategy/model version and existing
   two-hour expiry. Old score identities are retired, not relabeled as the new
   score. No new persistence or candle cache is added.
6. **Failure boundaries:** Missing or unusable DSS values add no DSS points;
   the NWE event remains exactly 7/10. Missing, stale, incomplete, malformed,
   or non-finite NWE inputs emit no signal. Catalog minimums are validated and
   rows below the published floor are rejected.
7. **Recovery:** The catalog explains the formula and floor in the empty state;
   scan status and retry continue to distinguish a valid no-cross result from a
   failed or stale scan. Rechecking after a successful scan shows the new score
   and its data provenance.
8. **Governance:** This ranking does not imply probability, backtest validation,
   execution evidence, a shortable IDX product, or authorization for plans or
   orders. Proxy candles keep their source and validation labels.
9. **Trajectory:** Hypothesis: displaying aligned DSS evidence makes stronger
   crosses easier to prioritize. Chronological out-of-sample evidence would be
   needed to establish predictive value; a score distribution alone cannot.
10. **Decision:** Reuse the existing 70/30 strategy/confluence blend and DSS
    cross/near-cross/extreme-turn point definitions. Apply the catalog-backed
    7/10 floor at signal creation/storage and display it beside the score.
    Reconsider if the UI formula differs from the numeric score or valid crosses
    appear below the advertised floor.

## Decision record: rank Stockbit 4h NWE crosses and near-band states with a 50% floor

This decision supersedes the 70% cross-only floor above. A confirmed lower-band
cross remains LONG and an upper-band cross remains SHORT. Without a cross, the
nearest band also qualifies when a completed close is within half the full
envelope width of that band, on either side. The proximity contribution rises
linearly from 5 to 7 points as price approaches the band; confirmed crossings
retain the full 7-point NWE contribution. Direction-aligned DSS evidence adds
up to 3 points, so the inclusive minimum is 5/10. Scores rank research signals;
they are not probabilities or validated returns.

1. **Direct effect:** Show current 4h NWE crosses and qualifying near-band
   states, with lower-band LONG and upper-band SHORT. Acceptance: the latest
   completed bar produces the matching direction and a score of at least 5/10.
2. **Immediate consequence:** Each row keeps its 4h score, band state, current
   close, source, quality, and bar time together. A fresh 15m quote is not
   required for this Stockbit research signal.
3. **Response:** Scheduled scans may continue to rank a signal while completed
   4h closes remain near the same band. The existing strategy identity, alert
   lifecycle, and snapshot expiry prevent repeated scans from inventing a new
   candle or changing the score model silently.
4. **Interactions:** The strategy owns the half-width proximity rule and 5/10
   floor; catalog metadata publishes both to the backend and dashboard. Keep
   the existing 70/30 NWE/DSS blend, app/category scope, and proxy provenance.
5. **Accumulation:** The existing bounded scan reads completed candles from
   SQLite and replaces the current signal snapshot. No candle cache, new store,
   or account state is added. Research-proxy rows remain visibly unvalidated.
6. **Failure boundaries:** Missing, stale, incomplete, non-finite, or
   non-positive prices, invalid/zero envelope width, an exact equidistant
   midline, and below-floor results produce no row. A confirmed cross remains
   eligible even when its overshoot exceeds the near-band range; non-cross
   proximity beyond half a width does not qualify.
7. **Recovery:** Existing scan status, last-success time, stale/error message,
   and Retry action remain the recovery path. A scan error must not become a
   valid empty result, and retry does not implicitly fetch market data.
8. **Governance:** Publish a new strategy/model identity and retire the
   cross-only model so old score semantics are not relabeled. SHORT remains an
   informational research direction and does not enable IDX short plans,
   accounts, positions, paper trading, or orders.
9. **Trajectory:** Hypothesis: near-band ranking may make the screener useful
   between crosses. Predictive value remains unestablished; chronological
   out-of-sample evidence would be required before making that claim.
10. **Decision:** Use a versioned strategy, the current causal endpoint
    envelope, an inclusive 5/10 score floor, and the existing DSS ranking
    evidence. Reconsider if a row's displayed score or state disagrees with its
    completed-bar distance, or if a valid near/cross state is omitted after a
    successful scan.

## Decision record: add a scored Stockbit 4h DSS Bressert near-cross signal

The DSS signal is separate from the NWE-triggered signal. A completed bullish
DSS cross or approaching bullish near-cross is LONG; a completed bearish cross
or approaching bearish near-cross is SHORT. A near-cross must be approaching
zero within the DSS owner's 5-point spread threshold. Its 70% base contribution
rises linearly from 5 to 7 points as the absolute spread moves from 5 to zero;
a confirmed cross contributes 7 points. Same-direction causal NWE cross or
near-band support contributes the remaining 30%, with a 5/10 inclusive floor.
These are ranked research signals, not probabilities or validated returns.

1. **Direct effect:** Add a separate Stockbit IDX 4h screener row for the latest
   completed DSS cross or qualifying near-cross. Acceptance: the row shows
   direction, 5–10/10 score, DSS and NWE components, completed bar time, and
   source/quality; a non-approaching spread outside the threshold creates no
   DSS row.
2. **Immediate consequence:** The DSS strategy can use its 50-bar indicator
   history independently of the NWE strategy's 501-bar endpoint-envelope
   requirement. It uses its own completed 4h close, so no fresh 15m quote is
   needed for either Stockbit 4h research signal.
3. **Response:** Scheduled scans recompute current states. Repeated near-cross
   readings update the current snapshot; the existing strategy identity and
   two-hour alert lifecycle preserve candle and direction identity.
4. **Interactions:** Keep the app, product, category, and 4h scope aligned with
   the existing Stockbit NWE model. NWE support only adds points when its
   direction agrees with DSS. The screener catalog owns labels and formula
   details; SQLite remains the signal snapshot store and canonical candles stay
   in market-history SQLite.
5. **Accumulation:** The two signal models have distinct strategy and scoring
   identities, so stored NWE records are not relabeled. Only bounded screener
   rows are added; no candle cache, browser market state, or additional store is
   introduced.
6. **Failure boundaries:** Missing, stale, unordered, incomplete, malformed,
   or non-finite bars produce no DSS row. A missing or opposing NWE state adds
   no confluence points. Rows below the catalog floor are withheld. Proxy
   provenance remains explicit and is never venue-native execution evidence.
7. **Recovery:** The existing scan status and Retry action remain the recovery
   path. The Stockbit 4h empty state lists each model's history and score floor.
   A scan failure or stale history must remain distinguishable from a valid
   no-signal result.
8. **Governance:** This does not change Stockbit's long-only plan or execution
   policy. SHORT is informational, and the signal creates no plan, account,
   position, paper trade, or order state. The score is not validation evidence.
9. **Trajectory:** Hypothesis: showing DSS near-crosses between crosses may
   make the 4h screener more useful. Predictive value remains unproven; only
   chronological out-of-sample evidence can support that claim.
10. **Decision:** Reuse the existing 70/30 score blend, 5/10 floor, causal NWE
    candidate, DSS spread threshold, SQLite history, source labels, and retry
    flow. Reconsider if the displayed score disagrees with the DSS spread or
    NWE direction, if fewer than 50 valid bars can create a row, or if a valid
    near-cross is omitted after a successful scan.

## Decision record: keep valid Stockbit signals visible in the screener

The API freshness filter must use each row's exact `storage_symbol`, because
display tickers such as `MGNA` omit the `.JK` suffix that identifies the IDX
freshness policy. The initial screener app is Stockbit, where the current
workspace has saved 4h NWE and DSS signals.

1. **Direct effect:** Apply the IDX freshness threshold to the stored market
   symbol and show the available Stockbit 4h signal rows on first load.
2. **Immediate consequence:** Valid completed 4h rows remain current under the
   same freshness rule used when the scan writes them; expired rows stay
   withheld.
3. **Response:** Users can switch to other apps at any time. A genuine lack of
   qualifying rows still shows the existing empty state and recovery message.
4. **Interactions:** Keep display symbol and `storage_symbol` distinct. Use
   `storage_symbol` only for market-history freshness; retain app and product
   filters for screener selection.
5. **Accumulation:** No additional persistence or cache is introduced. The
   screener continues to read the existing SQLite signal snapshot.
6. **Failure boundaries:** Missing `storage_symbol` or stale/malformed bars
   remain withheld. Never infer freshness from a display label or current
   wall-clock scan time.
7. **Recovery:** The normal scheduled scan refreshes snapshots. The UI retains
   its retry and explicit stale/empty states when provider history is actually
   stale or unavailable.
8. **Governance:** Stockbit becomes the initial app filter so the requested IDX
   signals appear first; app identity, plan eligibility, proxy provenance, and
   execution policy do not change.
9. **Trajectory:** Hypothesis: matching read-time freshness to write-time
   freshness will prevent false empty screens for other app-specific symbols.
   Verify each additional symbol family against its owning freshness rule.
10. **Decision:** Use the existing canonical `storage_symbol`, not a new
    heuristic or weaker freshness threshold. Reconsider if the UI includes a
    row whose exact stored candle is genuinely stale.

## Decision record: add current crypto 4h NWE and DSS signals

At implementation time `/api/signals` contained 1,052 Stockbit rows and no
crypto rows. The latest scan reported fresh 15m prices unavailable for 2,919
registered instruments. Read only SQLite checks showed current crypto provider
attempts failing to connect and the retained 15m/1h rows older than the signal
expiry window. The requested crypto output must not turn that stale history into
a current signal.

1. **Direct effect:** Apply the existing 4h NWE/DSS ranking algorithms to
   registered crypto spot and perpetual products using completed 4h candles.
   Acceptance: a current qualifying Pintu, Pluang, or Tokocrypto crypto row has
   its own strategy identity, shared score model, price, source, and quality.
2. **Immediate consequence:** These models can use the 4h close as their price
   and do not depend on a separate 15m price. Other strategies keep their own
   freshness requirements.
3. **Response:** The screener can show NWE band proximity and DSS near-cross
   rows when their conditions qualify. A valid no-signal state remains possible
   when no fresh completed bar meets the ranking floor.
4. **Interactions:** Use the existing product scoped 4h history loader, which
   aggregates canonical hourly candles where appropriate. Proxy provider and
   quality labels stay attached. Tokocrypto tokenized stocks remain outside
   crypto categories.
5. **Accumulation:** The change adds no market history, cache, or new runtime
   store. The existing SQLite `bars` data remains canonical and the two score
   models retain separate crypto strategy IDs.
6. **Failure boundaries:** Incomplete, stale, expired, or malformed bars emit
   no current row. A missing NWE confluence value remains labeled unavailable.
   Signals expire two hours after their candle closes, including rows already
   present in a saved snapshot.
7. **Recovery:** The UI distinguishes no current crypto signals and states that
   Refresh signals reruns analysis while the existing hourly market worker
   updates candles. If provider connections remain down, no current signal is
   claimed; the next successful market sync is the recovery evidence.
8. **Governance:** The models are informational and unvalidated. They create no
   plans, account, position, paper trade, or order state. Proxy candles never
   count as venue native execution evidence.
9. **Trajectory:** Hypothesis: sharing score weights across crypto and IDX may
   make rankings easier to compare. Predictive value and provider coverage
   remain unknown until fresh data and chronological validation are available.
10. **Decision:** Use distinct crypto strategy IDs with the same existing
    4h score model IDs, preserve category and source identity, and keep expiry
    and freshness gates. Reconsider only if live, fresh 4h history passes the
    signal rules but the screener still returns no crypto row.

## Decision record: show crypto NWE and DSS signals on every screener timeframe

The screener supports 15m, 1h, 2h, 4h, 1d, and 1w. Apply the existing NWE and
DSS Bressert ranking algorithms and score floors independently to each frame's
own completed candles. Do not reuse 4h prices or infer a qualifying setup when
history is insufficient or no latest-bar condition qualifies.

1. **Direct effect:** Crypto screener models and scans cover all six supported
   frames. Acceptance: each frame has distinct NWE and DSS strategy identities,
   and generated rows identify the same timeframe as their source bars.
2. **Immediate consequence:** Each frame uses its own freshness metadata,
   completed-candle history, score floor, and existing two-hour post-close
   expiry. The 15m quote requirement does not gate these independent signals.
3. **Response:** Users can select a frame or All; qualifying rows appear for
   the chosen frame, while a valid no-setup result remains a clear no-signal
   state.
4. **Interactions:** Reuse the authoritative screener timeframe definitions,
   NWE/DSS algorithms, score weights, model projections, and product/category
   filters. Tokenized stocks stay excluded from crypto categories.
5. **Accumulation:** Read only bounded canonical SQLite candles. Add no history
   cache, runtime store, or browser financial state; keep per-frame strategy
   identity so repeated scans and saved snapshots cannot merge horizons.
6. **Failure boundaries:** Missing, stale, incomplete, malformed, insufficient,
   or non-finite per-frame inputs produce no row. Weekly NWE may remain empty if
   its 501-bar requirement is unavailable; DSS keeps its 50-bar requirement.
7. **Recovery:** Existing market-data sync, signal refresh, status, and retry
   paths remain. Per-frame freshness and the score model's history requirement
   explain a valid empty result separately from a scan failure.
8. **Governance:** All rows remain unvalidated research rankings. Proxy source
   labels remain visible and do not count as venue-native execution evidence;
   no plans, accounts, positions, paper trades, or orders are introduced.
9. **Trajectory:** Hypothesis: showing the same scoring model across horizons
   may make current setups easier to compare. Comparable or predictive value is
   not established without chronological out-of-sample evidence.
10. **Decision:** Generate both crypto models on each supported screener frame,
    keep horizon-specific strategy IDs and the existing shared score model
    versions, and preserve freshness, history, provenance, and expiry rules.
    Reconsider if a fresh qualifying latest-bar setup is omitted or if the UI
    presents a no-signal state as a scan failure.

## 2026-10-09 — Retain and rescore Stockbit 4h rows

The user clarified that Stockbit signal rows must stay visible and their score
must be recalculated from the forming 4h bar on every sync. This decision
supersedes the earlier completed-bar and two-hour screener expiry decisions for
Stockbit regular IDX 4h rows only. Indicator alert history and every other
screener scope keep their existing lifecycle.

1. **Direct effect:** A successful scan updates a matching Stockbit 4h row in
   `QUANT_SIGNAL_DB` from the current same-source 15m-to-4h rollup. The row stays
   in SQLite under its existing signal identity. Acceptance: after a sync, its
   score bar time, price, score, components, and status reflect that scan.
2. **Immediate consequence:** The current forming-bar score is provisional and
   can change until the 4h candle closes. If the latest model output has no
   qualifying direction, keep the prior row and direction as history, set its
   score to 0, and label it `not_qualifying`.
3. **Response:** Each successful sync reevaluates the forming bar. A qualifying
   result updates the row; a no-setup result updates its score state. This does
   not extend the lifecycle of other signals.
4. **Interactions:** Build the forming 4h candle only from the selected 15m
   source when it matches the selected stored history source. Keep app,
   product, category, strategy IDs, data source/provider, and research-proxy
   quality labels intact. Alerts continue to use completed bars.
5. **Accumulation:** The existing SQLite primary key keeps one current row per
   Stockbit symbol and strategy. Successful scans update rows transactionally;
   no history cache, new file, or browser financial state is added.
6. **Failure boundaries:** If 15m input is missing, stale, malformed, or from
   a different source, do not invent a score; retain the stored score and mark
   it stale. A failed scan also keeps the last good snapshot and marks its
   Stockbit score rows stale. A scan that succeeds but lacks enough history
   leaves that row stale.
7. **Recovery:** Show provisional, no-current-setup, and stale states with the
   score-bar age and keep the normal retry path. A later fresh successful sync
   replaces the stale label with the new model result.
8. **Governance:** This behavior is explicitly requested by the user. It
   remains informational research ranking; it adds no account, position,
   execution, paper-trading, or order state.
9. **Trajectory:** Hypothesis: retaining and rescoring rows may make changing
   intrabar model state easier to inspect. It does not establish predictive
   quality; revisit only if UI evidence shows users confuse provisional scores
   or stored direction with a current qualifying setup.
10. **Decision:** Preserve the existing database and row identity, update the
    score fields on each fresh scan, and hold old scores as stale when inputs
    are unavailable. Reconsider if a fresh same-source forming bar fails to
    advance the score-bar time or if a non-Stockbit signal gains this retention
    behavior.

## 2026-10-09 — Score Stockbit NWE/DSS independently by timeframe

The user requested Stockbit NWE/DSS models on every configured screener
timeframe and confirmed that each timeframe must have its own signal. This
extends the previous 4h-only retention decision to 15m, 1h, 2h, 4h, 1d, and 1w
for these models. Other strategies and indicator-alert history keep their
existing lifecycles.

1. **Direct effect:** A successful sync calculates Stockbit NWE and DSS scores
   from each timeframe's own bar series and stores them under distinct
   timeframe/strategy identities in the existing signal SQLite database.
   Acceptance: the six timeframe filters can each return their own row and
   score for a symbol without replacing or reusing another timeframe's row.
2. **Immediate consequence:** Each score describes its own interval. A current
   forming bar is provisional; its score can change before close.
3. **Response:** Every successful sync updates the matching symbol, strategy,
   and timeframe row. A non-qualifying model output sets only that row's score
   to zero and state to `not_qualifying`; other timeframes keep their own
   results.
4. **Interactions:** Build larger current bars from fresh 15m candles only
   when the selected source matches that timeframe's stored history. Preserve
   product, category, source, provider, and research-proxy quality labels.
   Indicator alert history continues to use completed bars.
5. **Accumulation:** Keep one current row per symbol, model, and timeframe in
   `QUANT_SIGNAL_DB`; add no database file, schema migration, or browser
   financial state.
6. **Failure boundaries:** Missing, stale, malformed, insufficient, or
   different-source input does not produce a new score. Retain the previous
   score for only the affected timeframe and mark it stale. A failed scan marks
   retained rows stale without deleting them.
7. **Recovery:** A later fresh sync refreshes that timeframe's row and clears
   its stale state. Show the timeframe, provisional/closed state, score-bar
   age, and data provenance so a user can identify which interval recovered.
8. **Governance:** This behavior is explicitly requested by the user and stays
   within informational research ranking. It does not add execution, account,
   position, paper-trading, or order state.
9. **Trajectory:** Hypothesis: independent interval scores may make
   cross-timeframe disagreement visible. That disagreement does not establish
   strategy quality; reconsider only if observed rows reuse another
   timeframe's score or create misleading duplicates.
10. **Decision:** Reuse the existing NWE/DSS scoring rules with each
    timeframe's own bars, unique strategy identity, and row key. Keep shared
    thresholds because the scoring rule is shared. Revisit only if validated
    evidence calls for timeframe-specific thresholds or if a fresh same-source
    bar fails to advance its own score-bar time.

## 2026-10-10 — Add NWE and DSS Bressert score components

The user requested signals from NWE and DSS Bressert by timeframe, with each
indicator contributing up to 10 points, a combined total up to 20, and a
minimum render score of 1.

1. **Direct effect:** The NWE and DSS components each score from 0 to 10 and
   sum to 0–20. Render qualifying rows at 1/20 or higher. Acceptance: every
   current row shows the correct total and both component scales.
2. **Immediate consequence:** A score represents additive indicator evidence;
   one component can qualify a row when the other contributes zero. It remains
   a ranking score, not a probability or validation result.
3. **Response:** Users can compare the NWE and DSS components and inspect the
   score's timeframe without interpreting the total as a confidence percent.
4. **Interactions:** Keep one row identity per app, product, category, ticker,
   strategy, and timeframe. Preserve source and quality provenance; leave
   unrelated 0–10 strategies and execution boundaries unchanged.
5. **Accumulation:** Version the NWE/DSS score models. Keep retained scores on
   their original 0–10 scale and mark them legacy/stale until a fresh v2 score
   replaces them.
6. **Failure boundaries:** Reject non-finite, negative, over-cap, or
   non-additive components. Missing evidence contributes zero only when the
   score contract identifies that component as unavailable. Failed scans keep
   the last valid row stale.
7. **Recovery:** Require a stopped-service, explicit SQLite migration to widen
   the table constraint. It validates copied values and counts, records source
   provenance, and can be safely rerun if migration does not complete.
8. **Governance:** This score change is user-authorized and stays within
   informational ranking. It adds no account, order, trade-history, or
   paper-trading state.
9. **Trajectory:** Hypothesis: showing component scores may make the evidence
   behind a rank easier to inspect. Reconsider if the UI suggests a score is a
   probability or if component totals fail to match stored totals.
10. **Decision:** Keep the change in the existing strategy, catalog, signal
    storage, and UI boundaries. Verify the minimum, maximum, additive total,
    independent timeframe rows, and retained legacy scale in a fresh browser
    session. Revisit if any layer disagrees about the row's scale or identity.
