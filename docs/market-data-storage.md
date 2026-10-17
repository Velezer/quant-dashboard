# Persistent market history

SQLite is the only durable application-data store. `QUANT_APP_DB` defaults to
`data/app/app.sqlite3` and owns reference catalogs, registered instruments,
plans, evidence, listing status, scheduler cursors, signal-alert history and
indicator-model settings, migration records, and app metadata.
`QUANT_SIGNAL_DB` defaults to `data/signals/signals.sqlite3` and owns active
signal-screener rows and scan status. A successful signal scan transactionally
replaces rows other than Stockbit regular IDX NWE/DSS model rows for the
configured screener timeframes. Each retained row has its own timeframe and
strategy identity and updates from that timeframe's fresh same-source candle on
every sync. If those inputs are unavailable, the last score remains visible
and is marked stale. A failed scan keeps the last-good state and records an
error status. Other screener rows keep their existing two-hour signal
lifecycle; alert history also keeps its two-hour expiry.
`QUANT_MARKET_DIR` defaults to `data/market/tickers` and contains one SQLite
history database per exact storage symbol. Each file owns that symbol's
canonical `bars`, provenance `series`, and `ingestion_runs`. File names use a
reversible lower-case hex encoding of the symbol bytes; the app database keeps
the inventory and migration records. `QUANT_MARKET_DB` defaults to
`data/market/market.sqlite3` and is used only as an explicit offline migration
source. It is not opened by normal runtime reads or writes.

`data/` may contain the app database, signal database, per-symbol market
databases, and their SQLite-managed journal/WAL files. Do not add another
persistence system, file-backed catalogs, manifests, caches, exports, or runtime
state. Browser storage holds only disposable presentation preferences. The
dashboard remains read-only with respect to execution.

## Decision record: dedicated active signal snapshot database

The user approved `data/signals/signals.sqlite3` for current screener rows and
scan status. Alert history and indicator-model settings remain in the app
database. The current snapshot is recalculated and replaced on a successful
scan; it is not an append-only signal history.

1. **Direct effect:** Current signal rows and their scan status move from the
   app database into `QUANT_SIGNAL_DB`. Acceptance: `/api/signals` reads the
   latest committed snapshot from this file, and each successful scan replaces
   all active rows and refreshes its timestamp in one transaction.
2. **Immediate consequence:** Signal reads and writes use the dedicated store;
   catalogs, registered instruments, alert history, and indicator-model
   settings continue using `QUANT_APP_DB`.
3. **Response:** Repeated successful scans replace the snapshot instead of
   growing it. A failed scan preserves the prior rows and updates status so the
   dashboard can show stale/error recovery.
4. **Interactions:** The scanner still reads registered instruments and
   strategy metadata from the app database and market bars from ticker
   databases. No market-data or execution ownership changes.
5. **Accumulation:** The existing row bound and two-hour expiry continue to
   bound active signal state. The one-time migration records source hash and
   row counts in `QUANT_APP_DB`.
6. **Failure boundaries:** Startup refuses a legacy app-database snapshot until
   the maintainer runs the explicit migration. An interrupted copy leaves the
   app source intact; rerunning recopies idempotently, validates rows and
   values, then retires the old tables.
7. **Recovery:** The migration command validates the destination before
   dropping source tables. A failed or incomplete migration can be rerun while
   the service is stopped; runtime never falls back to app-database rows.
8. **Governance:** The user explicitly approved this dedicated signal database
   exception. `QUANT_SIGNAL_DB` is the only additional active database; all
   other storage boundaries remain unchanged.
9. **Trajectory:** Hypothesis: isolating a small, bounded active snapshot may
   reduce contention with catalog and alert writes. Reconsider if scan commits,
   API freshness, or the 16 MB memory target regress.
10. **Decision:** Keep the change at the existing SQLite composition boundary,
    migrate the active snapshot explicitly, and preserve alert-history
    semantics. Reconsider if the API reads a different snapshot than the last
    successful scan or if repeated scans accumulate old active rows.

For an existing app database with `signal_screener_rows` or
`signal_screener_status`, stop the service and run
`tooling/migrate-signal-screener.bat` before starting the updated service.
Fresh databases create the signal schema directly; startup never migrates old
rows implicitly.

For an existing `QUANT_SIGNAL_DB` with the prior NWE/DSS 0–10 score constraint,
stop the service and run `tooling/migrate-signal-score-scale.bat`. The
transactional rebuild validates the copied row count and every stored value
outside `score_basis_json`, records the source hash and row counts, and widens
the score constraint to 0–20. It preserves old scores and model IDs as stale
legacy 0–10 evidence, including their former 70/30 score basis. Rerunning the
command after completion is safe. Startup never runs this migration implicitly.

## Explicit legacy migration

Stop the service. If the shared legacy database still contains app-owned tables,
run `tooling/split-market-database.bat` or
`cargo run --release -p rust_chart_api -- --split-market-database` first. This
copies app-owned tables transactionally, compares row counts and stored values,
records the source hash and per-table counts in SQLite, and drops legacy app
tables only after validation succeeds.

Then run `tooling/shard-market-database.bat` or
`cargo run --release -p rust_chart_api -- --shard-market-database`. The sharder
copies each market table by exact `storage_symbol` to its ticker database. It
records a deterministic source content hash, per-table and per-symbol counts,
and transactional checkpoints in the app database. It validates destination
counts and stored values before retiring the source database and its SQLite
sidecars. Interrupted runs resume only from checkpoints whose hashes and
contents still validate. Repeated runs detect completion without duplicating
data. Normal startup refuses an incomplete app layout or a remaining shared
market source; conversion never runs implicitly. The sharder also attaches and
validates `QUANT_SIGNAL_DB` while preparing the app schema (default
`data/signals/signals.sqlite3`) and rejects placing it inside
`QUANT_MARKET_DIR`, which is reserved for per-symbol history databases.

## Runtime behavior and provenance

Canonical chart history lives only in SQLite `bars`. Keep source, provider,
exchange, product, and timeframe provenance in `series` and `ingestion_runs`.
Each ticker database contains history for one exact storage symbol, while
multiple products with that symbol retain separate series/source identities.
Missing history stays missing; charts must not fabricate or forward-fill
candles.

Research-proxy candles remain visibly labeled and cannot count as venue-native
execution evidence. Hourly candle sync selects active registered instruments
independently of exchange. Tokocrypto uses its bounded public provider adapter;
other registered instruments may use bounded research-proxy candles until
native adapters are available. Never register an instrument from candle data
alone. Chart and evidence reads stay bounded to preserve the 16 MB steady-state
memory target.

Required reference data is seeded or regathered only by an explicit documented
command. Seeding is idempotent, preserves provenance, and validates
completeness. Reference catalog records live in SQLite as JSON text; do not add
file-backed application-data readers. Browser/server structured messages use
JSON with `application/json`. The service does not persist raw provider payloads
or forward raw upstream JSON or XML. CBOR remains prohibited except for an
explicit read-only legacy migration. The service does not read or write
file-backed runtime data.
