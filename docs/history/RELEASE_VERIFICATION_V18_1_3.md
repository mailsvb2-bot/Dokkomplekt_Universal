# Release verification — Dokkomplekt Universal 18.1.3

## Проверено на рабочем дереве

- Python regression/source contracts: **123/123**.
- Vitest UI/user scenarios: **32/32**.
- TypeScript `tsc --noEmit`: пройден.
- Vite production build: пройден.
- `npm audit --audit-level=high`: один прогон на финальном рабочем дереве завершился с **0 уязвимостей**. Повтор из отдельно распакованного ZIP не дал результата: registry вернул HTTP 502.
- Static source gate: пройден, **81 Tauri-команда**, **110 Rust-файлов**, Cargo не исполнялся.
- Production Rust panic-shortcut source audit: пройден.
- Reference-data freshness: пройдена на 20 июля 2026 года; 2027 остаётся provisional.
- Три content-pack manifest: пройдена проверка схемы, hashes и draft-only policy.
- 11 DOCX starter templates: ZIP/OOXML открывается, SHA-256 совпадает, предупреждение видно в документе.

## Обязательные гейты, не выполненные в этой среде

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit --deny warnings
npm run tauri build
```

Также обязательны Authenticode, NSIS и self-hosted Windows hardware E2E: Word, изображения, реальные принтеры, duplex/tray, watcher, перезагрузка, установка и обновление.

До появления этих артефактов версия остаётся `SOURCE_RUST_GATE_REQUIRED` и не является готовым production-релизом.
