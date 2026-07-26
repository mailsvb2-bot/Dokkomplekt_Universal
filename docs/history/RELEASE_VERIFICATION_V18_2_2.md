# Release verification — Dokkomplekt Universal 18.2.2

## Статус

**SOURCE REPAIRED AND LINUX-VERIFIED — PROTECTED RELEASE/WINDOWS GATES REQUIRED.**

Текущее дерево собирается и тестируется на точном Rust 1.85.1 в Linux-среде с DBus/GTK/WebKitGTK development-библиотеками. Оно не объявляется подписанным production-релизом: для этого всё ещё нужны актуальная RustSec database, защищённая attestation, Windows installer/signing и hardware E2E.

## Проверено на финальном дереве 21 июля 2026 года

- `cargo metadata --locked`: passed;
- `cargo fmt --all -- --check`: passed;
- `cargo check --workspace --all-targets --locked`: passed;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed;
- `cargo test --workspace --locked`: **365/365**, включая Tauri backend **40/40**;
- `npx tauri build --no-bundle`: passed; optimized Linux application created;
- DBus/Xvfb runtime smoke: no immediate crash during a 12-second controlled launch;
- clean Python environment from `requirements-dev.txt`: **150/150**;
- Vitest: **34/34**;
- TypeScript typecheck: passed;
- production frontend build: passed;
- npm audit: **0 vulnerabilities**;
- static source gate: passed — **86 Tauri commands, 284 Rust source files**;
- production panic-shortcut audit: passed;
- reference-data freshness: passed (`2026=complete`, `2027=provisional`);
- starter-pack reproducibility: **11/11**;
- DOCX structural/visual goldens: **7/7**;
- reviewed security-backport hash verification: passed.

## RustSec boundary

`cargo-audit 0.22.1` is installed, but the isolated verification container could not fetch the RustSec advisory database from GitHub. The command failed closed before scanning and is therefore **not** reported as passed. CI/release must rerun `cargo audit --deny warnings` against an up-to-date mirrored or reachable database.

The previously identified vulnerable `pyo3` and `quick-xml` versions were removed. The project uses `pyo3 0.24.2` and one consolidated `quick-xml 0.41.0`. Because upstream `time 0.3.47` requires Rust 1.88, the exact RFC 2822 recursion-depth mitigation is backported to vendored `time 0.3.45`; the source is hash-pinned, regression-tested, and the audit exception is limited to `RUSTSEC-2026-0009`.

## Не считается пройденным

- signed `.cargo-gate` attestation generated with protected release secrets;
- NSIS/MSVC/Windows SDK packaging on the release runner;
- production Authenticode and timestamp validation;
- Microsoft Word COM, DPAPI, printer, duplex/tray and spooler hardware execution;
- watcher survival across a real operating-system reboot;
- production component packs with licensed vendor binaries and approved model weights;
- qualified electronic signature and certified PDF/A validation.

## Обязательные release artifacts

1. Ed25519-signed `.cargo-gate/CARGO_GATE_ATTESTATION.json` and signature bound to source fingerprint, `Cargo.lock`, commit and CI run;
2. signed application EXE and signed NSIS installer;
3. Windows hardware evidence with PrintService completion and two-boot watcher proof;
4. signed component catalog and hash-verified runtime/component artifacts;
5. current RustSec report, SBOM, binary hashes and provenance.
