@echo off
setlocal
cd /d "%~dp0.."
title Quant Dashboard - Harness Engineering Checks

echo ================================================================
echo   QUANT DASHBOARD - HARNESS ENGINEERING CHECKS
echo ================================================================
echo.

where cargo >nul 2>&1
if errorlevel 1 (
  echo ERROR: Cargo was not found in PATH.
  exit /b 1
)

echo [1/3] Browser Pine WASM build...
cargo build --release --target wasm32-unknown-unknown -p quant_pine_runtime
if errorlevel 1 (
  echo ERROR: Browser Pine WASM build failed.
  exit /b 1
)

echo [2/3] Source line limit...
powershell -NoProfile -ExecutionPolicy Bypass -File tooling/check_source_line_limit.ps1
if errorlevel 1 (
  echo ERROR: Source line limit check failed.
  exit /b 1
)

echo [3/3] Rust unit and integration tests...
cargo test --workspace -- --test-threads=1
if errorlevel 1 (
  echo ERROR: Rust tests failed.
  exit /b 1
)

echo.
echo Rust harness passed. Provider sync, paper order, or live order actions are not part of this check.
endlocal
