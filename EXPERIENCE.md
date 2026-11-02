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
- Calculate chart overlays from the exact candle source rendered on screen.
  Shared Spot reference charts for Futures must not receive index-aligned
  indicators calculated from Futures-specific candles; return overlays with
  the chart bundle and preserve them during later indicator hydration.
- Use GitHub Actions to compile and test changes when the workspace cannot run
  commands. Inspect the exact failing step and rerun after the fix.
