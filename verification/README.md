# Verification evidence — Dokkomplekt Universal 18.3.2

Date: 2026-07-23

This directory contains evidence for the exact source checkpoint in this archive. It is not evidence of a signed production installer or a hardware Windows acceptance run.

## Passed gates

- Rust 1.97.0 `cargo fmt --all -- --check`.
- `cargo check --workspace --all-targets --locked --offline`, including the Tauri shell.
- `cargo test --workspace --locked --offline`: 370 passed, 0 failed.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`.
- cargo-audit 0.22.2 against the official RustSec database: 0 reported vulnerabilities. Seventeen exact Tauri transitive warnings are accepted in `.cargo/audit.toml` and explained in `RUSTSEC_ACCEPTED_RISKS.md`; any new advisory remains fatal.
- `python3 -m pytest -q tests`: 190 passed, 0 failed.
- TypeScript typecheck.
- Vitest: 36 passed, 0 failed.
- Vite production build.
- Playwright Chromium E2E: 2 passed, 0 failed.
- npm production dependency audit: 0 vulnerabilities.
- Static source gate: passed after removal of build caches; see `static-quality-gate.log`.

`summary.json` contains the machine-readable totals and lockfile hashes.
