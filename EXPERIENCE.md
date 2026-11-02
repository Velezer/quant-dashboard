# Task experience

Keep this file short and reusable. Read it before repository work and append a
lesson only when it will improve future tasks.

## Verified execution lessons

- The local execution environment may be unavailable while the GitHub connector
  still works. Use repository file and GitHub Actions tools for edits and
  verification; don't claim a local build ran when it did not.
- The Rust signal scan reserves two database connections. Worker count must be
  based on queued tickers, not the number of nonempty apps, or one large app
  silently scans serially. The progress API previously exposed one
  `current_scan_symbol`; show all active ticker labels to make concurrency
  observable. The dashboard's status poll is coarse, so emit a bounded,
  throttled SSE progress event when parallel workers are active.
- A market provider's successful request or `last_success` timestamp does not
  prove the final candle was received. For IDX, the observed final 15m bar
  starts at 16:00 WIB and closes at 16:15. Check that stored bar's timestamp
  and OHLCV, then checkpoint only after its scan batch publishes. Keep missing
  final data retryable.
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
- Crypto NWE/DSS setups can remain visible after their reference price expires,
  but they are not current market entries. Keep the API and browser filters aligned
  with the crypto strategy catalog, preserve the stale-price label, and keep
  stale setups out of current-entry notification counts.
- Use GitHub Actions to compile and test changes when the workspace cannot run
  commands. Inspect the exact failing step and rerun after the fix.
- Apply screener filters before pagination and compute filtered match counts on
  the server; a browser-only filter can make a matching signal appear absent
  from the first page.
- Keep fresh Stockbit score snapshots with `not_qualifying` visible as
  zero-score, no-entry rows, while excluding them from current-entry and
  notification counts. The server and browser must use the same rule.
- Give a selected score-indicator filter a useful default threshold; `Any /10`
  is a no-op when every signal row has a score for that indicator. Explain how
  users can widen the results again.
