# Release verification — Dokkomplekt Universal 18.0.0

Дата проверки: 15.07.2026.

## Проверенное окружение

- Rust `1.85.0`;
- Cargo `1.85.0`;
- Clippy `0.1.85`;
- Node.js `22.16.0`;
- npm `10.9.2`;
- Linux build-host с системными библиотеками Tauri/GTK/WebKit.

## Rust gate

Фактически выполнены:

```text
cargo metadata --locked
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Результат:

- **230 Rust-тестов пройдено**;
- 0 падений;
- 0 предупреждений строгого Clippy;
- скомпилированы Tauri shell, core, DOCX, storage, morph, refdata, license core/server/Python binding;
- итоговый source fingerprint: `78d22e7aa5649620b6246462d12d23cd24445a1cfb41ec3fb860af22d5d44254`.

## Frontend и release-контракты

- TypeScript typecheck — успешно;
- Vitest — **14/14**;
- Python release/fingerprint tests — **2/2**;
- production Vite build — успешно;
- статический TS↔Tauri command contract — успешно;
- npm audit offline — **0 известных уязвимостей в lockfile**.

## Browser UI E2E

```text
PLAYWRIGHT_CHROMIUM_EXECUTABLE=/usr/bin/chromium npm run e2e
```

Результат: **2/2**.

Это browser UI E2E с mock Tauri IPC. Настоящие Rust-команды и backend отдельно подтверждены компиляцией и 230 Rust-тестами; browser-тест не выдаётся за desktop IPC E2E.

## Новые проверенные сценарии 18.0.0

- строгие вложенные `if/unless/else`;
- коллекции, `each`, `sum`, `count` и формулы с фиксированной точностью;
- клонирование полных строк таблиц DOCX;
- извлечение Word-таблиц в `items[]`;
- безопасная подтверждаемая разметка DOCX/DOCM;
- SQLite-счётчики и библиотека блоков;
- CSV/TSV mail-merge с атомарной публикацией и тарифным учётом;
- VIN checksum;
- контроль расчётного и корреспондентского счёта в связке с БИК;
- локальная проверка предложений внешней LLM;
- обратная совместимость со старыми плейсхолдерами и профильными медицинскими контрактами.

## Честные границы релиза

Не объявлены готовыми:

- Windows NSIS `setup.exe` — должен собираться и проверяться на Windows runner;
- XLSX mail-merge;
- вставка изображений/печатей/факсимиле;
- полные официальные базы БИК/ОКВЭД/адресов;
- PDF/A, КЭДО, КриптоПро и Госключ;
- встроенная локальная LLM;
- СЭМД/ЕГИСЗ;
- требуемый ТЗ 24-часовой fuzz-run — инфраструктура для него должна быть отдельным долгим CI job.

Файл `.cargo-gate/CARGO_GATE_PASSED.ok` создан после полного gate и проверен `scripts/assert_release_ready.py`.
