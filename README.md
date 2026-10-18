# Quant Trading Dashboard

A local Rust-backed dashboard for reviewing Stockbit IDX, Pintu Futures, and Tokocrypto plans, inspecting locally stored market charts, and reviewing strategy evidence. It has no simulated portfolio or order-placement API.

## Run locally

On Windows, run the canonical launcher:

```powershell
.\tooling\start.bat
```

The launcher builds the Rust service in release mode and opens the dashboard at [http://127.0.0.1:7777](http://127.0.0.1:7777). Run `tooling\harness.bat` for the project checks.

## Project layout

```text
designer/                     Design tokens, UI flows, and interaction guidance
frontend/
  dashboard/                  Dashboard HTML, CSS, and JavaScript
backend/
  service/                    Rust HTTP service and SQLite integration
quant/
  core/                       Strategies, indicators, risk, and evidence calculations
data/
  market/tickers/             One canonical history database per storage symbol
  app/app.sqlite3             Instruments, catalogs, plans, evidence, and metadata
  signals/signals.sqlite3     Current signal snapshot and scan status
docs/                         Product and engineering contracts
tooling/                      Canonical launcher and harness scripts
```

The Rust workspace contains `backend/service` and `quant/core`. The backend serves dashboard files from `frontend/dashboard` while retaining the existing `/web/...` asset URLs. Retired backtest URLs redirect to the live dashboard; plan cards read any historical evidence from SQLite.

## Railway deployment preparation

The repository includes a Docker build and `railway.toml`. Create a Railway service from this repository root and attach a persistent volume mounted at `/data`. The image listens on Railway's `PORT` and serves `/api/health` for the configured health check. The volume must retain all three SQLite stores; the container filesystem is disposable. The startup command initializes empty databases, while market history and registry data must be gathered or restored separately.

The service configuration sets these storage paths in the image:

```text
QUANT_MARKET_DIR=/data/market/tickers
QUANT_APP_DB=/data/app/app.sqlite3
QUANT_SIGNAL_DB=/data/signals/signals.sqlite3
```

The dashboard does not currently provide user authentication. Keep the Railway service private or put an authenticated access layer in front of it before assigning a public domain. The first deployment needs an empty persistent volume or a validated copy of the local SQLite stores; do not point the app database and signal database at the same file.

## Volume Delta study

The chart's **VOL Δ** control renders a per-bar volume delta estimate in the chart volume band. It classifies each selected chart candle's volume by comparing its close with the preceding chart candle's close (using the candle body for ties). This keeps the study available on the chart while making the data-resolution limit explicit. TradingView's lower-timeframe scan, custom timeframe selection, and expert-plan `1 tick` aggregation are not available in this dashboard runtime; the displayed estimate is not a parity implementation of `ta.requestVolumeDelta()`.

SQLite is the only durable application-data store. `QUANT_MARKET_DIR` (default `data/market/tickers`) holds one SQLite database per exact storage symbol, with its canonical `bars`, `series`, and `ingestion_runs`. `QUANT_APP_DB` (default `data/app/app.sqlite3`) holds catalogs, instruments, plans, evidence, listing status, scheduler cursors, signal-alert history, indicator-model settings, and app metadata. `QUANT_SIGNAL_DB` (default `data/signals/signals.sqlite3`) holds the active signal-screener snapshot and scan status; every successful scan replaces the previous snapshot transactionally. `QUANT_MARKET_DB` (default `data/market/market.sqlite3`) is only the explicit offline migration source. `data/` contains the app database, signal database, ticker databases, and SQLite-managed journal files. The browser receives authoritative data from the Rust API; the HTML is a static shell. Structured requests, responses, and browser/server messages use plain JSON with `application/json`.

For an existing legacy combined database, stop the service. Run `tooling/split-market-database.bat` if app-owned tables remain, then run `tooling/shard-market-database.bat` to distribute history by storage symbol. Both migrations validate row counts and stored values, record hashes and counts in SQLite, and retire source data only after validation succeeds. Startup never migrates implicitly.

For an existing app database with a saved signal snapshot, stop the service and run `tooling/migrate-signal-screener.bat` once before restarting. The migration validates the new signal database and retires the old app-database snapshot only after the copy succeeds. Startup never migrates implicitly.

Retired imports, archived outputs, and the unused validation database have been removed. No file-backed application-data reader is retained.

Hourly candle sync selects active registered tickers across exchanges. Tokocrypto uses bounded native completed-candle sync first; eligible crypto spot proxies independently collect Binance, Coinbase, Bybit, OKX, and Yahoo data, while perpetual/futures proxies collect Binance, Bybit, and Yahoo. Eligible equity mappings use Yahoo. SQLite records the app exchange separately from the data provider, and proxy bars remain labeled research data rather than venue execution evidence. Missing or stale history remains visible. After cloning, run `tooling\regather-market-data.bat` to explicitly rebuild public registry and candle data in SQLite. The selected app controls instrument classification: Stockbit is long-only IDX equities, Pintu Futures is crypto perpetuals, Tokocrypto includes crypto spot and tokenized stocks, and Pluang's registered crypto spot/perpetual products remain registry-only for execution.

## Trading and evidence rules

- The dashboard presents plans and research evidence; it has no paper-trading or live-order workflow.
- Research-only models remain marked unvalidated. A price inside an entry zone does not make a plan an order instruction.
- Historical claims retain their symbol, asset class, strategy, data window, and validation status. Win rate or total P&L is not a substitute for CAGR.
- Compounding evidence uses a chronological equity curve. The default promotion gate requires at least 10 completed trades over 730 calendar days and a positive compounded curve. Day-trade and multibagger profiles have additional strategy-specific gates.
- The steady-state memory target is 16 MB; chart and evidence reads are bounded.

## Useful references

- [Current architecture](ARCHITECTURE.md)
- [Market data storage](docs/market-data-storage.md)
- [Strategy literature and implementation](docs/strategy-literature-and-implementation.md)
- [Venue holding rules](docs/venue-holding-rules.md)
- [UI and financial meaning contract](docs/uiux-driven-development.md)
- [Validation and harness notes](docs/harness-engineering.md)
