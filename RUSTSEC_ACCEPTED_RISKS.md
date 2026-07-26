# RustSec accepted transitive risks

Reviewed: 2026-07-23  
Mandatory re-review: 2026-10-01, on every Tauri/Wry upgrade, or when any listed crate version changes.

The release gate remains `cargo audit --deny warnings`. Only the exact advisory IDs in
`.cargo/audit.toml` are accepted; a new advisory or a changed dependency remains fatal.

## Tauri Linux GTK3 closure

Tauri 2.11.5 / tauri-runtime-wry 2.11.4 / Wry 0.55.1 currently pull gtk-rs 0.18 and
WebKitGTK bindings on Linux. RustSec marks the GTK3 bindings unmaintained:

- RUSTSEC-2024-0411 through RUSTSEC-2024-0420, excluding 0429;
- RUSTSEC-2024-0370 (`proc-macro-error`, build-time through glib/gtk macros).

These crates are not used by the Windows runtime. Linux packages must remain opt-in and
must be rebuilt and retested when Tauri provides a maintained replacement backend.

## `glib::VariantStrIter` unsound advisory

RUSTSEC-2024-0429 affects `glib 0.18.5` iterator implementations. The application does not
call `VariantStrIter` directly; `glib` is transitive through Tauri's Linux GTK/WebKit stack.
This is an accepted Linux-only transitive risk, not a claim that the advisory is fixed.

## UNIC crates through Tauri URL pattern parsing

RUSTSEC-2025-0075, 0080, 0081, 0098 and 0100 mark UNIC 0.9 crates unmaintained. They are
transitive through `urlpattern -> tauri-utils`. These are maintenance warnings rather than
reported memory-safety vulnerabilities. They remain pinned by `Cargo.lock` and must be
removed when Tauri replaces that dependency chain.

## Evidence

The exact reverse dependency chains are recorded in
`verification/rustsec-transitive-chains.txt`. The current audit uses the official RustSec
advisory database commit recorded in `verification/rustsec-advisory-db-commit.txt`.
