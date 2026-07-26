# Отчёты проверки 18.4.1

## Тулчейн
Прогоны на Rust 1.91 (максимум из репозитория Ubuntu 24.04;
static.rust-lang.org недоступен). Проект требует 1.97 — в поставке
rust-version не изменён. Перед релизом повторить на 1.97.

## Результаты (все коды возврата зафиксированы)

| Проверка | Результат |
|---|---|
| `cargo test --workspace` | WORKSPACE_TEST_EXIT=0, **401 passed, 0 failed** |
| `cargo fmt --all -- --check` | FMT_EXIT=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | CLIPPY_EXIT=0, 0 warnings |
| `pytest tests` | 204 passed |
| чистая сборка из архивной копии | CLEAN_EXIT=0, ядро 291 passed |

## Новое в 18.4.1
- CalibratedFloor: калибровка доходит до гейта генерации (двойной гейт закрыт)
- 5 тестов безопасности калибровки
- corpus_simulation.rs: воспроизводимый генератор корпуса
- verification/18.4.1/calibration/: реальные откалиброванные артефакты
