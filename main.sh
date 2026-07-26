#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
REQUIRED_RUST="1.97.1"
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
  echo "ERROR: cargo/rustc not found. Install rustup and Rust ${REQUIRED_RUST}."
  exit 1
fi
ACTUAL_RUST="$(rustc --version | awk '{print $2}')"
if [[ "$ACTUAL_RUST" != "$REQUIRED_RUST" ]]; then
  echo "ERROR: this source tree requires Rust ${REQUIRED_RUST}; found ${ACTUAL_RUST}."
  echo "Run: rustup toolchain install ${REQUIRED_RUST} --profile minimal --component clippy,rustfmt"
  exit 1
fi
if ! command -v npm >/dev/null 2>&1; then
  echo "ERROR: npm not found. Install Node.js LTS first."
  exit 1
fi
npm ci
if [[ ! -f Cargo.lock ]]; then
  cargo generate-lockfile
fi
npm run tauri:dev
