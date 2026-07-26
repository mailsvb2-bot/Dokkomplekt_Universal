# Dokkomplekt Universal 18.2.2 — final repair verification

Date: 2026-07-21  
Declared toolchain: Rust 1.85.1  
Verification platform: Debian Linux with DBus/GTK/WebKitGTK development libraries

## Outcome

The source tree is repaired so the complete Rust workspace, including the Tauri backend, compiles and passes its automated tests. This report deliberately does not claim a signed Windows production release or physical Word/printer/reboot acceptance.

## Fundamental repairs

1. Removed the unconditional `resources/tools/**` requirement from ordinary Tauri builds and added a separate `tauri.offline.conf.json` used only after verified sidecar staging.
2. Fixed all seven Tauri compile/type errors: malformed newline literals, missing resume field, stale document field name, invalid unit-result handling and mismatched result types.
3. Closed strict Clippy findings without blanket lint suppression.
4. Made signed component-catalog verification deterministic and offline. HTTPS, credentials, fragments, localhost and signed host allow-list are checked without DNS; DNS resolution, forbidden-IP checks and address pinning remain immediately before the download.
5. Added role-aware document header parsing so act/order/invoice identifiers do not get replaced by contract identifiers.
6. Prevented legal requisites from being interpreted as phone numbers and separated organization names from INN/KPP/OGRN/BIC tails.
7. Added HR narrative parsing for employee name, position, hire date and employment contract, with regression tests.
8. Prevented a table header such as “Наименование Количество Цена Сумма” from consuming the first item quantity/price as the document total; explicit “Итого/К оплате” evidence now wins and is regression-locked.
9. Upgraded `pyo3` to 0.24.2.
10. Consolidated XML handling on `quick-xml 0.41.0`, including named and numeric entity preservation in DOCX/DOCM parts.
11. Backported the upstream RFC 2822 recursion-depth guard to `time 0.3.45` because the first fixed upstream release requires Rust 1.88. The backport is hash-pinned, fingerprinted and regression-tested.
12. Corrected release workflow ordering and thin/offline Tauri configuration selection.
13. Updated project status documents so they no longer claim Rust or Linux Tauri were unavailable.

## Passed verification

### Rust

- `cargo metadata --locked`
- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`: **365 passed, 0 failed, 0 ignored**
- Tauri backend tests: **40 passed, 0 failed**
- `npx tauri build --no-bundle`: passed; optimized Linux binary created
- controlled DBus/Xvfb launch: process remained alive for 12 seconds and was stopped by the test timeout; only a non-fatal AT-SPI accessibility-bus warning was emitted

### Python/source contracts

- clean virtual environment from `requirements-dev.txt`
- Python tests: **150 passed, 0 failed**
- static source gate: passed
- production Rust panic audit: passed
- security-backport verifier: passed
- reference-data freshness: passed

### Frontend

- TypeScript typecheck: passed
- Vitest: **34 passed, 0 failed**
- production Vite build: passed
- npm audit: **0 vulnerabilities**

### Documents/content

- starter content pack reproducibility: **11 templates passed**
- DOCX structural/visual goldens: **7 fixtures passed**

## Supply-chain boundary

`cargo-audit 0.22.1` was installed and invoked with `--deny warnings`. The isolated container could not fetch the RustSec advisory database and the command stopped before scanning. This is recorded as an unpassed external gate, not converted into a false success. A protected CI/release runner must use an up-to-date reachable or mirrored advisory database.

The dependency tree no longer contains the old vulnerable `pyo3 0.22.6`, `quick-xml 0.36.2`, or `quick-xml 0.38.4`. The sole scoped exception is the reviewed `time` backport documented in `SECURITY_EXCEPTIONS.md`.

## External acceptance still required

- Visual Studio Build Tools and Windows SDK build;
- NSIS installer generation;
- production Authenticode and timestamping;
- real Microsoft Word automation;
- DPAPI and Windows user-profile behavior;
- watcher recovery after actual reboot;
- real printer/spooler, duplex and tray evidence;
- signed Cargo/release attestation;
- licensed production sidecars and approved model weights.

The result is a clean, fully Rust-checked source candidate. It is not mislabeled as a completed Windows production installer.
