@echo off
setlocal
cd /d "%~dp0.."

where cargo >nul 2>&1
if errorlevel 1 (
  echo ERROR: Cargo was not found in PATH.
  exit /b 1
)

echo Sharding the shared market database into per-symbol SQLite files.
echo Stop the Rust service before running this migration.
echo QUANT_MARKET_DB selects the legacy source; QUANT_MARKET_DIR selects the ticker directory; QUANT_APP_DB selects the app database; QUANT_SIGNAL_DB selects the signal database.
echo Defaults are data\market\market.sqlite3, data\market\tickers, data\app\app.sqlite3, and data\signals\signals.sqlite3.
cargo run --release -p rust_chart_api -- --shard-market-database
exit /b %ERRORLEVEL%
