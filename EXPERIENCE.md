# Task experience

Keep this file short and reusable. Read it before repository work and append a
lesson only when it will improve future tasks.

## Verified execution lessons

- The Rust build workflow previously skipped frontend-only changes because its
  path filters named only backend and Rust sources. Keep `frontend/**` in both
  pull-request and branch-push filters, and confirm the release build check ran
  before treating a code PR as verified; GitGuardian alone is not a build.
- The main chart's date scale is rendered inside the canvas' bottom 32 CSS pixels.
  Keep its drag hit target and anchor calculation aligned with the plot bounds
  (15 px left, 96 px right). Verify a date-axis drag changes visible bar count
  while keeping the candle under the pointer anchored; right-axis price scaling
  remains a separate vertical drag.
- The local execution environment may be unavailable while the GitHub connector
  still works. Use repository file and GitHub Actions tools for edits and
  verification; don't claim a local build ran when it did not.
- The Rust signal scan reserves four database connections. Keep the worker
  count, scan reservation, and total open-database cap aligned; worker count
  must be based on queued tickers, not nonempty apps, or one large app silently
  scans serially. The progress API previously exposed one
  `current_scan_symbol`; show all active ticker labels to make concurrency
  observable. The dashboard's status poll is coarse, so emit a bounded,
  throttled SSE progress event when parallel workers are active.
- A market provider's successful request or `last_success` timestamp does not
  prove the final candle was received. For IDX, the observed final 15m bar
  starts at 16:00 WIB and closes at 16:15. Check that stored bar's timestamp
  and OHLCV, then checkpoint only after its scan batch publishes. Keep missing
  final data retryable. Include successful per-timeframe series refreshes in
  the close-scan fingerprint too: an unchanged final 15m candle can have new
  older or higher-timeframe history behind it, which changes NWE/DSS scoring.
- An exact IDX close checkpoint is a deduplication aid, not a prerequisite for
  displaying scores. If the provider omits the final bar, rescore the latest
  completed stored 15m close after hours and mark it no-market-entry.
- The smallest signal timeframe is 15m. Roll higher timeframes from stored
  15m bars for recent buckets so they can be rescored on every scan, and mark a
  still-forming bucket explicitly. Keep older higher-timeframe history coarse
  to bound scan work. Preserve source consistency and anchor IDX buckets to its
  trading session rather than UTC.
- A scan's global snapshot publication can remove rows for instruments that
  were intentionally skipped. Carry the preserved instrument identities into
  final publication and mark closed-market rows historical.
- Check aggregate readiness merges: replacing a product entry with a worker
  fragment undercounts instruments. Add counts and keep examples bounded.
- The candlestick canvas separates its price plot from the volume pane.
  Clip price overlays to the visible plot bounds: vertical zoom can move an
  indicator band out of view, and an unclipped band can bleed into volume.
- Separate-pane indicators need scales in their own units. In particular,
  NWE excursion percentages are often below 1; using the DSS oscillator's
  fixed 0–100 range makes 15m excursions look flat. Fit and label the percent
  axis from the selected NWE output while keeping oscillator scales unchanged.
- Crypto NWE/DSS setups can remain visible after their reference price expires,
  but they are not current market entries. Keep the API and browser filters aligned
  with the crypto strategy catalog, preserve the stale-price label, and keep
  stale setups out of current-entry notification counts.
- A crypto setup was being dropped before screener persistence when its
  same-source strategy candles qualified but the separately fetched 15m price
  was absent or source-mismatched. Use that setup candle's close as an
  explicitly reference-only price, retain the setup, and exclude it from
  market-entry and alert counts. Never invent a directional signal or bypass
  the model floor to force a nonempty crypto screener.
- Calculate chart overlays from the exact candle source rendered on screen.
  Shared Spot reference charts for Futures must not receive index-aligned
  indicators calculated from Futures-specific candles; return overlays with
  the chart bundle and preserve them during later indicator hydration.
- Before opening a code PR, verify both locked release builds:
  `cargo build --locked --release --target wasm32-unknown-unknown -p quant_pine_runtime`
  and `cargo build --locked --release -p rust_chart_api`. If Cargo is unavailable,
  use the Rust build workflow and wait for both steps to finish successfully.
  Distinguish warnings from errors, and never call an in-progress, skipped, or
  partially completed workflow a verified build. Run tests only when the user
  asks to test or verify implementation.
