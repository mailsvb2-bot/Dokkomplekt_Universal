# Dokkomplekt Universal 18.2.0 — Signed Component Delivery

Версия 18.2.0 отделяет тяжёлый локальный runtime от базового установщика без ослабления доверия.

## Каталог и trust anchor

`components-catalog.json` содержит подписанный payload: schema, minimum app version, publication time, allow-list доменов и descriptors `ocr/office/semantic`. Подпись проверяется тем же отдельным update Ed25519 public key, который закреплён в бинарнике. Release pipeline отклоняет каталог, если private signing key не соответствует закреплённому public key. Подписанный каталог с более старым `published_at` не заменяет уже принятый каталог.

## Per-user runtime

Компоненты хранятся без административных прав:

- Windows: `%LOCALAPPDATA%\Dokkomplekt\components`;
- Linux: `$XDG_DATA_HOME/dokkomplekt/components` или `~/.local/share/dokkomplekt/components`;
- macOS: `~/Library/Application Support/Dokkomplekt/components`.

Эта папка проверяется первой в `resolve_tool`, но файл считается доступным только после полной цепочки проверки.

## Цепочка проверки

1. Ed25519 подписанного каталога;
2. app minimum version и target;
3. HTTPS-only URL, SSRF-filter, DNS pinning и signed host allow-list;
4. точный размер и SHA-256 ZIP до распаковки;
5. запрет абсолютных путей, `..`, symlink, duplicate path и zip-bomb;
6. SHA-256 `component-files.json`, закреплённый descriptor;
7. SHA-256 каждого файла;
8. атомарный rename staging → active;
9. повторная проверка конкретного файла при каждом `resolve_tool`.

## UI

Раздел «Автоматизация → Зависимости» показывает `bundled / downloaded / system / missing`. При missing функция предлагает разовую загрузку с подписанным размером и показывает `component://progress`. Отказ просто отменяет функцию. Системные и встроенные инструменты используются без повторной загрузки. Удаляется только downloaded-пак.

## Release artifacts

`scripts/build_component_packs.py` детерминированно строит `ocr`, `office`, `semantic`, внутренние manifests, `components-catalog.json`, detached signature и SHA-256. Thin installer и offline installer используют один verified staging; различается только включение тяжёлых ресурсов.

## Не утверждается

SOURCE-архив не содержит реальные сторонние бинарники или GGUF-веса. Rust/Windows gate текущего дерева и release-сборка на доверенной машине обязательны.
