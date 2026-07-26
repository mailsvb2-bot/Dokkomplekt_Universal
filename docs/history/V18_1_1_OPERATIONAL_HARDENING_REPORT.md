# Dokkomplekt Universal 18.1.1 — operational hardening report

## Исправлено

- успешные источники атомарно архивируются в `_обработано/YYYY-MM`, рядом создаётся SHA-256 receipt;
- текущие и legacy `.dokkomplekt-processed` мигрируются и очищаются;
- attention/«НЕ ПРОЧИТАН» заметки получают retention и служебный архив;
- дедуп повторов использует SHA-256 содержания, а не `mtime`;
- processing guard переведён на атомарную lock-directory, host identity, nonce и heartbeat;
- learned scanner rules ограничены fingerprint конкретного layout; точное совпадение получает 0.999 confidence, чужой layout правило не наследует;
- risk-gate получил пакетное подтверждение и безопасный повтор без отключения content marker/lock;
- PDF-печать на Windows использует проверенный SumatraPDF sidecar с printer/copies/duplex/tray и fail-closed ошибками;
- производственный календарь блокирует release с 1 октября, если следующий год остаётся provisional;
- workflow contract консолидирован в одном каноническом core-модуле;
- полный офлайн-установщик обязан повторно проверить SHA-256 Tesseract rus+eng, Poppler, LibreOffice, SumatraPDF, llama.cpp и GGUF-модель.

## Не подменено обещанием

- сторонние бинарники и модельные веса не включены в source-архив без лицензирования и точных SHA-256;
- Rust/Tauri слой не объявляется проверенным без `cargo fmt/check/clippy/test/audit` на Rust 1.85.1;
- КЭДО hand-off не является электронной подписью;
- PDF/A-кандидат не является подтверждённым PDF/A-1A;
- Windows Word/printer/watcher/NSIS E2E остаётся обязательным релизным gate.
