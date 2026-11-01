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
  observable.
- A market provider's successful request or `last_success` timestamp does not
  prove the final candle was received. For IDX, the observed final 15m bar
  starts at 16:00 WIB and closes at 16:15. Check that stored bar's timestamp
  and OHLCV, then checkpoint only after its scan batch publishes. Keep missing
  final data retryable.
- Stockbit's higher-timeframe signal path already rolls intraday candles up
  from 15-minute source bars. Crypto higher-timeframe scores can use a forming
  rollup when marked as forming; 15m crypto signals remain on completed bars.
  General chart reads select a stored interval when available, so rollups must
  preserve source consistency and IDX session boundaries.
- A scan's global snapshot publication can remove rows for instruments that
  were intentionally skipped. Carry the preserved instrument identities into
  final publication and mark closed-market rows historical.
- Check aggregate readiness merges: replacing a product entry with a worker
  fragment undercounts instruments. Add counts and keep examples bounded.
- Use GitHub Actions to compile and test changes when the workspace cannot run
  commands. Inspect the exact failing step and rerun after the fix.
