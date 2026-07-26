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

run_logged() {
  local name="$1"
  shift
  local log=".cargo-gate/${name}.log"
  echo "::group::${name}"
  set +e
  "$@" >"$log" 2>&1
  local status=$?
  set -e
  if [ "$status" -ne 0 ]; then
    echo "::error title=${name} failed::Command exited with code ${status}"
    tail -n 220 "$log" || true
    echo "::endgroup::"
    return "$status"
  fi
  echo "${name}: passed"
  echo "::endgroup::"
}

run_logged cargo-metadata cargo metadata --locked --format-version 1
run_logged cargo-fmt cargo fmt --all -- --check
run_logged cargo-check cargo check --workspace --all-targets --locked --quiet
run_logged cargo-clippy cargo clippy --workspace --all-targets --locked --quiet -- -D warnings
run_logged cargo-test cargo test --workspace --locked --quiet
if ! cargo audit --version >/dev/null 2>&1; then
  echo "ERROR: cargo-audit 0.22.2+ is required before packaging." >&2
  exit 1
fi
run_logged commercial-rust-crates python scripts/check_commercial_rust_crates.py

set +e
cargo audit --deny warnings --json > .cargo-gate/RUSTSEC_AUDIT.json 2> .cargo-gate/RUSTSEC_AUDIT.stderr
rustsec_status=$?
set -e
if [ "$rustsec_status" -ne 0 ]; then
  python3 - <<'PY'
import json
from pathlib import Path

path = Path('.cargo-gate/RUSTSEC_AUDIT.json')
if path.exists() and path.read_text(encoding='utf-8').strip():
    report = json.loads(path.read_text(encoding='utf-8'))
    for item in report.get('vulnerabilities', {}).get('list', []):
        advisory = item.get('advisory', {})
        package = item.get('package', {})
        print(
            f"VULNERABILITY {advisory.get('id', 'unknown')}: "
            f"{package.get('name', 'unknown')} {package.get('version', '')} — "
            f"{advisory.get('title', '')}"
        )
    for category, entries in report.get('warnings', {}).items():
        for item in entries:
            advisory = item.get('advisory', {})
            package = item.get('package', {})
            print(
                f"WARNING {category} {advisory.get('id', 'unknown')}: "
                f"{package.get('name', 'unknown')} {package.get('version', '')} — "
                f"{advisory.get('title', '')}"
            )
else:
    print('cargo-audit produced no JSON report')
PY
  if [ -s .cargo-gate/RUSTSEC_AUDIT.stderr ]; then
    tail -n 80 .cargo-gate/RUSTSEC_AUDIT.stderr
  fi
  exit "$rustsec_status"
fi

python3 scripts/write_rustsec_evidence.py
if [ -n "${DOKKOMPLEKT_GATE_PRIVATE_KEY_B64:-}" ]; then
  python3 scripts/write_cargo_gate_attestation.py
else
  echo "RUST QUALITY GATE PASSED: signed release attestation skipped (no release signing key)."
fi
