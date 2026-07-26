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

## Проверено в доступной среде

- `python -m pytest -q`: 212 passed.
- Isolated shard runner: 33 modules, 212 passed, source fingerprint unchanged.
- DOCX visual goldens: 7 fixtures passed.
- Rust production panic/source audit and security backport policy: passed.
- `python scripts/static_quality_gate.py --source-only`: passed.
- Полная граница проверки и неисполненные внешние gates указаны в `CURRENT_VERIFICATION_STATUS.txt`.

## Статус

Это исправленный source-checkpoint. Production-релиз допускается только после свежего Rust/frontend CI, подписанного NSIS, production sidecars и Windows Word/printer/reboot evidence.
