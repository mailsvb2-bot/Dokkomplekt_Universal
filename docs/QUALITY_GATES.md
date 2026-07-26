# Quality gates

Dokkomplekt deliberately separates source verification from release proof.

## Source gate

`npm run quality` runs TypeScript compilation, frontend tests, production web build,
all Python contracts and `static_quality_gate.py --source-only`. It is valid on a
review machine without Rust and explicitly reports `cargo_not_executed=true`.

## Release gate

`npm run quality:release` additionally requires a real Cargo toolchain and runs
metadata, rustfmt, workspace check, clippy with warnings denied and all Rust tests
with `--locked`. Packaging still requires the signed Cargo/RustSec/Windows evidence
checked by `scripts/assert_release_ready.py`.

A green source gate is never evidence of a releasable installer.
