# Release verification — Dokkomplekt Universal 18.0.1

Дата проверки: 15.07.2026.

## Проверенное окружение

- Rust `1.85.0`;
- Cargo `1.85.0`;
- строгий Clippy;
- Node.js и npm из Linux build-host;
- Linux build-host с системными библиотеками Tauri/GTK/WebKit.

## Полный Rust gate

Фактически выполнены на всём workspace:

```text
cargo metadata --locked --format-version 1
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Результат:

- **250 Rust-тестов пройдено**;
- 0 падений;
- 0 предупреждений строгого Clippy;
- проверены Tauri shell, универсальное ядро, DOCX, storage, morphology, refdata и лицензирование;
- source fingerprint: `af4e5da55a7f1eca43a189c630fb6693fc9b281a4db96b0374b3af88dad23948`;
- marker `.cargo-gate/CARGO_GATE_PASSED.ok` сверён через `scripts/assert_release_ready.py`.

## Frontend и Python-контракты

- TypeScript typecheck — успешно;
- Vitest — **14/14**, 4 тестовых файла;
- production Vite build — успешно;
- Python source-fingerprint/release tests — **2/2**;
- `git diff --check` — успешно;
- Windows DPAPI-вызовы отдельно типизированы против `windows-sys 0.61.2`.

## Что дополнительно закрыто в 18.0.1

- безопасное распознавание точного XML-элемента `<w:t>` без повреждения таблиц;
- вложенные строки DOCX-таблиц;
- строгие условия, формулы и незакрытая шаблонная разметка;
- fixed-point округление и контроль переполнения;
- контрольные цифры ИНН и корректный КПП;
- сохранение лишних CSV/TSV-ячеек;
- производственный календарь РФ 2026;
- DPAPI-защита локального ключа на Windows и миграция старого сырого ключа;
- безопасный откат последних резервирований счётчиков;
- профильные дневниковые серии, граница выпиской, финальная запись, подписи и пользовательские шаблоны 01–31;
- склонение составных, женских и множественных названий должностей.

## Не засчитано как выполненное

- Playwright browser E2E: bundled Chromium в контейнере отсутствует, а системный Chromium блокирует loopback политикой среды;
- настоящий Windows NSIS `setup.exe`;
- установка и обновление на чистых Windows 10/11;
- подпись кода, SmartScreen и антивирусная проверка;
- production updater, платежи и лиценз-сервер;
- пилот на реальных пользователях.

## Статус

18.0.1 является полностью проверенным **исходным кандидатом P0-hardening**. Это не подменяет отдельную проверку подписанного Windows-установщика и коммерческого контура.
