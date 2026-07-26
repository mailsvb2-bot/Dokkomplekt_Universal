# Dokkomplekt Universal 18.1.0 — release verification status

## Итоговый статус

**SOURCE CANDIDATE — RUST GATE REQUIRED.**

Версия 18.1.0 существенно изменяет Rust/Tauri backend относительно доказанного дерева 18.0.8. В текущей среде отсутствует `cargo`, поэтому старые логи и marker 18.0.8 не используются как доказательство нового дерева. Коммерческий/production-релиз разрешён только после отдельного зелёного прогона на Rust 1.85.1.

## Что фактически проверено на текущем дереве

- `python -m unittest discover -s tests -p 'test_*.py'`: **81/81**;
- `npm run typecheck`: успешно;
- Vitest: **31/31**, 7 test files;
- `npm run build`: успешно;
- `npm audit --audit-level=moderate`: **0 vulnerabilities**;
- `python scripts/static_quality_gate.py --source-only`: успешно;
- зарегистрировано **76 Tauri-команд**;
- source-gate обработал **106 Rust-файлов**;
- tree-sitter Rust parser: **106/106 файлов без синтаксических ошибок**;
- full static quality gate корректно завершился fail-closed: `cargo is not available`.

Точные логи находятся в `verification/*_v18_1_0.log`.

## Что обязательно проверить до production-маркировки

На чистом checkout и с `rust-toolchain.toml`:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit --deny warnings
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
npm ci
npm run typecheck
npm test -- --run
npm run build
npx tauri build --bundles nsis
```

После этого обязательны Windows 10/11 E2E:

1. чистая машина без Rust/Node/Python;
2. установка NSIS;
3. запуск UI и watcher;
4. перезагрузка и проверка автозапуска;
5. DOCX/DOCM/PDF/scanned-PDF/XLSX intake;
6. Ollama/llama.cpp loopback transport;
7. Word image insertion;
8. printer selection, tray, duplex and copies;
9. crash during staging and resume/retry;
10. upgrade/rollback/uninstall.

## Граница доказанности отдельных функций

### SemanticModel

Встроен transport к локальному Ollama или OpenAI-compatible llama.cpp server. Веса модели и runtime не вложены. Принимаются только loopback-адреса; proxy и redirects запрещены. Model values проходят evidence/type/checksum validation. Точность понимания «любого документа» не объявляется доказанной без реального обезличенного корпуса и калибровки confidence.

### Sidecars

Есть hash-verified staging, runtime discovery и Tauri resource wiring. Source-кандидат не содержит сторонних executable/DLL/model files. Полный офлайн installer должен быть собран только из юридически допустимых, проверенных vendor packages с manifest SHA-256.

### PDF/A и КЭДО

LibreOffice создаёт PDF/PDF-A-кандидат. Соответствие PDF/A-1A не подтверждается без veraPDF/профильной проверки. КЭДО-пакет содержит XML manifest, SHA-256 и detached-signature slots, но не создаёт фиктивную КЭП.

### Печать и изображения

Backend хранит printer/duplex/tray preferences и содержит Windows Word/PrintQueue route. Реальная совместимость зависит от Word, Windows print driver и устройства; обязательна аппаратная E2E. `{{image ...}}` реализован fail-closed, но нуждается в проверке разных Word story ranges и сложных шаблонов.

### Content packs

Поставляются workflow-каркасы HR/legal/accounting и validator. Реальные нормативные DOCX/DOCM отсутствуют; validator запрещает статус `pilot/approved` с пустыми или непроверенными слотами.

## Почему архив не называется RUST_GATE_PASSED

Release fingerprint имеет смысл только когда marker сформирован после `fmt/check/clippy/test` на том же неизменённом дереве. Поскольку текущая среда не смогла выполнить Cargo, честное имя артефакта — `SOURCE_RUST_GATE_REQUIRED`.
