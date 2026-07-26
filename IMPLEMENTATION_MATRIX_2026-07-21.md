# Dokkomplekt Universal 18.3.2 — матрица выполнения задания

Фактическая проверка: **2026-07-23**. Это проверенный source checkpoint, но не подписанный production-релиз.

## Критические и высокоприоритетные требования

| № | Требование | Фактический статус |
|---|---|---|
| 1 | Required fields только из итогового generation plan | **Реализовано и скомпилировано.** Исключённые документы не участвуют в readiness/risk gate. |
| 2 | Bundle Decision Engine до learned promotion | **Реализовано и протестировано.** Уверенный route ограничивает комплект; при неуверенности создаётся одно понятное исключение. |
| 3 | Свежий Rust 1.97, lock/check/test/clippy/audit | **Выполнено.** Полный Tauri workspace check прошёл; 370 Rust-тестов; Clippy `-D warnings`; lockfile регенерирован Cargo 1.97.0. RustSec: 0 уязвимостей, точечные принятые transitive-риски Tauri документированы. |
| 4 | Полный production-sidecar runtime | **Не поставлен.** Component manager и fail-closed manifest есть, но доверенные binaries/model weights отсутствуют. |
| 5 | Постраничный OCR mixed PDF | **Реализовано и протестировано.** OCR принимается по каждой странице, сохраняются page/layout/evidence и orientation/PSM attempts. |
| 6 | Template Intelligence Wizard | **Реализовано, UI подключён, Rust и frontend проверены.** Пустой DOCX + примеры → карта полей → обязательное подтверждение. |
| 7 | Watcher устанавливается Windows E2E | **Harness реализован; реальный Windows reboot не выполнен.** |
| 8 | Tauri/Word/printer/reboot/installer E2E | **Browser E2E 2/2 выполнен; аппаратный Windows Regression Wall не выполнен.** |
| 9 | Case Segmentation Engine | **Реализовано и протестировано.** Несколько людей/организаций/дел блокируют автоматическое смешивание. |
| 10 | OCR/layout/рукопись | **Частично.** Layout/таблицы/координаты усилены; handwriting остаётся risk/attention без поставленной handwriting-модели. |
| 11 | Честная поддержка форматов | **Реализован fail-closed capability routing.** DOC/PPT требуют LibreOffice; CAD/БД/защищённые и неизвестные форматы не обещаются. |
| 12 | Единый Rust toolchain | **Реализовано и проверено:** Rust 1.97.0. |
| 13 | XLSX sheets/formulas/conflicts | **Реализовано и протестировано.** |
| 14 | Нетехнический первый запуск | **Реализовано; Vitest и Playwright прошли.** |
| 15 | Готовые нормативные пакеты | **Только честные workflow blueprints.** Утверждённые формы нельзя выдумывать без владельца нормативного контента. |
| 16 | Автономная печать | **Не завершена внешне.** Маршруты fail-closed; Word/Sumatra/LibreOffice и реальный printer proof отсутствуют. |
| 17 | Central queue exact-once | **Код и моделирующие тесты есть; две реальные машины/PostgreSQL/SMB не проверены.** |
| 18 | Ежегодные календари | **Подписанный import/update и freshness gate сохранены; доверенный feed должен быть настроен владельцем.** |

## P0 — центральный интеллектуальный слой

- **Bundle Decision Engine:** реализован и скомпилирован.
- **Template Intelligence Wizard:** реализован, скомпилирован и подключён к UI.
- **Case Segmentation Engine:** реализован; неоднозначные multi-case источники fail-closed.
- **Page-level OCR/layout:** реализован; реальные OCR-бинарники не вложены.
- **Windows Regression Wall:** исполняемый harness усилен, но Windows/Word/printer/reboot evidence отсутствует.
- **Реальные анонимизированные корпуса:** измерительный инструментарий есть; корпуса и реальные метрики не выдуманы.

## P1 — коммерческий продукт

Организационные знания, template regression checks, локальная quality telemetry, exceptions dashboard,
process blueprints, evidence UI и multilingual semantic configuration реализованы. Конкретные CRM/SharePoint/
ЭДО/МИС/КЭП connectors требуют API, credentials, SDK и договоров соответствующих поставщиков.

## Проверенные контуры текущего дерева

- Rustfmt — passed.
- Full Rust/Tauri `cargo check --workspace --all-targets --locked --offline` — passed.
- Rust tests — **370 passed**.
- Full Clippy `-D warnings` — passed.
- RustSec — 0 vulnerabilities; accepted transitive advisories listed in `RUSTSEC_ACCEPTED_RISKS.md`.
- Python — **190 passed**.
- TypeScript — passed.
- Vitest — **36 passed**.
- Vite build — passed.
- Playwright — **2 passed**.
- npm production audit — 0 vulnerabilities.
- Static source gate — passed.

## Неподделанные внешние блокеры релиза

1. Trusted production sidecars, OCR language packs, local model runtime and model weights.
2. Authenticode certificate, signed NSIS installer and signed update/component manifests with production keys.
3. Clean Windows VM proof: installation, real reboot, watcher, Word COM, printer queue, PrintService and uninstall.
4. Real multi-machine PostgreSQL/shared-folder run.
5. Approved legal/medical/HR/accounting templates and real anonymized corpora with measured quality.
6. PDF/A certification, qualified electronic signature and deployment-specific 152-ФЗ controls.
7. Credentials and SDKs for named CRM/SharePoint/ЭДО/МИС/КЭП providers.
8. Guaranteed handwriting, CAD, databases and protected/unknown formats.