- Treat compiler warnings in release-build output as cleanup work. Remove
  unused imports and parameters, and put helpers used only by unit tests behind
  `#[cfg(test)]`; do not silence warning families with crate-wide allow
  attributes. Recheck both release build steps after cleanup and confirm the
  resulting warning list is gone or explicitly understood before marking the PR
  verified.
- Apply screener filters before pagination and compute filtered match counts on
  the server; a browser-only filter can make a matching signal appear absent
  from the first page.
- Normalize an overall score filter by each signal model's declared maximum and
  apply it before pagination on both server and browser. State the displayed
  normalization clearly when models use /10, /20, /40, and /50 scales.
- Keep fresh Stockbit score snapshots with `not_qualifying` visible as
  zero-score, no-entry rows, while excluding them from current-entry and
  notification counts. The server and browser must use the same rule.
- Give a selected score-indicator filter a useful default threshold; `Any /10`
  is a no-op when every signal row has a score for that indicator. Explain how
  users can widen the results again.
- Describe fee-adjusted breakeven as an estimated exit price, never as a
  risk-free trade. Apply entry and exit fees to their directional transaction
  sides, keep unknown fees unavailable until both are supplied, and disclose
  excluded execution costs and price-step rounding. A stop moved to that level
  still may not fill there.
- For entry-score research, retain raw causal factor observations before applying
  the composite floor so alternate weights can change qualification. Fit bounded
  weights on training data, select exits and WIB hour pairs on validation, and
  keep the chronological holdout read-only. Report score and consecutive-trade
  return autocorrelation with pair counts; it describes dependence, not accuracy.
- Apply the 1,000-day validation minimum to the full chronological history, not
  the 20% holdout tail. Requiring 1,000 days inside that tail would require about
  fourteen years of stored data and exceed the bounded 100,000-candle report
  read; keep the holdout's trade-count requirement as a separate check. Treat
  1,000 days as a minimum, never a maximum history horizon; use all completed
  candles in the report sample and disclose the separate candle-read cap.
- A daily all-ticker backtest must be sequential, resumable, controllable, and
  visible on its research page. Provide run-now, pause-after-current, and resume
  controls there; persist its ticker/timeframe cursor and controls, wake the
  worker promptly after a request, share the lock with manual runs, yield to
  signal scans, and keep learned candidates separate from live weights unless
  a promotion rule is explicitly authorized.
- Keep the stored-history target separate from the evidence sample requirement.
  For multi-year crypto backfills, advance one bounded page from the oldest
  saved timestamp and run the small latest-candle refresh independently. A
  recent-success shortcut must still allow historical pages while coverage is
  below target; recent candles alone never mean historical coverage is complete.
- Before raising the per-report candle cap, remove repeated prefix replay and
  per-bar heap-heavy signal objects. Reuse causal indicator arrays and visit
  directional candidates so the longer sample stays within the process memory
  budget.
- After-fee profile promotion is separate from live signal eligibility. When a
  screener is empty, check directional setups, the base score floor, candle and
  price inputs first; do not hide a setup because its cohort lacks a promoted
  profile or misses a historical score cutoff. Use equal weights when no
  profile is active and report validation/holdout percentages as informational
  metrics. Keep the 50% evidence threshold for profile promotion.
- When an authorized promotion path moves learned scoring weights into live
  scans, persist immutable profile versions and pin one profile snapshot per
  scan. Stamp signal output with that version so a promotion cannot mix scores
  within a scan and prior results remain reproducible.
- A composite score is setup strength, not an individual win probability.
  Retain the model's base score floor for signals. Calibrated after-fee
  thresholds are historical metrics used to evaluate profile promotion, not
  filters on signal creation, screener listing, or notification counts. Show
  validation/holdout percentages and explain when a cohort has no promoted
  profile.
- Mobile screener filters must not sit fixed over signal rows or readiness
  recovery actions. Keep a collapsed, keyboard-accessible summary of the active
  app/product/timeframe; expand the controls in document flow and keep the
  selection visible after closing. Verify app, product, and timeframe changes,
  Escape handling, no horizontal overflow at 320px and 390px, and the selected
  chart as well as empty/loading states after workspace CSS is applied.
- When a scoring model gains a component, carry its model version and scale
  through the live scorer, chronological backtest, learned-profile gate,
  catalog, stored score basis, screener/API validation, filters, and guide. Keep
  old rows on their recorded scale and mark them stale/legacy. For Markov input,
  use the causal regime at the scored bar and document its fixed contribution;
  do not treat whole-history transition statistics as a signal-time forecast.
