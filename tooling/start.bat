@echo off
setlocal
cd /d "%~dp0.."
title Quant Dashboard - Build and Run
cls
set "QUANT_RUST_ADDR=127.0.0.1:7777"
echo =================================================================
echo   QUANT DASHBOARD - NATIVE RUST
echo   Rebuilding native service and browser Pine WASM before starting...
echo =================================================================

powershell -NoProfile -Command "Get-Process -Name rust_chart_api -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue" >nul 2>&1
set "QUANT_PORT_PID="
for /f "tokens=5" %%P in ('netstat -ano -p tcp ^| findstr /R /C:":7777 .*LISTENING"') do set "QUANT_PORT_PID=%%P"
if defined QUANT_PORT_PID (
  echo ERROR: Dashboard address %QUANT_RUST_ADDR% is already in use.
  echo The port is owned by process ID %QUANT_PORT_PID%.
  pause
  exit /b 1
)
set "QUANT_DASHBOARD_ADDR=%QUANT_RUST_ADDR:0.0.0.0=127.0.0.1%"
set "QUANT_DASHBOARD_ADDR=%QUANT_DASHBOARD_ADDR:[::]=[::1]%"
set "QUANT_DASHBOARD_URL=http://%QUANT_DASHBOARD_ADDR%/"
set "QUANT_HEALTH_URL=%QUANT_DASHBOARD_URL%api/health"

where cargo >nul 2>&1
if errorlevel 1 (
  echo ERROR: Cargo was not found in PATH.
  pause
  exit /b 1
)

cargo build --release --target wasm32-unknown-unknown -p quant_pine_runtime
if errorlevel 1 (
  echo ERROR: Browser Pine WASM build failed. Install the wasm32-unknown-unknown Rust target.
  pause
  exit /b 1
)

cargo build --release --manifest-path backend\service\Cargo.toml
if errorlevel 1 (
  echo ERROR: Rust build failed.
  pause
  exit /b 1
)

if not exist target\release\rust_chart_api.exe (
  echo ERROR: Built executable was not found.
  pause
  exit /b 1
)

echo Starting Local Dashboard Server on %QUANT_DASHBOARD_URL% ...
start "Quant Dashboard" target\release\rust_chart_api.exe
set "QUANT_SERVICE_STARTED="
for /l %%I in (1,1,15) do (
  if not defined QUANT_SERVICE_STARTED (
    powershell -NoProfile -Command "$health = Invoke-RestMethod -Uri '%QUANT_HEALTH_URL%' -TimeoutSec 1 -ErrorAction SilentlyContinue; if ($health.service -eq 'rust_quant_engine') { exit 0 } else { exit 1 }" >nul 2>&1
    if not errorlevel 1 set "QUANT_SERVICE_STARTED=1"
    if not defined QUANT_SERVICE_STARTED timeout /t 1 /nobreak >nul
  )
)
if not defined QUANT_SERVICE_STARTED (
  echo ERROR: The Rust dashboard failed its local health check at %QUANT_HEALTH_URL%.
  echo Check the Quant Dashboard server window for startup or port-binding errors.
  pause
  exit /b 1
)
timeout /t 2 /nobreak >nul
start "" "%QUANT_DASHBOARD_URL%"
echo Dashboard is running. Keep this window open to rebuild again later.
pause
