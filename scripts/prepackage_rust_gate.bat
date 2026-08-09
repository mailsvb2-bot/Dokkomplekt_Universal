@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0\.."
python scripts\check_reference_data_freshness.py || exit /b 1
python scripts\audit_rust_production_panics.py || exit /b 1
where cargo >nul 2>nul
if errorlevel 1 (
  echo ERROR: cargo is required before packaging. Install Rust 1.97.1+ and rerun.
  exit /b 1
)
if not exist Cargo.lock (
  echo ERROR: Cargo.lock is missing. Run: cargo generate-lockfile with Rust 1.97.1+ and commit it.
  exit /b 1
)
if exist .cargo-gate rmdir /s /q .cargo-gate
mkdir .cargo-gate
cargo metadata --locked --format-version 1 >nul || exit /b 1
cargo fmt --all -- --check || exit /b 1
cargo check --workspace --all-targets --locked || exit /b 1
cargo clippy --workspace --all-targets --locked -- -D warnings || exit /b 1
cargo test --workspace --locked || exit /b 1
cargo audit --version >nul 2>nul || (
  echo ERROR: cargo-audit 0.22.2+ with CVSS 4 support is required before packaging.
  exit /b 1
)
python scripts\check_commercial_rust_crates.py || exit /b 1
REM Security invariant: run_rustsec_audit.py executes cargo audit --deny warnings --json
REM against the exact validated DB with --db and --no-fetch; no stale/advisory bypass is allowed.
python scripts\run_rustsec_audit.py --json-output .cargo-gate\RUSTSEC_AUDIT.json --pin-report .cargo-gate\RUSTSEC_DB_PIN.json || exit /b 1
python scripts\write_rustsec_evidence.py || exit /b 1
python scripts\write_cargo_gate_attestation.py || exit /b 1
python scripts\assert_release_ready.py || exit /b 1
