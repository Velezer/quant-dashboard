# Task experience

Keep this file short and reusable. Read it before repository work and append a
lesson only when it will improve future tasks.

## Verified execution lessons

- The local execution environment may be unavailable while the GitHub connector
  still works. Use repository file and GitHub Actions tools for edits and
  verification; don't claim a local build ran when it did not.
- The Rust signal scan already reserves two database connections and runs two
  scan threads. The UI previously exposed only one `current_scan_symbol`, so
  a single displayed ticker did not prove serial execution. Track active scan
  labels separately from completed progress.
- A market provider's successful request or `last_success` timestamp does not
  prove the final candle was received. Check the actual completed bar timestamp
  and its OHLCV; keep missing final data retryable.
- Stockbit's higher-timeframe signal path already rolls intraday candles up
  from 15-minute source bars. General chart reads select a stored interval when
  available, so changing scan rollups must preserve source consistency and
  IDX session boundaries.
- A scan's global snapshot publication can remove rows for instruments that
  were intentionally skipped. Carry the preserved instrument identities into
  final publication and mark closed-market rows historical.
- Check aggregate readiness merges: replacing a product entry with a worker
  fragment undercounts instruments. Add counts and keep examples bounded.
- Use GitHub Actions to compile and test changes when the workspace cannot run
  commands. Inspect the exact failing step and rerun after the fix.
