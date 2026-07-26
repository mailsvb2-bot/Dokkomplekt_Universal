#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python scripts/check_reference_data_freshness.py
python scripts/audit_rust_production_panics.py
python scripts/verify_security_backports.py

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo is required before packaging. Install Rust 1.97.1+ and rerun." >&2
  exit 1
fi
if [ ! -f Cargo.lock ]; then
  echo "ERROR: Cargo.lock is missing. Run: cargo generate-lockfile with Rust 1.97.1+ and commit it." >&2
  exit 1
fi

rm -rf .cargo-gate
mkdir -p .cargo-gate
cargo metadata --locked --format-version 1 >/dev/null
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
if ! cargo audit --version >/dev/null 2>&1; then
  echo "ERROR: cargo-audit 0.22.2+ with CVSS 4 support is required before packaging." >&2
  exit 1
fi
python scripts/check_commercial_rust_crates.py
cargo audit --deny warnings --json > .cargo-gate/RUSTSEC_AUDIT.json
python3 scripts/write_rustsec_evidence.py
if [ -n "${DOKKOMPLEKT_GATE_PRIVATE_KEY_B64:-}" ]; then
  python3 scripts/write_cargo_gate_attestation.py
else
  echo "RUST QUALITY GATE PASSED: signed release attestation skipped (no release signing key)."
fi
