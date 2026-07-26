# Release verification — Dokkomplekt Universal 18.2.1

## Verdict

18.2.1 is a **source candidate for a controlled pilot**. It is not a compiled production release. The current execution environment has no Rust 1.85.1 toolchain, therefore the changed Rust backend has not been proven by `cargo fmt`, `cargo check`, Clippy, Rust tests, RustSec, Tauri build or Windows hardware E2E.

No historical `.cargo-gate` marker was copied into this tree.

## Verified on the final source tree

- Python regression/source contracts: **135/135**.
- Vitest user/API scenarios: **34/34**.
- TypeScript typecheck: passed.
- Production frontend build: passed.
- npm audit: **0 vulnerabilities** against the committed lockfile.
- Static Rust source/DTO/command gate: passed — **86 Tauri commands, 117 Rust source files**.
- Production panic-shortcut source audit: passed.
- Reference-data freshness gate: passed according to the bundled fail-closed policy; 2027 is still provisional.
- `main.rs` architectural ceiling: passed — **2,214 lines**, with four named subsystems below 3,000 lines each.
- Per-document resume, queue, calendar auto-pull and UI-observability contracts: passed.

These checks prove source consistency and frontend behavior only. They do not replace compilation.

## 18.2.1 release invariants

1. A document checkpoint is written immediately after that document renders successfully.
2. Resume fingerprint v2 hashes the template, app version, watermark and only the fields, collections and named blocks actually referenced by that template.
3. A correction to an unrelated field does not invalidate an already-rendered document.
4. Templates containing counters, image placeholders or working-day calculations are rendered again conservatively because counters, assets and signed calendar data can change outside the semantic case.
5. When `DOKKOMPLEKT_QUEUE_DATABASE_URL` is configured, an unreachable central queue blocks processing instead of silently falling back to a local lock.
6. PostgreSQL claim uses a bounded connection attempt, row transaction, lease, worker identity, renewal and completed receipt keyed by source SHA-256.
7. Without a central queue configuration the existing shared-filesystem SHA-256 queue remains active.
8. Signed calendar auto-pull is attempted at most once per 24 hours; a failed download never replaces the last verified calendar.
9. Packaging remains forbidden until the real Rust marker matches the current source fingerprint.

## Mandatory production gate

Run on a clean supported builder:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit --deny warnings
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
npm ci
npm run typecheck
npm run test
npm run build
npx tauri build --bundles nsis
```

Then run the self-hosted Windows hardware workflow with a real Word installation, real printer profiles, watcher restart/reboot phases and production Authenticode signing.

## Explicitly not claimed

- production-ready central queue transport on an untrusted network;
- actual vendor OCR/office/model binaries in this source archive;
- legally significant CryptoPro/Goskey signature;
- certified PDF/A-1A without veraPDF evidence;
- approved Tier-1 professional forms;
- guaranteed autonomous understanding of any unknown document.
