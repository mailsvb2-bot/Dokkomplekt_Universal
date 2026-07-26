# Release verification — Dokkomplekt Universal 18.0.4

## Подтверждается в этой среде

- синхронизация версии npm/Tauri/Rust/Python manifest;
- TypeScript typecheck;
- Vitest UI/API/DTO scenarios;
- production frontend build;
- Python regression/source contracts;
- Rust tree-sitter syntax parse;
- единый popup для документа и комплекта;
- пользовательский popup designer и scanner-to-question flow;
- профильные вопросы для нескольких профессий;
- fresh-case reset при новом источнике;
- чистота итогового source archive и SHA-256 manifest.

## Не подтверждается без Rust toolchain

В контейнере отсутствуют `cargo`, `rustc`, `rustfmt` и `clippy`. Поэтому изменённый Rust-код не получает `.cargo-gate/CARGO_GATE_PASSED.ok`. Перед NSIS-релизом обязательно выполнить:

```bash
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
```

Только этот прогон подтверждает `cargo fmt/check/clippy/test` на Rust 1.85+.
