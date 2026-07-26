# Технический спец по всем фазам

**Проект:** Dokkomplekt Universal 18.2.2
**Основа:** `ROADMAP_SEMANTIC_MAGIC.md`
**Принцип (неизменный):** модель предлагает → Rust проверяет → печатается только доказанное.

> **Важная поправка после чтения исходников.** Слой 2 у вас готов не на 70%, а ближе к 80%. `build_extraction_prompt` уже schema-constrained, grounding уже привязывает цитату к источнику, consensus уже считает `confidence = min(model_conf, count/passes)` и блокирует high-risk без двух совпадений. Поэтому реальные задачи мельче и точечнее, чем в roadmap. Настоящий пробел один и тот же во всех фазах: **shadow-mode меряет согласие модели с детерминированным парсером, но не с истиной (финалом специалиста).** Пока это не исправлено — корпуса нет, калибровки нет, доказать «правильно» нельзя.

Ниже по каждой фазе: *что уже есть → чего не хватает → конкретные изменения по файлам → структуры данных → приёмочный тест → оценка.*

---

## ФАЗА 0 — Runtime + OCR (разблокировка)

### Что уже есть
- Hash-verified staging и Tauri resource wiring для сайдкаров.
- Скрипты: `scripts/prepare_sidecars.py`, `scripts/assert_offline_runtime_ready.py`, `scripts/verify_offline_runtime_bundle.py`, `scripts/create_offline_runtime_bundle.py`.
- Транспорт к модели: `src-tauri/src/semantic_model.rs` (`complete_many`, `consensus_sampling_profile`, loopback-only, health-check).
- Нормализация входа: `universal_intake::normalize_path(&source, &workspace, 0)`.

### Чего не хватает
- Самих проверенных бинарников (Tesseract/Poppler/LibreOffice) и GGUF-весов в staging.
- Гарантии, что OCR-выход сохраняет таблицы, а не только плоский текст.

### Изменения по файлам
1. **`scripts/prepare_sidecars.py`** — прописать конкретные версии+SHA-256 бинарников; прогнать `assert_offline_runtime_ready.py` до зелёного.
2. **`src-tauri/src/universal_intake.rs`** → `normalize_path`: для image/scanned PDF ветку добавить вызов OCR-сайдкара и вернуть не только `text`, но и распознанные табличные блоки в тот же формат, что уже потребляет `parse_source_text`. Ключ: таблицы из OCR должны попадать в `items[]` тем же маршрутом, что и Word-таблицы.
3. **`crates/dokkomplekt-core/src/source_classification.rs`** — добавить класс входа `scanned_image`, чтобы UI честно показывал «распознано OCR, проверьте внимательнее».

### Приёмочный тест
`tests/` (Python) + Rust: скан-PDF с таблицей на входе → в case есть текст И `items[]` с позициями. Без ручной подготовки. Добавить golden-фикстуру scanned-PDF рядом с существующими 7 DOCX-golden.

### Оценка
1–3 недели. Кода мало; основное — упаковка и проверка бинарников.

---

## ФАЗА 1 — Domain-scoped grounded extraction (усилить слой 2)

### Что уже есть (проверено в коде)
- **Schema-constrained prompt.** `crates/dokkomplekt-core/src/semantic_llm.rs::build_extraction_prompt` итерирует `schema_entries()` и подаёт модели список канонических полей с подсказками. Свободного извлечения уже нет.
- **Grounding.** `parse_model_extraction_with_source` отклоняет значение, если цитата не найдена в источнике буквально.
- **Consensus + confidence.** `apply_model_consensus_with_source`: голосование, `selected.confidence = min(model_conf, count/passes)`, high-risk требует `count >= 2`.
- **Провенанс уже в модели данных.** `SemanticValue { field_id, value, source: ValueSource, confidence: f32, evidence: Vec<ValueEvidence> }`; `ValueSource::{SafeDefault=10, Model=15, Scanner=20, SessionSelection=30, UserConfirmed=40}`.

### Чего не хватает (реальные точечные пробелы)
- **Схема глобальная, а не доменная.** `schema_entries()` (в `semantic_engine.rs:1236`) — один статический список на все домены. Модель получает поля чужих доменов → путает `contract.number` с `invoice.number`.
- **`is_high_risk_model_field` работает по подстроке.** `field_id.contains("number")` ловит и `phone_number`. Ложные high-risk и пропуски.
- **Confidence не откалиброван.** `min(model_conf, agreement)` — правильная форма, но число ещё не соответствует реальной доле ошибок (это Фаза 4).

