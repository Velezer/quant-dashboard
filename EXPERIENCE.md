# Task experience

Keep this file short and reusable. Read it before repository work and append a
lesson only when it will improve future tasks.

## Verified execution lessons

- NWE-led and DSS-led rows can carry the same signed signal for a ticker and
  timeframe. Select one before paginating the screener and use a stable
  instrument/timeframe selection key while retaining the selected model's
  evidence and its alert links.

- A 4h chart bucket can exist even when one of its hourly candles is missing.
  For a cross-sectional forecast, require four consecutive native hourly bars
  per bucket and the same completed cutoff across pairs. Reset history at
  gaps and keep stale or proxy series out of forecast coverage.

- A Pintu ticker-feed entry is not proof that its Futures product offers it.
  Keep explicit product exclusions separate from official delisting notices,
  enforce them during import, reconciliation, and saved-signal reads, and
  invalidate existing venue verification even when a live refresh fails.
  Preserve candle history and show the registry's product-unavailable reason.

- Stockbit same-timeframe stored history must remain scoreable when matching
  15m prices are absent. Use its candle close as an explicit reference-only
  price, retain current-model stale score rows in both API and browser, and
  exclude them from current-entry counts. Never roll a source using another
  source's 15m timestamp. Legacy models still require current-model rescoring.

- Keep screener cards compact: ticker/timeframe, app/product/model context,
  direction, signed score, price, win-rate availability, and stale/no-entry state.
  Put factor breakdowns and research diagnostics in a closed native details
  disclosure outside the chart-selection button so keyboard/touch users can
  inspect them without selecting or nesting interactive controls.

- A directional imbalance is not evidence of predictive quality. Expose bullish
  and bearish contribution totals and cancellation, and partition the same
  chronological holdout trade sequence by LONG/SHORT after configured fees.
  The combined entry score supersedes duplicate NWE-led and DSS-led rows.
  A warm-up difference does not establish an independent model. Version the
  screener identity and saved evidence protocol together when replacing the
  model, and compare replay with the live score on the same candle prefix.

- Hydrate chart indicators by exact candle timestamp rather than array offsets.
  Missing or newly appended bars can shift independently fetched indicator arrays.
  Cache identity must include candle times and OHLCV; equal row counts do not
  prove the same series or unchanged forming bars.

- Scope chart presentation changes to the chart column, preserve all existing
  control IDs and dynamic OHLC color classes, and review chart tools while open
  on narrow screens. A toolbar can look tidy while its popover still widens the
  whole phone page.

- Signed entry scoring must start with independent bullish/bearish factor
  observations: each raw component is −10…+10, and weighted contributions sum
  to the signed total (−60…+60 under the six-factor budget). Positive totals
  determine LONG, negative totals SHORT, and zero is neutral. Never assign a
  fixed ±50 after first choosing a side, or clip opposing factors to zero.
  Apply the eligibility floor and strength filters to the total's magnitude.
  Live scans and backtests must use the same observations and derive direction
  again for every candidate weight vector. Version model/protocol identities
  so old directional-support profiles cannot enter the signed model.
- Keep signed component values, weighted contributions, total, source, and
  freshness visible together in the screener and selected chart. A stale row
  keeps its last recorded signed calculation and no-current-entry warning;
  closed-market Stockbit references never become current entries. Historical
  win rates and raw correlation ρ remain separate metrics.
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
- Carry registered app and product identity through chart and regime history
  reads. Use the timeframe-aware series loader for both candles and freshness
  metadata; generic ticker reads can cross product sources or omit valid
  derived intervals. For 4h, compare the latest hourly rollup bucket with the
  direct 4h tail before choosing hourly history; old hourly bars must not hide
  newer direct bars.
- A selected-signal setup view should reuse the row's after-fee holdout evidence
  and strategy metadata, while price zones come from that signal's own
  timeframe chart bundle. Label NWE proximity zones as contextual references,
  show both directions, and keep an explicit unavailable state when endpoint
  bands are missing; do not fabricate a win rate or entry price.
- A crypto chart may be display-ready while its latest continuous segment is
  too short for an indicator-dependent setup. Put the history-repair action
  beside the unavailable setup zone and show the contiguous-bar count and
  indicator warmup needed; the chart's error overlay alone will not appear when
  a few recent bars are drawable.
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
  normalization clearly when models use /10, /20, /40, /50, and /60 scales;
  keep retired `/50` rows on their stored scale when the current model moves to
  `/60`.
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
  keep the chronological holdout read-only. Report lagged, strategy-aligned
  return autocorrelation with pair counts on training data, comparing the
  equal-weight baseline with any training-fitted candidate. A negative lag is a
  reversal clue between separate outcomes, not a position-flip instruction or
  evidence of accuracy; any reverse-direction rule needs its own validation and
  untouched holdout evaluation.
