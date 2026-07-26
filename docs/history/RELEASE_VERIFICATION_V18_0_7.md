# Release verification — Dokkomplekt Universal 18.0.7

> Исторический отчёт версии 18.0.7. Он не описывает текущее дерево 18.0.8 и не должен использоваться как актуальное доказательство; см. `RELEASE_VERIFICATION_V18_0_8.md`.


Дата проверки: 16 июля 2026 года.

## Подтверждено в текущей среде

- Python regression contour: **62/62**.
- Vitest frontend contour: **30/30**.
- Комбинационные матрицы внутри Python-контуров: **2160** сценариев (1152 scanner + 1008 popup).
- TypeScript `tsc --noEmit`: успешно.
- Production frontend build: успешно.
- `npm audit --audit-level=moderate`: 0 известных уязвимостей соответствующего уровня.
- Структурный source gate: успешно; версия, обязательные файлы и exact parity Tauri/TypeScript проверены.
- Зарегистрировано и синхронизировано **55** Tauri-команд.
- Tree-sitter syntax parse: **103/103 Rust-файла** без синтаксических ERROR/missing nodes. Это не заменяет Cargo type checking.
- Update manifest generator проверен временной Ed25519-парой: подпись и canonical JSON воспроизводимы.
- Финальный source ZIP: CRC, безопасные пути и каждый SHA-256 из внутреннего manifest проверены после упаковки.

Фактические логи лежат в каталоге `verification/` внутри source archive. SHA-256 каждого исходного файла перечислен в `SOURCE_MANIFEST_SHA256.txt`.

## Не выполнено локально и не выдано за выполненное

В среде отсутствует Rust toolchain. Поэтому локально не запускались:

- `cargo metadata --locked`;
- `cargo fmt --all -- --check`;
- `cargo check --workspace --all-targets --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `cargo test --workspace --locked`.

Также Linux-среда не подтверждает Microsoft Word COM, Windows NSIS installer smoke и реальную установку/обновление на чистой Windows-машине.

## Release policy

GitHub workflows блокируют выпуск, пока настоящий Rust gate не создаст `.cargo-gate/CARGO_GATE_PASSED.ok` с fingerprint текущих исходников. Installer jobs повторяют Rust gate на каждом целевом runner. Windows release дополнительно выполняет NSIS/offline smoke, а Word COM должен подтверждаться Windows acceptance-контуром.