### Изменения по файлам
1. **`crates/dokkomplekt-core/src/semantic_engine.rs`** → сделать `schema_entries()` параметризуемым доменом:
   ```rust
   pub(crate) fn schema_entries_for(domain: DomainKind) -> Vec<(&'static str, FieldType, &'static str)>
   ```
   Возвращать объединение universal-ядра + слотов активного домена/пака. Старую `schema_entries()` оставить как `schema_entries_for(DomainKind::Custom)` для обратной совместимости.
2. **`crates/dokkomplekt-core/src/semantic_llm.rs`** → `build_extraction_prompt(text: &str, domain: DomainKind)`; подставлять доменный список. Обновить единственный вызов в `automation_runtime.rs`.
3. **`crates/dokkomplekt-core/src/semantic_llm.rs`** → заменить `is_high_risk_model_field` со «substring» на явный набор канонических id + суффиксов:
   ```rust
   const HIGH_RISK_EXACT: &[&str] = &["subject.name","subject.birth_date","org.inn","org.kpp","org.ogrn","subject.snils", ...];
   fn is_high_risk_model_field(id: &str) -> bool {
       HIGH_RISK_EXACT.contains(&id)
       || id.ends_with(".date") || id.ends_with("_date")
       || id.ends_with(".amount") || id.ends_with(".number")
   }
   ```
   (суффиксы, а не `contains`, чтобы `phone_number` не считался high-risk).
4. **Провенанс уже логируется** — доработка не нужна, но убедиться, что `evidence.page_index` заполняется из OCR-слоя Фазы 0.

### Приёмочный тест
- Unit в `semantic_llm.rs`: для домена HR модели не предлагается медицинский `diagnosis`; `phone_number` больше не high-risk; `contract.number` и `invoice.number` не путаются на смешанном тексте.
- Существующие тесты `one_model_pass_cannot_approve_a_high_risk_field` и `high_risk_consensus_requires_two_equal_grounded_answers` должны остаться зелёными.

### Оценка
2–3 недели. Изменения локальны в 2 файлах ядра + 1 вызов.

---

## ФАЗА 2 — Корпус из реальной работы (снять узкое место)

### Что уже есть (проверено в коде)
Shadow-mode в `automation_runtime.rs` (~строки 318–400) уже:
- гоняет `apply_model_consensus_with_source` на клоне case, ничего не пишет в комплект;
- считает `shadow_model_proposals`, `shadow_model_agreements`, `model_grounding_rejections`;
- пишет audit-событие `semantic_model_shadow_evaluated`.

### Чего не хватает (это и есть узкое место)
**Сейчас `agreements` = совпадение модели с детерминированным парсером, а НЕ с истиной.** Детерминированный парсер сам ошибается — значит метрика меряет «два потенциально неверных согласились». Ground truth — это **финальный case после правок специалиста**, которого shadow-логгер не видит, потому что он срабатывает в начале интейка, до подтверждений.

### Изменения по файлам
1. **Новый модуль `crates/dokkomplekt-core/src/corpus_recorder.rs`.** Одна запись = одно завершённое дело:
   ```rust
   #[derive(Serialize, Deserialize)]
   pub struct CorpusEntry {
       pub source_sha256: String,
       pub domain: DomainKind,
       pub pack_id: Option<String>,
       pub input_text_sha256: String,         // не сам текст — приватность
       pub model_proposals: Vec<FieldObservation>,   // что предложила модель (value, confidence, evidence-hash)
       pub deterministic: Vec<FieldObservation>,      // что дал детерминированный парсер
       pub final_accepted: Vec<FieldObservation>,     // ИСТИНА: финал после правок (source=UserConfirmed/Scanner)
       pub kit_documents: Vec<String>,                // фактический состав комплекта
       pub created_at: String,
   }
   ```
   Хранить значения хешами/усечёнными (152-ФЗ): для метрик нужно «совпало/не совпало», а не сами ФИО.
