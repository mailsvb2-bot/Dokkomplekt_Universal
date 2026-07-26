@echo off
setlocal enabledelayedexpansion
python scripts\check_reference_data_freshness.py || exit /b 1
python scripts\static_quality_gate.py --source-only || exit /b 1
cargo metadata --locked --format-version 1 >nul || exit /b 1
cargo check --workspace --all-targets --locked || exit /b 1
cargo fmt --check || exit /b 1
cargo clippy --workspace --all-targets --locked -- -D warnings || exit /b 1
cargo test --workspace --locked || exit /b 1
npm ci || exit /b 1
npm run typecheck || exit /b 1
npm run test || exit /b 1
npm run e2e:install || exit /b 1
npm run e2e || exit /b 1
npm run tauri:build:check || exit /b 1
powershell -ExecutionPolicy Bypass -File tests\installer\windows_installer_contract.ps1
