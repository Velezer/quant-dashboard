# Task experience

Keep this file short and reusable. Read it before repository work and append a
lesson only when it will improve future tasks.

## Verified execution lessons

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
- Calculate chart overlays from the exact candle source rendered on screen.
  Shared Spot reference charts for Futures must not receive index-aligned
  indicators calculated from Futures-specific candles; return overlays with
  the chart bundle and preserve them during later indicator hydration.
- Use GitHub Actions to compile and test changes when the workspace cannot run
  commands. Inspect the exact failing step and rerun after the fix.
- Apply screener filters before pagination and compute filtered match counts on
  the server; a browser-only filter can make a matching signal appear absent
  from the first page.
- Normalize an overall score filter by each signal model's declared maximum and
  apply it before pagination on both server and browser. State the displayed
  normalization clearly when models use /10, /20, and /40 scales.
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
- A daily all-ticker backtest must be sequential, resumable, and visible on its
  research page. Persist its ticker/timeframe cursor and status, share the lock
  with manual runs, yield to signal scans, and keep learned candidates separate
  from live weights unless a promotion rule is explicitly authorized.
- When an authorized promotion path moves learned scoring weights into live
  scans, persist immutable profile versions and pin one profile snapshot per
  scan. Stamp signal output with that version so a promotion cannot mix scores
  within a scan and prior results remain reproducible.
- Mobile screener overrides must be checked after `signals-workspace-active`
  is applied: higher-specificity terminal rules can keep the header in one row,
  expand the layout viewport, and move a fixed app/timeframe dock off-screen.
  Verify no horizontal document overflow at 320px and 390px, exercise both
  filter rows and disclosures, and inspect the selected-chart state as well as
  the empty/loading view.