2. **`automation_runtime.rs`** — перенести фиксацию истины в конец интейка, после блока подтверждений (`req.confirmed_fields`) и после генерации, когда известны и финальный `case`, и `created_documents`. Записать `CorpusEntry`, соединив ранее сохранённые `model_proposals`/`deterministic` (положить их в `case_run` на этапе `recognizing`) с финалом.
3. **Хранилище** — переиспользовать `crates/dokkomplekt-storage` (зашифрованный SQLite + HMAC audit-chain уже есть). Новая таблица `corpus_entries`. Флаг включения — рядом с `shadow_mode` в конфиге, по умолчанию **off** и только для явно согласившегося пилота.
4. **Экспорт** — `scripts/export_corpus.py`: выгружает обезличенный корпус в формат для офлайн-анализа точности (совместимо с `content-packs/manifest.schema.json` по слотам).

### Пилотный протокол (организационная часть, не код)
- Один специалист, один домен, обычная работа + shadow-mode + corpus-recorder.
- Каждая правка поля и каждое ручное добавление документа в комплект = золотая метка бесплатно.
- Цель: **≥50 завершённых дел** → первый измеримый корпус.

### Приёмочный тест
- Rust: после интейка с правками `CorpusEntry.final_accepted` содержит значения с `source ∈ {UserConfirmed, Scanner}`, а `model_proposals` — то, что было `ValueSource::Model` до правок.
- Python: обезличенность — в экспортируемом корпусе нет сырых ФИО/номеров, только хеши + флаги совпадения.

### Оценка
Разработка — 2–3 недели. Календарно ограничено темпом пилота (2–6 недель до 50 дел).

---

## ФАЗА 3 — Выбор комплекта (доменное ядро, слой 3)

### Что уже есть
- Доменные профили: `crates/dokkomplekt-core/src/domains/*.rs` (hr, legal, accounting, education, medical, custom).
- Learned scanner rules на уровне полей: `apply_learned_scanner_rules(app, &source_text, &mut case)`.
- Content-pack схема с `template_slots` и `workflows`: `content-packs/manifest.schema.json`.

### Чего не хватает
- Правил «тип источника → обязательный состав комплекта» (сейчас есть каркасы, но не kit-selection).
- Обучения комплектам (learned rules только для полей, не для наборов документов).

### Изменения по файлам
1. **Детерминированный костяк** — расширить каждый доменный профиль:
   ```rust
   // domains/hr.rs
   pub fn required_kit(source_class: SourceClass) -> Vec<TemplateSlot> { ... }
   ```
   Пример HR: источник «заявление о приёме» → `[employment_contract, employment_order, personal_data_consent, familiarization_sheet]`. Начать строго с домена Фазы 2.
2. **Обучение на истории** — новый `crates/dokkomplekt-core/src/kit_learning.rs`: на `CorpusEntry` кластеризует `source_class → kit_documents`, выдаёт **предложение** набора. Никогда не применять молча — всегда «предлагаю комплект: …, подтвердите».
3. **Промоушен правил по метрикам** — правило `source_class → kit` переходит из «предлагать» в «применять» только после **N подтверждений подряд с нулём исправлений состава** на корпусе. Хранить счётчик рядом с learned rules. Это тот же паттерн промоушена, что у вас уже есть в scanner-правилах — переиспользовать.
4. **UI** — `src/components/AutomationControlCenter.tsx`: показать предложенный состав с источником правила (curated/learned) и кнопкой подтверждения.

### Приёмочный тест
- Rust: для пилотного домена на held-out части корпуса предложенный состав = фактический в **≥90%** дел.
- Правило не промоутится в auto, пока не набрало N чистых подтверждений (тест на счётчик промоушена).

### Оценка
4–8 недель на первый домен. Каждый следующий дешевле (механизм переиспользуется, меняются только curated-правила).

---

## ФАЗА 4 — Порог уверенности + review-фолбэк (гарантия «правильно»)

### Что уже есть
- Per-field `confidence: f32` и `source: ValueSource` в каждом `SemanticValue`.
- Детерминированные валидаторы (`validators.rs`) — ИНН/СНИЛС/ОГРН/КПП/БИК/кадастр/VIN.
- Отчёт доверия `ПРОВЕРИТЬ_КОМПЛЕКТ.txt` (поле, значение, источник, уверенность).
- Запрет статуса `pilot/approved` без проверенных DOCX.

### Чего не хватает
- Трёх корзин по confidence в маршруте печати.
- **Калибровки порога по реальной доле ошибок** (Фаза 2 даёт данные, здесь считается).
- Обязательного diff-шага перед авто-печатью средней корзины.

