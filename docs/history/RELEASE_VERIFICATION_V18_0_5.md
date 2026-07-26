# Release verification — Dokkomplekt Universal 18.0.5

## Статус

**Source candidate verified, Rust/Windows runtime gate pending.**

## Пройдено в текущей среде

- `npm run typecheck`;
- `npm test -- --run`: 18/18;
- `npm run build`;
- `python -m unittest discover -s tests -p 'test_*.py'`: 40/40;
- tree-sitter parsing: 104 Rust-файла, 0 синтаксических ошибок;
- проверка command surface для `start/activate/capture/apply/close_word_scanner`;
- проверка простого UI, автоматических предложений, повторного выделения и безопасной копии;
- архивная CRC и SHA-256 manifest проверяются после финальной очистки и упаковки.

## Fail-closed ограничение

В контейнере отсутствуют `cargo`, `rustc`, `rustfmt` и Clippy. Поэтому этот документ **не утверждает**, что изменённый Rust-код скомпилирован. `scripts/static_quality_gate.py` корректно завершает проверку ошибкой на Rust-этапе.

До выпуска установщика необходимо выполнить на Rust 1.85+:

```bash
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
```

## Обязательная Windows-проверка

Guided scanner использует Windows ShellExecute + Microsoft Word COM. Его нужно фактически проверить на Windows 10/11 с установленным Word. До такой проверки функция считается source-complete, но не Windows-runtime-verified.
