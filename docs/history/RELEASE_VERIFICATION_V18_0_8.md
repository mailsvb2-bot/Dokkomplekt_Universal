# Release verification — Dokkomplekt Universal 18.0.8

Дата проверки: 18 июля 2026 года.

Этот файл содержит только фактически полученные результаты текущего дерева исходников. Исторические логи других версий доказательством для 18.0.8 не считаются.

## Результаты текущего дерева

- Rust format: **PASSED** — `cargo fmt --all` и `cargo fmt --all -- --check`.
- Rust workspace check: **PASSED** — `cargo check --workspace --all-targets --locked`.
- Rust Clippy: **PASSED** — `cargo clippy --workspace --all-targets --locked -- -D warnings`; предупреждений нет.
- Rust workspace tests: **PASSED** — 307 тестов, 0 падений.
- Python regression contour: **PASSED** — 70 тестов, 0 падений.
- TypeScript typecheck: **PASSED** — `tsc --noEmit`.
- Vitest: **PASSED** — 31 тест, 0 падений.
- Frontend production build: **PASSED** — `tsc && vite build`.
- npm audit: **PASSED** — 0 найденных уязвимостей.
- Static source gate: **PASSED** — версия 18.0.8, 63 Tauri-команды, 105 Rust-файлов в чистом source-дереве.
- Mandatory Rust release marker: **PASSED** — `scripts/prepackage_rust_gate.sh`; fingerprint `ed567854f3de…` подтверждён `scripts/assert_release_ready.py`.
- Deterministic source archive: **PASSED** при финальной упаковке — ZIP CRC, безопасные пути, отсутствие дубликатов и все SHA-256 записи проверяются `scripts/build_source_archive.py`; внешний manifest и `.sha256` поставляются рядом с ZIP.

Подробные логи текущей версии находятся в `verification/*_v18_0_8.log`.

## RustSec dependency audit

В оба GitHub Actions workflow добавлен non-optional `cargo-audit 0.21.2`, совместимый с MSRV Rust 1.85, с запуском `cargo audit --deny warnings`.

На текущем изолированном Linux-host сам `cargo-audit 0.21.2` успешно собран, однако локальный анализ не завершён: host не смог получить актуальную базу `https://github.com/RustSec/advisory-db.git`. Это сетевое ограничение окружения, а не зелёный результат аудита. Локальный лог сохранён как `verification/cargo_audit_v18_0_8.log`; окончательным доказательством RustSec должен быть зелёный CI job `rust-dependency-audit` с доступом к актуальной advisory database.

## Внешние acceptance-проверки

Следующие проверки не подменяются Linux-компиляцией и остаются отдельным выпускным контуром:

- Microsoft Word COM на поддерживаемой Windows;
- реальная установка/обновление NSIS и полный offline installer;
- watcher после перезагрузки Windows;
- физическая печать на целевых принтерах, включая выбор принтера и параметры драйвера;
- поставка и запуск OCR/Poppler/LibreOffice sidecar на чистой пользовательской машине;
- визуальное golden-master сравнение реальных DOCX/DOCM в Microsoft Word.

Версия 18.0.8 является проверенным source-релизом исправленного существующего контура, но не заявляется завершённым универсальным автономным документным автопилотом.
