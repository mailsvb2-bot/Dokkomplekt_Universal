# Dokkomplekt Universal 18.1.1 — release verification status

## Статус

**SOURCE CANDIDATE — RUST GATE REQUIRED.**

18.1.1 меняет watcher, filesystem locking, retention, печать, learned rules и release-sidecar контур. Эти изменения затрагивают Rust/Tauri backend, поэтому логи и marker любых предыдущих версий не подтверждают это дерево.


## Доступные проверки этого дерева

- Python regression/source contracts: **97/97**;
- TypeScript typecheck: пройден;
- Vitest: **31/31**;
- production frontend build: пройден;
- npm audit: **0 vulnerabilities**;
- reference-data freshness: 2026 complete, 2027 provisional с fail-closed дедлайном 1 октября;
- static source gate: **78 Tauri-команд, 107 Rust-файлов**;
- full Rust gate: корректно остановлен, потому что `cargo` отсутствует.

Ни `.cargo-gate`, ни утверждение `RUST_GATE_PASSED` в source-кандидат не включаются.

## Обязательный production gate

На чистом checkout с Rust 1.85.1:

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
npm run test
npm run build
```

Полный Windows installer дополнительно требует:

```bat
set DOKKOMPLEKT_SIDECAR_MANIFEST=C:\secure-release\windows-x86_64.json
BUILD_WINDOWS_INSTALLER.bat
```

Manifest обязан содержать лицензированно допустимые и SHA-256-проверенные Tesseract rus+eng, Poppler, LibreOffice, SumatraPDF, llama.cpp runtime и GGUF. `assert_offline_runtime_ready.py --require-semantic-model` блокирует неполный installer.

## Обязательный Windows E2E

- чистая Windows 10/11 без toolchain;
- NSIS install/upgrade/rollback/uninstall;
- watcher после перезагрузки;
- локальный и SMB/UNC intake без двойной обработки;
- DOCX/DOCM/PDF/scanned PDF/XLSX;
- Word image placeholders в разных story ranges;
- SumatraPDF и Word printing на реальных драйверах, printers, trays и duplex;
- crash/recovery, `_обработано`, retention и повтор attention-case;
- локальный llama.cpp/Ollama с реальным обезличенным корпусом.

## Честные границы

«Универсальный» означает расширяемый форматный и доменный конвейер с безопасным fallback, а не гарантированное понимание любого текста. Risk-gate специально предпочитает короткое подтверждение молчаливой ошибке. Юридически значимая ЭП, сертифицированный PDF/A-1A и СЭМД/ЕГИСЗ в 18.1.1 не объявляются готовыми.
