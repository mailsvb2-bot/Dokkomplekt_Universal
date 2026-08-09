#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

resolve_python() {
  if [ -n "${PYTHON_BIN:-}" ]; then
    if command -v "$PYTHON_BIN" >/dev/null 2>&1; then
      printf '%s\n' "$PYTHON_BIN"
      return 0
    fi
    echo "ERROR: PYTHON_BIN points to an unavailable interpreter: $PYTHON_BIN" >&2
    return 1
  fi

  local candidate
  for candidate in python python3; do
    if command -v "$candidate" >/dev/null 2>&1; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  echo "ERROR: Python 3 is required before packaging." >&2
  return 1
}

PYTHON_BIN="$(resolve_python)"
if ! "$PYTHON_BIN" -c 'import sys; raise SystemExit(0 if sys.version_info.major == 3 else 1)'; then
  echo "ERROR: $PYTHON_BIN is not a Python 3 interpreter." >&2
  exit 1
fi

"$PYTHON_BIN" scripts/check_reference_data_freshness.py
"$PYTHON_BIN" scripts/audit_rust_production_panics.py
"$PYTHON_BIN" scripts/verify_security_backports.py

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
"$PYTHON_BIN" scripts/check_commercial_rust_crates.py
# Security invariant: run_rustsec_audit.py executes the equivalent of
# cargo audit --deny warnings --json against the exact validated advisory DB pin,
# using cargo-audit --db <checkout> --no-fetch; no advisory/stale bypass is allowed.
"$PYTHON_BIN" scripts/run_rustsec_audit.py \
  --json-output .cargo-gate/RUSTSEC_AUDIT.json \
  --pin-report .cargo-gate/RUSTSEC_DB_PIN.json
"$PYTHON_BIN" scripts/write_rustsec_evidence.py
if [ -n "${DOKKOMPLEKT_GATE_PRIVATE_KEY_B64:-}" ]; then
  "$PYTHON_BIN" scripts/write_cargo_gate_attestation.py
else
  echo "RUST QUALITY GATE PASSED: signed release attestation skipped (no release signing key)."
fi
