#!/usr/bin/env bash
set -euo pipefail
python scripts/check_reference_data_freshness.py
python scripts/static_quality_gate.py --source-only
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --locked
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm ci
npm run typecheck
npm run test
npm run e2e:install
npm run e2e
npm run tauri:build:check
bash tests/installer/linux_installer_contract.sh
