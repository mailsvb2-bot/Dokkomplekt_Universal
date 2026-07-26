# Dokkomplekt Universal 18.2.1 — Resume, Queue and Runtime Modularization

## 1. Why this version exists

18.2.0 introduced signed downloadable components, but three structural gaps remained:

- `main.rs` contained more than eight thousand lines and mixed unrelated OS/update/document/automation responsibilities;
- recovery could restart a case, but did not safely reuse completed documents at dependency level;
- filesystem leases reduced duplicate processing, but could not provide a transactional multi-computer claim.

18.2.1 addresses those gaps without creating a second workflow, parser or renderer.

## 2. Runtime decomposition

`src-tauri/src/main.rs` is reduced to the application shell, shared DTO/state/helpers and command registration. The same crate now includes four named subsystem files:

- `subsystems/update_runtime.rs`;
- `subsystems/desktop_io.rs`;
- `subsystems/document_commands.rs`;
- `subsystems/automation_runtime.rs`.

The extraction is deliberately same-crate and mechanical. Business rules remain in the existing Rust crates; the subsystem files do not become competing engines.

A regression contract limits `main.rs` and every subsystem to fewer than 3,000 lines.

## 3. Dependency-level resume

SQLite now stores one `case_run_documents` record per document and case:

- document id;
- input fingerprint;
- encrypted checkpoint/final path;
- status;
- source case from which a checkpoint was reused.

After each successful render the output is copied atomically to an application-data checkpoint and its record is committed immediately. If a later document fails, the completed documents survive for the retry.

Fingerprint v2 includes:

- document id and exact template bytes;
- application version;
- current watermark;
- only referenced semantic values and canonical aliases;
- only referenced collections;
- named block contents, recursively, including their own dependencies.

An unrelated correction therefore does not force the whole package to be regenerated. Counter-, image- and working-day-dependent templates are intentionally not reused because their result can depend on external mutable state, including a signed calendar update independent of the app version.

## 4. Central multi-machine queue

When `DOKKOMPLEKT_QUEUE_DATABASE_URL` is absent, the existing filesystem content-addressed queue remains the default.

When it is present, processing requires a reachable PostgreSQL queue. The claim is fail-closed and uses:

- source SHA-256 primary key;
- `SELECT ... FOR UPDATE` transaction;
- worker id (`host:pid:nonce`);
- bounded connection attempt;
- renewable lease;
- stale-lease takeover;
- completed receipt;
- retryable state when a worker exits before completion.

The queue contains hashes and operational state, not source document text. The current direct PostgreSQL transport uses `NoTls`; production deployment must therefore keep it on localhost or a separately secured private/VPN network until a mutually authenticated queue service/TLS transport is delivered.

## 5. Calendar auto-maintenance

The signed calendar feed is checked automatically, but no more than once every 24 hours. The attempt record is written atomically. Any network, signature or content failure leaves the bundled/last verified calendar active.

This removes the need to press an update button every launch, but it does not invent future official calendars. Publishing a signed complete next-year package remains an operational responsibility.

## 6. Observability

The automation panel now displays:

- active queue mode and reachability;
- documents reused from checkpoints;
- documents rendered again;
- the existing zero-touch, attention, print and shadow-mode metrics.

## 7. Verification boundary

The source contracts, frontend tests and static gates are green. The Rust toolchain was unavailable in the execution environment, so the new Rust code is not claimed compiled. A production build must pass the exact Cargo/RustSec/Tauri/Windows sequence in `RELEASE_VERIFICATION_V18_2_1.md`.
