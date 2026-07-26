# Release verification — Dokkomplekt Universal 18.0.2

## Успешно выполнено

| Проверка | Результат |
|---|---:|
| `npm run typecheck` | PASS |
| `npm test -- --run` | PASS — 14/14 |
| `npm run build` | PASS |
| `python -m unittest discover -s tests -p 'test_*.py'` | PASS — 13/13 |
| статическая часть `scripts/static_quality_gate.py` | PASS |
| JSON/version/command parity | PASS |
| повторная распаковка, CRC и проверка всех SHA-256 manifest-записей | PASS — 219/219 |

## Обязательный gate, не выполненный в этом контейнере

`cargo metadata --locked`, `cargo fmt --check`, `cargo check --workspace --all-targets --locked`, строгий Clippy и `cargo test --workspace --locked` не запускались: Rust toolchain отсутствует и не может быть загружен из текущей изолированной среды.

Это не скрыто и не заменено фиктивным marker-файлом. `.cargo-gate/CARGO_GATE_PASSED.ok` из 18.0.1 не относится к новым исходникам и удалён. Перед созданием установщика необходимо выполнить:

```bash
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
```

Только успешное выполнение этих команд создаёт актуальный Cargo marker и разрешает коммерческую упаковку.