- Entry-validation reports must derive `stored_coverage_days` from indexed
  MIN/MAX bounds on the exact source interval used by the backtest. Chart
  metadata is deliberately display-bounded and can understate multi-year
  research history; keep it separate from the complete sampled span.
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
  When several app products share a crypto source, assign one registered owner
  to advance each interval's history and let aliases reuse its fresh bars; a
  fresh peer price must not suppress the owner's next page.
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
- A daily entry-validation cursor can pass a ticker before gradual candle
  ingestion reaches it. When sync adds rows, enqueue a deduplicated recheck for
  saved `NO_HISTORY` reports in that exact instrument and shared crypto
  category; let the single backtest worker drain it between scan pairs and
  expose the pending count. Any successful manual or scheduled rerun clears its
  queue row.
- Progressive ingestion must also recheck saved `BACKTEST_COMPLETE` or
  `VALIDATION_PENDING` reports when the indexed source span reaches the full
  history minimum. Do not use chart metadata for this threshold: its 1,000-bar
  display cap can make a derived 4h series look shorter than 1,000 days. Check
  the report reader's selected source and storage interval with indexed
  `MIN(ts)`/`MAX(ts)` bounds, allow for incomplete aggregation buckets, and
  enqueue only matching timeframes whose saved report is still below the
  evidence minimum.
- Keep entry-validation list summaries on explicit after-configured-fee fields.
  Unknown fee models must render after-fee win rate, net payoff, and net return
  as unavailable; overlay these fields from saved report JSON so older compact
  summaries cannot keep showing a gross holdout rate under an after-fee target
  label.
- The asymmetric after-fee rule is stored at
  `backtest.weight_learning.risk_reward_rule`. Keep report qualification,
  compact list summaries, and UI rendering on that canonical path. Existing
  compact summaries can outlive a serializer fix, so project only the needed
  rule fields from saved JSON on list reads and gate 4h validated counts on the
  current qualified rule.
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
  For a live autocorrelation factor, calculate same-timeframe returns only
  through the scored bar, expose its minimum pair count and unavailable state,
  and carry its fixed weight through backtest observations, profile protocol,
  score basis, filters, and the guide.
- Keep the wide-screen dashboard's app, product, timeframe, search, and score
  controls in the screener rail, with one scrollable signal list and a visible
  availability summary. Hide desktop navigation at phone widths and verify the
  selected chart at both a 1600×857 view and a 1280×720 view. When reading
  persisted `URLSearchParams`, treat a missing value as missing before numeric
  conversion: `Number(null)` is zero and can silently clamp a layout preference
  to its minimum.
- Keep the desktop signal-availability state visible in a compact scan-info
  control and float its detailed recovery panel above the signal list. Keep the
  full panel in document flow on phones. Verify the list's real `clientHeight`
  and wheel and Page Down behavior with signal rows; `overflow-y: auto` on a
  near-zero region is still unusable.
- Show current fixed-indicator contributions beside the screener total as well
  as in the expanded six-factor breakdown. Normalize component identities so a
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
  the declared total and each page before reconciliation. Count complete
  source rows separately from usable tickers: IDX includes malformed test or
  non-trading identifiers such as `TEST-SHM`, and rejecting one must not erase
  the rest of a valid 900+ ticker universe. Queue an app-scoped rescan after a
  successful live registry refresh so a first-boot scan against an empty
  registry is not the only scan until the next interval.
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
- Chart-priority sync requests must carry the selected timeframe and requested
  history size. A fixed base-bar window can cover only a few days of 15m data,
  even when the chart needs hundreds of daily bars. Derive the source window
  from the chart/base interval and keep it within the provider's bounded page
  budget so the repair reaches the missing dates the user can see.
- A fresh latest candle does not prove that a chart's visible window and regime
  warm-up are complete. On chart priority, repair gaps in the exact source
  selected by the chart bundle (including its shared Spot source for Futures),
  and expand the base-source repair window for derived timeframes using the
  chart's aggregation rule. Then refresh the view without enqueueing the same
  priority sync again. Keep the continuity guard active when provider repair
  fails.
- A preferred-timeframe rule search must use a predeclared grid, select only on
  chronological validation data, and score its frozen choice once on holdout.
  The performance target is a 50% win rate after configured transaction fees;
  use its 95% lower confidence bound and sufficient samples to describe the
  strength of that evidence. Do not invent a minimum payoff ratio, profit
  factor, or positive-return target. Report those as diagnostics. A 4h
  preference must not block otherwise valid research at other timeframes.
