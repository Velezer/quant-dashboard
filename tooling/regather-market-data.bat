@echo off
setlocal
cd /d "%~dp0.."

where cargo >nul 2>&1
if errorlevel 1 (
  echo ERROR: Cargo was not found in PATH.
  exit /b 1
)

echo Regathering approved registries and bounded public candle history into SQLite...
echo QUANT_MARKET_DIR and QUANT_APP_DB are honored; defaults are data\market\tickers and data\app\app.sqlite3.
cargo run --release -p rust_chart_api -- --regather-market-data
exit /b %ERRORLEVEL%
