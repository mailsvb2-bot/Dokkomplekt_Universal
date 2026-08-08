# FULL DOKKOMPLEKT AUTOPILOT

`FULL DOKKOMPLEKT AUTOPILOT` is the single entry point for automated product verification. It does not replace the existing authoritative workflows; it adds executable product oracles, dispatches the existing gates, waits for their exact commit results, and produces one fail-closed verdict.

## Run it

GitHub → **Actions** → **FULL DOKKOMPLEKT AUTOPILOT** → **Run workflow**.

Choose one scope:

- `software` — the normal full automated contour available on GitHub-hosted infrastructure.
- `production-hardware` — everything in `software` plus the protected self-hosted Windows hardware/signing acceptance gate.

When the Autopilot implementation itself is changed and lands on `main`, the workflow automatically performs a `software` run. This gives the implementation an end-to-end self-test without ever starting production signing, a physical printer or a reboot by accident. Ordinary product commits continue to use the repository's existing per-PR gates; the complete Autopilot remains available as one explicit button whenever a full re-check is wanted.

## Executable product oracles

Before it is allowed to dispatch the broader CI contour, Autopilot runs output-level checks itself:

1. **DOCX visual oracle**
   - validates OOXML package integrity and internal relationships for the controlled document corpus;
   - renders every golden DOCX through LibreOffice to PDF and then to page images with Poppler;
   - rejects missing/blank pages, page-count changes, dimension changes and visual-hash drift beyond the controlled tolerance.
2. **Image-only PDF OCR fixture**
   - installs controlled hosted Poppler and Tesseract with Russian and English language data;
   - proves that the PDF fixture has no text layer;
   - rasterizes the PDF, performs real `rus+eng` OCR and reconstructs table rows;
   - requires the expected grounded tokens and minimum table structure before the software contour can pass.
3. **Synthetic semantic corpus**
   - generates 500 deterministic cases across accounting, HR, legal, medical and education using the real Rust `build_corpus_entry` path;
   - measures each domain through the shipped `measure_domain.py` implementation;
   - requires 100 cases per domain, real field/high-risk observations, field accuracy of at least 0.75 and exact kit completeness.

The orchestration job depends on both the capability registry and these executable oracles, so a file merely existing in the repository cannot by itself satisfy the final product verdict.

## What `software` requires

The Autopilot dispatches and waits for these existing workflows on the same commit SHA:

1. `quality-gate.yml`
   - locked Rust fmt/check/clippy/test gate;
   - RustSec;
   - isolated Python regression wall;
   - real PostgreSQL 16 multi-connection and HTTP integration;
   - TypeScript/Vitest/build/npm audit;
   - browser E2E;
   - Windows NSIS build/install/launch/uninstall smoke;
   - Linux AppImage/DEB/RPM build and install/launch smoke.
2. `source-provenance.yml`
   - checked-in source manifest;
   - deterministic clean source archive;
   - Cargo metadata from extracted archive;
   - portable Git history bundle;
   - `workflow_dispatch` checks out the immutable event SHA and verifies `HEAD == GITHUB_SHA`, rather than resolving a potentially newer moving branch when the runner starts.
3. `macos-smoke.yml`
   - native `.app` and DMG build/verification.
4. `unsigned-preview.yml`
   - Windows thin NSIS preview;
   - explicit `NotSigned` boundary;
   - real installed application launch;
   - native Word-template picker;
   - button creation from a real DOCX;
   - persistence after restart;
   - silent uninstall.

A green software run is reported only as **SOFTWARE PASS**. It is deliberately not called production-ready.

## What `production-hardware` additionally requires

This scope can run only from protected `main`. It also dispatches `windows-hardware-e2e.yml` for the exact current commit.

The existing hardware gate requires the configured self-hosted Windows runner, production signing environment and runner-owned runtime. It verifies real evidence including:

- licensed Microsoft Word availability;
- real printer availability;
- completed Windows PrintService event 307;
- valid Authenticode for installer and installed application;
- visible titled application GUI on initial launch and restart;
- no newly appearing visible `cmd`, PowerShell, `conhost`, WSH or console windows;
- watcher installation/autostart;
- evidence from a real Windows reboot;
- post-reboot case completion and output SHA-256;
- signed/offline runtime and sidecar provenance gates;
- silent uninstall.

Only when every hosted workflow and the hardware workflow complete successfully does the Autopilot report **FULL PRODUCTION PASS**.

## Feature coverage contract

`verification/autopilot/feature-matrix.json` is a machine-enforced capability registry. Each mandatory capability has:

- a stable feature ID;
- a test category;
- a scope (`software` or `production-hardware`);
- an automation level (`integration`, `installed-e2e`, `hardware-e2e`, `golden`, etc.);
- one or more concrete evidence files already present in the repository;
- `automated: true`.

`scripts/full_product_autopilot.py validate` fails when:

- a mandatory capability disappears from the registry;
- an unknown/unreviewed capability ID is inserted;
- a feature becomes manual-only;
- an evidence path disappears;
- the software or hardware coverage floor drops unexpectedly.

`tests/test_full_product_autopilot_contract.py` makes this governance part of the normal Python regression wall and also guards executable OCR coverage, the explicit Rust corpus toolchain and immutable source-provenance dispatch checkout.

## Reports

Each run writes and uploads:

- `verification/autopilot/FULL_AUTOPILOT_REPORT.json` — machine-readable verdict;
- `verification/autopilot/FULL_AUTOPILOT_REPORT.md` — human-readable report shown in the GitHub job summary;
- the feature matrix and coverage report;
- OCR evidence, synthetic corpus and per-domain metric artifacts from the document-oracle job.

The final report contains the exact commit SHA and links to every dispatched workflow run. A timeout, missing run, cancellation, skipped required gate or non-success conclusion makes the Autopilot fail.

## Safety property

The Autopilot never converts missing physical evidence into a simulated success. Hardware requirements are either actually executed in `production-hardware` scope or explicitly excluded from a `SOFTWARE PASS`. This distinction is intentional and fail-closed.