- An absent after-fee win-rate can mean the product fee policy is unsupported,
  not that the strategy had zero wins. Check the exact app/product fee gate and
  its provenance before interpreting results. For market-entry research, use
  the published taker rate per side and keep funding, spread, slippage, and
  impact omissions explicit; equal rates across venues still need separate
  fee identities and source links.
- For asymmetric brackets, define target R as planned net reward divided by
  planned net stop loss after configured entry and exit fees. Solve the target
  price from the fee-adjusted stop. Show realized net win/loss ratio because
  stop gaps and timed exits can change observed payoff, but do not use it as a
  pass/fail gate unless the user supplies a payoff target. Version rule changes
  and mark older gross-target reports for rerun without invalidating unrelated
  score-weight evidence.
- Do not treat a Yahoo `range` preset as the full history horizon. IDX daily
  candles can be paged backward to `meta.firstTradeDate`; Yahoo rejects 15m
  requests older than 60 days and 60m requests older than 730 days. Keep those
  provider limits separate from the 1,000-day evidence minimum and from storage
  retention, and save pages incrementally.
- For position-risk sizing from return autocorrelation, estimate finite-horizon
  variance from contiguous causal candle returns and never let negative
  autocorrelation increase the risk budget. Compare fixed-risk and adjusted
  paths on validation and untouched holdout; keep any quantity calculator
  hypothetical, fee-aware, browser-local, and separate from live sizing.
- Show a signal's win estimate only when it matches the active model/profile,
  category, timeframe, source, and exact app/product fee calibration. Use
  after-fee holdout counts and uncertainty; distinguish score-band estimates
  from a profile-gate fallback, and say when evidence is insufficient instead
  of treating setup score as a probability.

- For chart workspace readability, keep setup evidence and momentum in native
  closed disclosures and preserve their existing IDs and toggle listeners.
  Check the final computed CSS: older ID-qualified grid rules can squeeze a
  new disclosure into one cell despite a later class rule. Verify selected
  mobile state explicitly (`signal-chart-selected`) at 320px and 390px.
- On chart timeframe switches, draw any cached candles before awaiting indicator
  hydration and show separate inline progress for candle history and studies.
  Keep one owner for hydration: if the chart bundle loader and click handler
  both hydrate, the second request can abort and restart the first. An empty
  timeframe array is not proof that a lazy chart bundle was fetched; request the
  bundle until it is marked loaded, then keep the chart's normal empty state.
- On app or screener-timeframe changes, rendering synchronously in a click
  handler does not guarantee a visible paint before fetch begins. Render the
  selected scope's loading state, yield through a frame and task, and abort the
  superseded request. Audit render hooks too: a panel's `show()` callback can
  start its own API call during that render. Push app/timeframe predicates into
  SQLite before per-row freshness work, while leaving product/search filtering
  after direction-score aggregation when those summaries must cover the whole
  ticker/timeframe. Do not add an all-app signal-count request solely for a
  notification badge; current screener rows and unread alert history are
  different concepts, and the alert panel can show its unread count from the
  alert-history response it already loads.

- Cross-sectional research must freeze the universe, cutoff and costs across
  retries, purge outcomes across common time splits, and deduplicate instrument
  identities. Use the causal strategy visitor with precomputed indicators for
  long samples, and verify past scores against live prefixes after appending
  future bars. Preserve quote-specific fee provenance and never infer listing
  dates from registration time or the oldest stored candle.

- During progressive signal scans, do not recalculate whole-screener counts on
  every ticker batch; preserve the last completed count and finalize it once
  when the app/full snapshot commits. Enrich a screener page's missing ticker
  holdout win rates with one set-based report query, not one JSON query per
  signal row. Keep price freshness and `NO_MARKET_ENTRY` semantics intact while
  optimizing this path.
- Keep server-recognized screener factor IDs in sync with every visible filter,
  and match persisted components by stable ID or normalized display label
  before pagination. Missing freshness, price coverage, or pagination evidence
  stays unavailable; validate a response before replacing the last good
  snapshot, and preserve explicit no-entry states through client-side expiry.
- Master-data app IDs and registry app keys can differ: Pintu's catalog resolves
  to `pintu_futures`, while its registered instruments use `pintu`. Convert at
  the registry boundary for selected-signal verification, app-scoped refresh,
  and listing queries; test against the stored app key.
- Readiness holdouts must come from the evaluation's exact `product_slices`
  entry for the selected exchange and product, with explicit fee provenance.
  The evaluation-wide holdout is pooled research evidence and cannot stand in
  for an app/product result. Keep the evaluation decision visible so a blocked
  rate is not presented as a promoted profile.
- Fail closed when a signal's lifecycle, market-entry, or score contract is
  unfamiliar. Indicator filters and contribution balances must use a numeric
  point and its valid weight together; an unavailable point must not become a
  zero or retain an orphaned weighted value.
