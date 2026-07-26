# Dokkomplekt Universal — semantic magic progress checkpoint

## Implemented in this checkpoint

- Added `kit_learning.rs`, an evidence-based learned document-bundle promotion engine.
- A learned bundle remains recommendation-only until the same domain/source cluster has at least eight observations, eight consecutive exact specialist confirmations, and at least 98% dominant-kit accuracy.
- Any specialist correction resets the clean streak and blocks automatic application.
- Added structural contracts for kit learning, corpus metrics and held-out confidence calibration.
- Split source verification from release proof:
  - `npm run quality` uses `static_quality_gate.py --source-only` and explicitly reports that Cargo was not executed.
  - `npm run quality:release` additionally requires real Cargo metadata, formatting, check, clippy and tests.
- Added `docs/QUALITY_GATES.md` so a green source gate cannot be represented as a release-ready installer.

## Verification performed here

- New semantic-magic contracts: 3/3 passed.
- Static source gate: passed; 98 Tauri commands; 291 Rust source files; Cargo explicitly not executed.
- The full sharded Python runner was started, but the surrounding execution environment terminated the long command after the first three green modules. This checkpoint therefore does not claim a fresh full-suite result.

## Still external evidence

Real Rust compilation, current RustSec database audit, packaged OCR/model runtime, and Windows hardware/installer/printing/reboot/Authenticode E2E remain mandatory release evidence.
