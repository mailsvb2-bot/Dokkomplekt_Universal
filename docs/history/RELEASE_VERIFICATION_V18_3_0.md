# Release verification — Dokkomplekt Universal 18.3.0

**Date:** 2026-07-21  
**Environment used here:** Linux container  
**Verdict:** verified source checkpoint; **not** a production-ready Windows binary release.

## Executed successfully

| Check | Result |
|---|---|
| TypeScript typecheck | PASS |
| Vitest frontend/component contracts | PASS — 36/36 |
| Production frontend build (`tsc && vite build`) | PASS |
| Static source quality gate | PASS — version 18.3.0, 98 Tauri commands, 290 Rust source files |
| Python contract modules in isolated processes | PASS — 27/27 modules, 176/176 tests, 0 skipped; source unchanged during run; exact fingerprint is recorded in `build-evidence/python-contract-shards.json` |
| Python 18.3 hardening contracts | PASS — 13/13 |
| OCR/layout contracts | PASS — 4/4 |
| Image-only scanned PDF table smoke | PASS in this Linux environment — Poppler + Tesseract recognized 4 table rows and all required grounded tokens |
| License DB transport contracts | PASS — 3/3 |
| Rust panic-shortcut audit contract | PASS — 2/2 |
| mTLS queue E2E contract | PASS inside the 18.3 hardening module |
| Deterministic source ZIP CRC/path/SHA verification | PASS — deterministic ZIP, 612 entries including source manifest; CRC, safe paths, duplicate check and per-file SHA-256 verified |

## Not executed and not claimed

| Required release proof | Status |
|---|---|
| `cargo metadata --locked` | NOT RUN — real Cargo unavailable |
| `cargo fmt --all -- --check` | NOT RUN — real Rust toolchain unavailable |
| `cargo check --workspace --all-targets --locked` | NOT RUN |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | NOT RUN |
| `cargo test --workspace --locked` | NOT RUN |
| Fresh `cargo audit --deny warnings --json` against current advisory DB | NOT RUN |
| Tauri/NSIS build on Windows | NOT RUN |
| Authenticode signing and verification | NOT RUN |
| Word COM/DPAPI/printer hardware tests | NOT RUN |
| Watcher-after-real-reboot two-boot evidence | NOT RUN |
| Real OCR/model payload production probe | NOT RUN — only synthetic staging contracts are available |

## Offline/online boundary verified at source-contract level

- Desktop has no direct PostgreSQL client or credentials.
- No mTLS queue configuration means no central network call.
- Local document processing remains the default path.
- Optional distributed coordination uses HTTPS/mTLS.
- PostgreSQL, when selected, is behind the queue service and requires verified TLS.
- Missing signed calibration blocks only automatic printing, not local document creation.

## Release decision

The tree may be distributed as a clean **source checkpoint** for continued Windows/CI verification. It must not be labelled production-ready, fully offline-installer-ready or “100/100” until the missing Rust, Windows, Authenticode and real-runtime gates complete for the exact archive hash.