### Изменения по файлам
1. **`crates/dokkomplekt-core/src/automation_quality.rs`** — функция триажа комплекта:
   ```rust
   pub enum PrintBucket { AutoPrint, ReviewFields(Vec<String>), HoldForReview }
   pub fn triage(case: &SemanticCase, thresholds: &CalibratedThresholds) -> PrintBucket
   ```
   Логика: минимальный confidence среди *использованных* документом high-risk полей → корзина.
2. **`CalibratedThresholds`** — не хардкод. Считаются `scripts/calibrate_thresholds.py` из корпуса Фазы 2: строим кривую «confidence vs фактическая доля ошибок» на `final_accepted` как истине, выставляем порог auto так, чтобы **error rate в auto-корзине < целевого** (напр. 0.5%). Порог хранить в подписанном виде рядом с паком (тот же Ed25519-механизм, что у каталога компонентов).
3. **`automation_runtime.rs`** → `print_files` / zero-touch путь: перед печатью вызвать `triage`. `AutoPrint` → печать; `ReviewFields` → собрать, но UI показывает «проверьте N полей» + diff; `HoldForReview` → в очередь, не печатать.
4. **UI diff** — вынести `ПРОВЕРИТЬ_КОМПЛЕКТ.txt` в обязательный экран для средней корзины (`RuntimePromptModal.tsx`).
5. **Жёсткая привязка к формам** — `draft_only`-пак **запрещён** для `AutoPrint` на уровне контракта (расширить существующий запрет статуса).

### Приёмочный тест
- Rust: комплект со средним confidence не попадает в `AutoPrint`; `draft_only` никогда не в `AutoPrint`.
- Python: на held-out корпусе измеренный error rate в auto-корзине ≤ целевого порога — иначе гейт падает fail-closed.

### Оценка
2–4 недели поверх Фаз 1–2.

---

## Метрики (единственное «доказано»)

Считаются `scripts/measure_domain.py` на held-out части корпуса Фазы 2, по домену:

| Метрика | Определение | Порог для повышения автономии |
|---------|-------------|-------------------------------|
| Field accuracy | доля полей, где `final_accepted` = предложенное системой | напр. ≥ 98% на high-risk |
| Kit completeness | доля дел, где предложенный состав = фактический | ≥ 90% (Фаза 3) |
| **Auto-bucket error rate** | доля ошибок среди напечатанного без ревью | **< 0.5%, главный KPI** |

Автономию домена повышаем, только когда все три под порогом. Не по интуиции.

---

## Последовательность и зависимости

```
Ф0 (runtime+OCR) ──┬─> Ф1 (domain-scoped extraction)
                   │        │
                   │        v
                   └─> Ф2 (corpus recorder, shadow) ──┬─> Ф3 (kit selection)
                                                       └─> Ф4 (threshold + fallback)
```

- Ф0 разблокирует всё (без OCR «любой документ» неверно).
- Ф1 и Ф2 параллельны.
- Ф3 и Ф4 обе питаются корпусом Ф2 — до него не начинать всерьёз.
- Первый полный проход = один домен end-to-end. Второй домен переиспользует Ф0/Ф1/Ф4 почти целиком.

**Суммарно до первого автономного домена:** ≈ 3–5 месяцев, из которых значительная часть — темп пилота, а не разработка.

---

## Границы (защищают вас юридически)

- Не обещать «любой документ, ноль настройки, ноль ошибок» — этого нет ни у кого. Обещать: домен за доменом высокая автономия + быстрый безопасный ревью везде остальное.
- Никогда не давать модели писать в документ без grounding + валидатора. Как только LLM решает юридическую правильность сама — «магия» превращается в ответственность.
- Не ослаблять grounding ради покрытия. Корзина «в ревью» лучше тихой ошибки.
- Ground truth = финал специалиста, а не согласие двух парсеров. Всё в Фазе 2 держится на этом.
- «Доказано» = измеримая метрика + Ed25519-аттестация, а не текстовый marker.

---

## Первый конкретный шаг (если начинать завтра)

1. Ф2, п.1–2: `corpus_recorder.rs` + перенос фиксации истины в конец интейка. Это дёшево, ничего не ломает (только пишет), и без него всё остальное — угадайка.
2. Параллельно Ф0: довести один OCR-сайдкар до зелёного `assert_offline_runtime_ready.py`.
3. Запустить одного пилотного специалиста в HR с включённым shadow + corpus. Пока он копит 50 дел — делать Ф1 (domain-scoped schema) и Ф3 curated-костяк для HR.
