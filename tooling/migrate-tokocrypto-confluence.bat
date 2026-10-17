@echo off
setlocal
cd /d "%~dp0.."
cargo run --release -p rust_chart_api -- --migrate-tokocrypto-confluence
endlocal
