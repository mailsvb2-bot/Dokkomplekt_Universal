# Dokkomplekt Universal 18.4.3 — corrective repair report

## Исправлено

- Восстановлен реальный путь запуска `main.bat`/`main.py`: готовый EXE запускается сразу, исходники запускаются через `npm run tauri:dev`, ошибки пишутся в `launcher_logs/last_launch.log`.
- Синхронизированы `VERSION`, npm, Tauri, Cargo crates и Python binding на 18.4.3; активный Rust toolchain закреплён на 1.97.1.
- Все действующие CI и installer-gates переведены с неполного `unittest discover` на изолированный полный pytest-контур; `pytest` добавлен в dev requirements.
- Для дневников и выписных документов заменён поиск слова «врач» на проверку реальной строки подписи; дневники требуют и создают подписи лечащего врача и заведующего отделением.
- Номер и дата создаваемого документа больше не наследуются молча из исходного договора/документа: поля запрашиваются заново.
- ИНН из одинаковых цифр (`0000000000`, `111111111111` и аналоги) блокируются до checksum-доверия.
- License server и Python native binding, исключённые из desktop workspace, получили обязательный изолированный Rust gate: `fmt`, `check`, `clippy -D warnings`, `test`, RustSec audit. Его lock/audit evidence криптографически связывается с release attestation.
- Детерминированный source-archive builder формирует новый SHA-256 manifest и проверяет CRC, безопасные пути, дубликаты и каждый файл архива.

## GitHub Actions repair

- Обновлён уязвимый `pyo3 0.24.2` до исправленного `0.29.0` в Python binding и workspace policy.
- `cargo-audit` обновлён до версии `0.22.2`, поддерживающей новые RustSec-записи с CVSS 4.0; security gate не ослаблен.
- Rust compile gate на Ubuntu теперь устанавливает обязательные GLib/GTK/WebKitGTK development-пакеты до сборки Tauri.
- Устранены неотформатированные Rust-файлы как в основном workspace, так и в отдельно проверяемых коммерческих crates.
- Исправлены устаревшие проверки подписей дневников; compatibility-путь теперь формирует каноническую строку `Заведующий отделением` и не добавляет её повторно, если полная строка уже существует.
- Устранён Clippy-дефект `manual_pattern_char_comparison` в разборе PostgreSQL URL license-server.
- Исправлен Windows-only тест локального ключа: он больше не сравнивает DPAPI-зашифрованный файл с открытым 32-байтовым ключом, а проверяет DPAPI-конверт, отсутствие plaintext и успешное восстановление того же ключа.
- Для повторных CI-прогонов добавлен безопасный Cargo cache, а дублирующий одновременный запуск workflow по `push` и `pull_request` для одной ветки устранён.
- При падении Rust gate полный диагностический лог сохраняется как GitHub Actions artifact без `continue-on-error` и без ослабления обязательных проверок.

## Проверено в доступной среде

- `python -m pytest -q`: 212 passed.
- Isolated shard runner: 33 modules, 212 passed, source fingerprint unchanged.
- DOCX visual goldens: 7 fixtures passed.
- Rust production panic/source audit and security backport policy: passed.
- `python scripts/static_quality_gate.py --source-only`: passed.
- Полная граница проверки и неисполненные внешние gates указаны в `CURRENT_VERIFICATION_STATUS.txt`.

## Статус

Это исправленный source-checkpoint. Production-релиз допускается только после свежего Rust/frontend CI, подписанного NSIS, production sidecars и Windows Word/printer/reboot evidence.