- Keep the wide-screen dashboard's app, product, timeframe, search, and score
  controls in the screener rail, with one scrollable signal list and a visible
  availability summary. Hide desktop navigation at phone widths and verify the
  selected chart at both a 1600×857 view and a 1280×720 view. When reading
  persisted `URLSearchParams`, treat a missing value as missing before numeric
  conversion: `Number(null)` is zero and can silently clamp a layout preference
  to its minimum.
- Show the current Markov contribution beside the screener total as well as in
  the expanded five-factor breakdown. Normalize component identities so a
  display label and a stable component ID resolve to the same indicator. For a
  retained legacy row, say that Markov was not scored instead of assigning a
  fabricated regime value.
- Write HTTP headers and an existing response body separately. Concatenating
  them with `format!` duplicates every large chart/API payload at peak; concurrent
  request handlers multiply that avoidable memory spike.
- Stream lower-timeframe rows into bounded rollup output. A 100,000-candle
  source cap can otherwise become a large per-request allocation when many
  higher-timeframe chart requests arrive together.
- Measure compiler RSS before claiming builder-memory improvement. With one Cargo
  job, LTO off and sixteen codegen units reduced service compiler peak 37.4%
  versus thin LTO/one unit in clean builds; the binary grew 11.4%. Compiler RSS,
  Blitz whole-builder peak, and running-service memory are separate metrics.
- Exercise the built Docker image's defaults on first boot and reused storage.
  EXPOSE does not set the service's listener; the image must default PORT to 8080.
- Probe default WAL before ATTACH. On exact SQLite error 4874, close the failed
  probe before exclusive recovery through a fresh connection; preserve committed
  WAL and cache the effective journal per path. Matching-mode connections should
  not reassign journals while other workers hold transactions.
- BEGIN IMMEDIATE reserves writers on all attached databases. For main-only
  candle persistence, resolve app metadata before a deferred transaction and
  qualify main writes, so unrelated tickers do not compete for the shared app
  writer. Retain immediate transactions where multiple stores are mutated.
- Signals-only snapshot transactions must reserve the signals writer before
  reads inside a deferred transaction; a no-op signals write avoids both an
  unrelated app writer lock and a later WAL read-to-write snapshot upgrade.
- App-only registry and profile transactions should start deferred; the first
  app write owns the app lock without reserving the attached signal store.
  Retry transient busy/locked errors at the market-history read boundary and
  after a failed, rolled-back provider write so temporary contention does not
  become a false missing-history result.
- IDX Stockbit registration must use the paginated `GetCompanyProfiles` JSON
  endpoint rather than assume the public HTML page embeds Nuxt state. Validate
  the declared total and each page before reconciliation, and queue an
  app-scoped rescan after a successful live registry refresh so a first-boot
  scan against an empty registry is not the only scan until the next interval.
- Market-history target selection must interleave due tickers by app after
  respecting explicit chart and stale-signal priorities. A global alphabetical
  order can let a larger app consume every bounded batch and leave another
  app's registered instruments without candles, making its signal scan
  correctly but uselessly report missing history.
- After a first Stockbit candle is stored on a fresh deployment, queue one
  Stockbit-only rescan. A successful registry refresh can happen before the
  bounded candle worker has ingested any prices, so a registry-only rescan still
  leaves the UI empty until the next periodic interval.
- Before drawing continuous-market crypto candles, check timestamp continuity.
  A short recent segment after a large history gap must not be index-spaced
  against old candles or inherit their NWE/Markov state. Label omitted stored
  bars, scale to the latest continuous segment, and keep absent oscillator
  values unavailable instead of plotting a neutral 50.
- A preferred-timeframe rule search must use a predeclared grid, select only on
  chronological validation data, and score its frozen choice once on holdout.
  Require the after-fee win rate and its 95% lower confidence bound, plus
  average net win/loss payoff and positive expectancy; show the measured
  evidence and missing coverage without claiming a rule was proven. A 4h
  preference must not block otherwise valid research at other timeframes.
- For asymmetric brackets, define target R as planned net reward divided by
  planned net stop loss after configured entry and exit fees. Solve the target
  price from the fee-adjusted stop, and separately gate the realized average
  net win/loss ratio because stop gaps and timed exits can change the observed
  payoff. Version this rule protocol and mark older gross-target reports for
  rerun without invalidating unrelated score-weight evidence.
