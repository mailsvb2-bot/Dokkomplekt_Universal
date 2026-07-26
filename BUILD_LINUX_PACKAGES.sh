#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
npm ci
npm run typecheck
npm run test
npm run build
npm run tauri:build
