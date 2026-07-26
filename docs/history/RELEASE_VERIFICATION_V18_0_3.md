# Release verification — Dokkomplekt Universal 18.0.3

## Подтверждено в текущей среде

- TypeScript: `tsc --noEmit` — успешно.
- Vitest: 14/14 — успешно.
- Frontend production build — успешно.
- Python regression/source contracts — успешно.
- Пер-document print DTO и UI wiring — проверены контрактными тестами.
- Cursor scanner для источника и загруженных DOCX/DOCM-шаблонов, а также приоритет пользовательских значений — проверены контрактами.
- Медицинские donor-поля изолированы от универсального core.

## Не подтверждено здесь

В контейнере отсутствуют `cargo`, `rustc`, `rustfmt` и `clippy`. Поэтому изменённые Rust-файлы не имеют нового Cargo-gate marker. Перед сборкой установщика обязательно выполнить:

```bash
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
```

Только успешный Rust gate разрешает считать исходник кандидатом на Windows NSIS release.
