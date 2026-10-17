@echo off
setlocal
cd /d "%~dp0.."

where cargo >nul 2>&1
if errorlevel 1 (
  echo ERROR: Cargo was not found in PATH.
  exit /b 1
)

echo Splitting legacy app tables into the app SQLite database.
echo Stop the Rust service before running this migration.
echo This explicit migration splits app-owned tables from the legacy single-file database.
echo QUANT_MARKET_DB selects the legacy source; QUANT_APP_DB selects the app database.
echo Defaults are data\market\market.sqlite3 and data\app\app.sqlite3.
cargo run --release -p rust_chart_api -- --split-market-database
exit /b %ERRORLEVEL%
