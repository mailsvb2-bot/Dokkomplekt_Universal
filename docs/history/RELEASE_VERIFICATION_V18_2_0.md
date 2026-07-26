# Release verification — Dokkomplekt Universal 18.2.0

## Статус

Текущее дерево является SOURCE-кандидатом до выполнения `cargo fmt/check/clippy/test/audit`, Tauri build, Authenticode/NSIS и Windows hardware E2E. Исторические marker-файлы других версий не подтверждают 18.2.0.

## Подтверждено в доступной среде

- 129 Python regression/source contracts;
- 34 Vitest UI/user scenarios;
- TypeScript typecheck и production frontend build;
- npm audit: one successful run on the unchanged package-lock reported 0 vulnerabilities; the final retry timed out waiting for the registry;
- static source gate: 85 Tauri-команд, 111 Rust-файлов;
- production panic-shortcut source audit;
- детерминированная тестовая сборка component ZIP, Ed25519-проверка каталога и внутренний SHA-256 manifest;
- UI-gating отсутствующего OCR и отсутствие лишней загрузки при `system`-состоянии;
- source-контракты HTTPS-only, DNS pinning, signed allow-list, target/size/hash/path-traversal guards;
- thin/offline/component artifact workflow contract.

## Обязательные release-проверки

1. `cargo fmt --all -- --check`;
2. `cargo check --workspace --all-targets --locked`;
3. `cargo clippy --workspace --all-targets --locked -- -D warnings`;
4. `cargo test --workspace --locked`;
5. `cargo audit --deny warnings`;
6. сборка и Authenticode-подпись application EXE;
7. сборка и подпись thin/offline NSIS;
8. сборка реальных `ocr/office/semantic` паков из легально полученного verified staging;
9. проверка совпадения component signing key с public key, встроенным в application binary;
10. Windows hardware E2E: установка, загрузка/удаление компонента, OCR, PDF, печать и перезагрузка watcher.

## Инварианты компонента

Компонент используется только при одновременном выполнении всех условий: подписанный каталог; подходящий target и min app version; валидный `component-status.json`; подписанный hash `component-files.json`; совпадение SHA-256 реально запускаемого файла. Простое наличие бинарника в пользовательской папке недостаточно.
