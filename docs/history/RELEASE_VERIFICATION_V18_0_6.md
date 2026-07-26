# Release verification — Dokkomplekt Universal 18.0.6

## Выполнено в текущей среде

- `npm run typecheck` — успешно;
- `npm test` — 24/24 теста успешно;
- `npm run build` — production frontend build успешно;
- `python -m unittest discover -s tests -p 'test_*.py' -v` — 50/50 тестов успешно;
- TypeScript ↔ Rust command registry — проверяется static quality gate и API contract test (54 команды);
- статическая проверка баланса Rust delimiters/strings/comments — 104 файла без обнаруженных нарушений;
- clean-source повторяет 50/50 Python tests и Rust source sanity;
- итоговый source archive повторно распаковывается, проверяется CRC, manifest SHA-256 и отсутствие build/cache directories.

## Не выполнено и не подменено статикой

В контейнере отсутствуют Rust toolchain и доступ к сети. Поэтому не выполнены:

- `cargo fmt --all -- --check`;
- `cargo check --workspace --all-targets --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `cargo test --workspace --locked`;
- настоящий Windows Word COM smoke-test.

Актуальный `.cargo-gate/CARGO_GATE_PASSED.ok` отсутствует. `scripts/assert_release_ready.py` обязан блокировать коммерческую упаковку до реального Rust gate. Этот ZIP является исправленным source-кандидатом, а не доказанным Windows-релизом.
