# Dokkomplekt Universal 18.2.2 — repair and verification report

Date: 2026-07-20  
Toolchain used: `rustc 1.85.1`, `cargo 1.85.1`

## Status

The source tree has been repaired so that its deterministic business crates are reproducible on the declared Rust toolchain. This report does **not** claim that a signed production installer or Windows hardware acceptance has been completed.

## Fundamental fixes

1. Rebuilt `Cargo.lock` with MSRV-aware resolution for Rust 1.85.1.
2. Pinned `deflate64` to `0.1.10`; newer transitive releases used unstable APIs on Rust 1.85.1.
3. Replaced the default AWS-LC TLS provider with explicit `rustls` + `ring` and added process-safe provider initialization before every reqwest client path.
4. Fixed moved-value error E0382 in semantic LLM merging.
5. Fixed mutable/immutable borrow conflict E0502 in encrypted SQLite audit writes.
6. Fixed strict Clippy failures and applied canonical `cargo fmt` formatting.
7. Fixed role-aware parsing of provider/customer names and inline requisites while preserving legal quotation marks in organization names.
8. Fixed loss of `collection.items` from the parsed-source report after semantic alias normalization.
9. Added Word `comments.xml` to strict rendering and text extraction, so placeholders in comments are rendered and verified with all other Word stories.
10. Corrected the encrypted case-run raw-storage regression test so nullable columns cannot turn the complete audit expression into SQL `NULL`.
11. Added the Python dependencies actually imported by the tests: PyYAML, cryptography and lxml.
12. Isolated bundled-template tests from `.venv`, `target`, `node_modules`, generated output and hidden service directories.
13. Separated ordinary CI compilation from signed release attestation: PR/push gates no longer require a production private signing key, while protected release workflows remain fail-closed.

## Passed verification

### Rust business crates

Packages:

- `dokkomplekt-core`
- `dokkomplekt-docx`
- `dokkomplekt-storage`
- `dokkomplekt-license-core`
- `dokkomplekt-license-server`
- `dokkomplekt-license-python`
- `dokkomplekt-morph`
- `dokkomplekt-refdata`

Passed:

- `cargo metadata --locked`
- `cargo fmt --all -- --check`
- `cargo check --locked --all-targets`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked`: **319 passed, 0 failed**

### Python and source contracts

- clean virtual environment installation from `requirements-dev.txt`
- Python tests: **147 passed, 0 failed**
- reference-data freshness policy: passed
- Rust production panic-shortcut audit: passed
- starter-pack reproducibility: **11 templates passed**
- DOCX structural/visual goldens: **7 fixtures passed**
- static source gate: passed; **86 Tauri commands**, **125 Rust source files**

### Frontend

- clean `npm ci`: passed
- TypeScript typecheck: passed
- Vitest: **34 passed, 0 failed**
- production Vite build: passed
- `npm audit`: **0 vulnerabilities**

## External gates still required

These items require a platform-specific release environment and are deliberately not marked as passed:

- full Tauri workspace build and NSIS packaging on a Windows runner with Visual Studio Build Tools and Windows SDK;
- Authenticode signing with the production certificate;
- signed Cargo/release attestation using protected release secrets;
- Microsoft Word COM, DPAPI, watcher-after-reboot, real printer, duplex/tray and spooler evidence;
- Linux AppImage/DEB/RPM linking on a runner with WebKitGTK/GTK/DBus development libraries;
- production OCR/PDF/LibreOffice/7-Zip/SumatraPDF sidecars and approved GGUF weights;
- mTLS central queue, external audit anchor, qualified electronic signature and certified PDF/A validation.

A Windows cross-target check was attempted from Linux. It reached native `ring` compilation and then stopped because the Linux container does not contain the Windows SDK C headers and MSVC librarian. This is an environment limitation, not counted as a passed or failed application acceptance test.
