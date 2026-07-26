# Dokkomplekt Universal 18.1.2 — release verification status

## Статус

`SOURCE_RUST_GATE_REQUIRED`.

Версия 18.1.2 меняет Rust core, Tauri watcher, DOCX renderer, SemanticModel, storage metrics, signed reference-data transport, печать и release pipeline. Маркеры или логи предыдущих версий это дерево не подтверждают. `.cargo-gate/CARGO_GATE_PASSED.ok` и `.release-gate/WINDOWS_HARDWARE_E2E_PASSED.json` намеренно отсутствуют.

## Что фактически проверено на финальном рабочем дереве

- `python -m unittest discover -s tests -p 'test_*.py' -v`: **115/115**;
- `npm run typecheck`: пройден;
- `npm run test -- --run`: **31/31**;
- `npm run build`: пройден;
- `npm audit --offline --audit-level=moderate`: **0 уязвимостей**;
- `python scripts/audit_rust_production_panics.py`: пройден — прямых production `unwrap()/expect()/panic!/todo!/unimplemented!` не найдено;
- `python scripts/static_quality_gate.py --source-only`: пройден — **81 Tauri-команда, 109 Rust-файлов**;
- YAML всех GitHub Actions workflow: разобран;
- freshness производственного календаря: пройден на текущую дату.

Эти проверки не заменяют компиляцию Rust.

## Честный результат обязательного gate

`scripts/prepackage_rust_gate.sh` был запущен на текущем дереве и завершился fail-closed:

```text
ERROR: cargo is required before packaging. Install Rust 1.85+ and rerun.
```

`python scripts/assert_release_ready.py` также правильно отказался объявить релиз готовым из-за отсутствия `.cargo-gate/CARGO_GATE_PASSED.ok`.

## Обязательный production gate

На Rust 1.85.1 необходимо выполнить без исключений:

```text
cargo metadata --locked --format-version 1
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit --deny warnings
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
```

Затем на выделенном self-hosted Windows runner:

- собрать и Authenticode-подписать основной EXE;
- собрать и подписать NSIS;
- проверить подпись установленного EXE;
- выполнить Word COM и PDF print submission на выделенный принтер;
- проверить printer/duplex/tray;
- установить приложение, проверить повторный запуск и watcher/autostart;
- выполнить реальную проверку после перезагрузки Windows;
- сформировать `.release-gate/WINDOWS_HARDWARE_E2E_PASSED.json`.

Созданные workflow и PowerShell harness — это исполняемый gate, но они не считаются пройденными, пока self-hosted runner не выпустил зелёный подписанный artifact.

## Runtime dependencies

Полный offline installer разрешён только при наличии SHA-256-проверенного набора Tesseract rus+eng, Poppler, LibreOffice, SumatraPDF, llama.cpp и GGUF. Runtime bundle должен иметь SBOM и detached Ed25519-подпись, проверенную заранее закреплённым public key. Source-кандидат не включает эти сторонние бинарники и веса.

## Границы multi-machine

Общая content-addressed очередь уменьшает риск двойной обработки на обычных SMB/NFS: claim и completion адресуются SHA-256 и имеют heartbeat/stale recovery. Это не распределённый consensus и не центральный сервер очереди. Для нескольких площадок, недоверенных сетевых FS или строгой exactly-once семантики нужен отдельный queue service.

## Юридические границы

Шифрование не равно автоматическому соответствию 152-ФЗ. PDF/A-кандидат не является сертифицированным PDF/A-1A без независимой veraPDF-проверки. КЭДО hand-off и `.sgn` slots не являются юридически значимой электронной подписью.
