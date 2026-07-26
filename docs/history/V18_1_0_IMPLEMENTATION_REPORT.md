# Dokkomplekt Universal 18.1.0 — implementation report

## Цель прохода

Приблизить продукт к сценарию «специалист положил произвольный первичный документ — система сама собрала доказуемые факты, создала правильный комплект и замкнула печать/вывод», не заменяя безопасность ложными обещаниями.

## Реализовано

### 1. Arbitrary-document semantic bridge

- loopback-only transport к Ollama и OpenAI-compatible llama.cpp;
- ограничения URL, redirects, proxy, размера и времени ответа;
- exact evidence snippets из исходного текста;
- отбрасывание выдуманной evidence-цитаты;
- понижение confidence при отсутствии доказательства;
- типовая и checksum-валидация перед записью в semantic case;
- автоматический semantic extract после intake;
- UI confidence/evidence до генерации.

### 2. Zero-touch correctness

- attention/processed dedup по SHA-256 содержимого вместо `mtime`;
- повтор после изменения содержимого работает даже при сохранённом времени файла;
- реальный parser remaining placeholders;
- экранированные `\\{{...\\}}` считаются буквальным текстом;
- parser errors, unknown fields и missing values остаются fail-closed.

### 3. Intake and batch

- XLSX mail-merge по первому листу;
- сохранение пустых колонок и формульных значений;
- sidecar resolver для OCR/PDF/office/message converters;
- hash-verified offline staging script без network download;
- runtime dependency status в интерфейсе.

### 4. Output closure

- DOCX/DOCM → PDF/PDF-A-candidate через LibreOffice;
- printer inventory и сохраняемые printer/duplex/tray preferences;
- Windows Word/PrintQueue и CUPS options routes;
- `{{image field.id}}` и безопасная проверка локального image asset;
- атомарный КЭДО hand-off package с manifest и checksums.

### 5. Reliability and recovery

- Case Run state machine и зашифрованная история;
- interrupted-run recovery;
- stale staging cleanup;
- retry как новая атомарная попытка;
- startup recovery;
- scanner confirmations сохраняются как learned rules.

### 6. Template governance

- immutable snapshot каждой опубликованной версии;
- SHA-256 archived DOCX/DOCM;
- encrypted template-version metadata;
- list/history/rollback UI;
- rollback создаёт новую опубликованную версию, не стирая историю.

### 7. Scale/content architecture

- Tier-1 content-pack schema, catalog and validator;
- HR/legal/accounting workflow skeletons;
- запрет статуса `pilot/approved` без реальных verified templates и reviewer;
- отсутствие встроенных чужих текстов в универсальном core.

## Намеренно не подменено заглушками

- модель и веса не вложены;
- OCR/LibreOffice vendor trees не вложены;
- КриптоПро/Госключ не имитируются;
- ЕГРЮЛ/ЕГРИП/СЭМД/ЕГИСЗ не заявлены без реального провайдера и договора API;
- PDF/A-1A не объявляется сертифицированным;
- отраслевые каркасы не выдаются за юридически утверждённые документы;
- Windows hardware E2E не заменён строковыми тестами;
- Rust compilation не заменена tree-sitter parsing.

## Следующий обязательный gate

1. Cargo fmt/check/clippy/test/audit на Rust 1.85.1.
2. NSIS build.
3. Clean-Windows installer/upgrade/uninstall.
4. Word/watcher/reboot/print crash matrix.
5. Реальный обезличенный document corpus и confidence calibration.
6. Shadow mode перед разрешением high-risk auto-print.
