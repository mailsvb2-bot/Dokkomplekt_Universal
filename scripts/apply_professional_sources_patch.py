from pathlib import Path

core = Path("crates/dokkomplekt-core/src/professional_records.rs")
text = core.read_text(encoding="utf-8")
start = text.index("fn diary_text_sources(case: &SemanticCase, diagnosis: &str) -> DiaryTextSources {")
end = text.index("\nfn record_is_final", start)
new_fn = r'''fn diary_text_sources(case: &SemanticCase, diagnosis: &str) -> DiaryTextSources {
    let mut all = Vec::<&SemanticRecord>::new();
    for collection_id in MEDICAL_DIARY_TEXT_COLLECTIONS {
        if let Some(rows) = case.collection(collection_id) {
            all.extend(rows);
        }
    }

    let target = normalize_match(diagnosis);
    let matching = all
        .iter()
        .copied()
        .filter(|row| {
            atom_text(row, "diagnosis")
                .map(|value| normalize_match(&value) == target)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let selected = if !matching.is_empty() {
        matching
    } else {
        // Unscoped rows are reusable within the active medical profile. Rows
        // explicitly assigned to a different diagnosis must never leak across.
        all.into_iter()
            .filter(|row| atom_text(row, "diagnosis").is_none_or(|value| value.trim().is_empty()))
            .collect::<Vec<_>>()
    };

    let mut result = DiaryTextSources::default();
    for row in selected {
        let Some(text) = atom_text(row, "text")
            .or_else(|| atom_text(row, "body"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if record_is_final(row) {
            if result.final_text.is_none() {
                result.final_text = Some(text);
            }
        } else {
            result.regular.push(text);
        }
    }

    // Persistent profile sources reuse the existing local clause-block store.
    // This keeps storage universal: other professions may introduce their own
    // namespaced sources without a medical database or a second semantic brain.
    let key = source_key(diagnosis);
    if result.regular.is_empty() {
        let regular_id = format!("professional.medical.diary.regular.{key}");
        if let Some(content) = case.blocks.get(&regular_id) {
            result.regular = split_status_source(content);
        }
    }
    if result.final_text.is_none() {
        let final_id = format!("professional.medical.diary.final.{key}");
        result.final_text = case
            .blocks
            .get(&final_id)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    result
}

fn source_key(value: &str) -> String {
    normalize_match(value)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn split_status_source(content: &str) -> Vec<String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if paragraphs.len() > 1 {
        return paragraphs;
    }
    let lines = normalized
        .lines()
        .map(str::trim)
        .filter(|value| value.chars().count() >= 25)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > 1 {
        lines
    } else {
        normalized
            .trim()
            .is_empty()
            .then(Vec::new)
            .unwrap_or_else(|| vec![normalized.trim().to_string()])
    }
}
'''
text = text[:start] + new_fn + text[end:]

test_anchor = '''    #[test]
    fn nonmedical_case_does_not_receive_medical_diaries() {
'''
if "persistent_clause_block_sources_feed_medical_diaries" not in text:
    test = r'''    #[test]
    fn persistent_clause_block_sources_feed_medical_diaries() {
        let mut case = medical_case();
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            "Первый профессиональный статус достаточно длинный для источника.\n\nВторой профессиональный статус также хранится локально.".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.final.f200".into(),
            "Подтверждённый специалистом итоговый дневник.".into(),
        );
        let rendered = render_text_template(
            "{{#each diaries}}{{diary.date}}|{{diary.text}}\n{{/each}}",
            &case,
            true,
        );
        assert!(rendered.missing_fields.is_empty(), "{:?}", rendered.missing_fields);
        assert!(rendered.output_text.contains("Первый профессиональный статус"));
        assert!(rendered.output_text.contains("Второй профессиональный статус"));
        assert!(rendered.output_text.contains("Подтверждённый специалистом итоговый дневник"));
    }

'''
    if test_anchor not in text:
        raise SystemExit("core test anchor missing")
    text = text.replace(test_anchor, test + test_anchor, 1)
core.write_text(text, encoding="utf-8")

ui = Path("src/components/AdvancedToolsPanel.tsx")
u = ui.read_text(encoding="utf-8")
constants_anchor = "const YEAR = new Date().getFullYear();\n"
constants = r'''const YEAR = new Date().getFullYear();
const MEDICAL_DIARY_REGULAR_PREFIX = 'professional.medical.diary.regular.';
const MEDICAL_DIARY_FINAL_PREFIX = 'professional.medical.diary.final.';

function diarySourceKey(value: string): string {
  return value
    .trim()
    .toLocaleLowerCase('ru-RU')
    .replace(/ё/g, 'е')
    .replace(/[^\p{L}\p{N}]+/gu, '');
}

function diagnosisFromDiaryFileName(fileName: string): string {
  return fileName.replace(/\.[^.]+$/, '').trim();
}
'''
if "MEDICAL_DIARY_REGULAR_PREFIX" not in u:
    if constants_anchor not in u:
        raise SystemExit("ui constants anchor missing")
    u = u.replace(constants_anchor, constants, 1)

state_anchor = "  const [blockContent, setBlockContent] = useState('');\n"
state_repl = state_anchor + "  const [diaryFinalDiagnosis, setDiaryFinalDiagnosis] = useState('');\n  const [diaryFinalText, setDiaryFinalText] = useState('');\n"
if "diaryFinalDiagnosis" not in u:
    if state_anchor not in u:
        raise SystemExit("ui state anchor missing")
    u = u.replace(state_anchor, state_repl, 1)

selected_anchor = "  const versionedDocument = selectedDocuments.length === 1 ? selectedDocuments[0] : null;\n"
selected_repl = selected_anchor + r'''  const medicalSelected = selectedDocuments.some((document) => document.category === 'Medical');
  const medicalDiarySources = blocks.filter((block) =>
    block.block_id.startsWith(MEDICAL_DIARY_REGULAR_PREFIX) || block.block_id.startsWith(MEDICAL_DIARY_FINAL_PREFIX));
'''
if "const medicalSelected" not in u:
    if selected_anchor not in u:
        raise SystemExit("ui selected anchor missing")
    u = u.replace(selected_anchor, selected_repl, 1)

function_anchor = '''  async function removeBlock(id: string) {
    const result = await execute('удаление блока', () => deleteClauseBlock(id));
    if (result) setBlocks(result);
  }
'''
functions = function_anchor + r'''
  async function importMedicalDiaryTexts(files: File[]) {
    if (!files.length) return;
    const result = await execute('импорт текстов дневников', async () => {
      let current = blocks;
      let imported = 0;
      for (const file of files) {
        if (!/\.txt$/i.test(file.name)) continue;
        const diagnosis = diagnosisFromDiaryFileName(file.name);
        const key = diarySourceKey(diagnosis);
        const content = (await file.text()).trim();
        if (!key || !content) continue;
        current = await saveClauseBlock(
          `${MEDICAL_DIARY_REGULAR_PREFIX}${key}`,
          `Тексты дневников: ${diagnosis}`,
          content,
        );
        imported += 1;
      }
      return { current, imported };
    });
    if (!result) return;
    setBlocks(result.current);
    onStatus(`Импортировано источников текстов дневников: ${result.imported}. Имя TXT используется как диагноз; данные сохранены локально.`);
  }

  async function saveMedicalFinalDiary() {
    const diagnosis = diaryFinalDiagnosis.trim();
    const key = diarySourceKey(diagnosis);
    if (!key || !diaryFinalText.trim()) {
      onStatus('Для итогового дневника укажите диагноз и подтверждённый специалистом текст.');
      return;
    }
    const result = await execute('сохранение итогового дневника', () => saveClauseBlock(
      `${MEDICAL_DIARY_FINAL_PREFIX}${key}`,
      `Итоговый дневник: ${diagnosis}`,
      diaryFinalText.trim(),
    ));
    if (!result) return;
    setBlocks(result);
    onStatus(`Итоговый дневник для ${diagnosis} сохранён локально и будет использоваться только в медицинском профиле.`);
  }
'''
if "async function importMedicalDiaryTexts" not in u:
    if function_anchor not in u:
        raise SystemExit("ui function anchor missing")
    u = u.replace(function_anchor, functions, 1)

jsx_anchor = '''      <section className="utilityCard advancedCard">
        <strong>Библиотека блоков</strong>
'''
jsx = r'''      {medicalSelected && (
        <section className="utilityCard advancedCard">
          <strong>Медицина · источники дневников</strong>
          <small>Совместимость с diary-filler: выберите TXT-файлы, названные по диагнозу (например F20.0.txt). Тексты сохраняются локально; чужой диагноз не подмешивается.</small>
          <label className="utilBtn fileButton">
            Импортировать «Тексты» (.txt)
            <input
              type="file"
              accept=".txt,text/plain"
              multiple
              hidden
              onChange={(event) => { void importMedicalDiaryTexts(Array.from(event.currentTarget.files ?? [])); event.currentTarget.value = ''; }}
            />
          </label>
          <input value={diaryFinalDiagnosis} onChange={(event) => setDiaryFinalDiagnosis(event.target.value)} placeholder="диагноз для итогового дневника, например F20.0" />
          <textarea value={diaryFinalText} onChange={(event) => setDiaryFinalText(event.target.value)} placeholder="подтверждённый специалистом итоговый дневник" />
          <button disabled={busy || !diaryFinalDiagnosis.trim() || !diaryFinalText.trim()} className="utilBtn" onClick={saveMedicalFinalDiary}>Сохранить итоговый дневник</button>
          {medicalDiarySources.length > 0 && (
            <div className="advancedList">
              {medicalDiarySources.map((block) => (
                <div key={block.block_id} className="advancedListRow">
                  <span>{block.title || block.block_id}</span>
                  <button disabled={busy} className="utilBtn danger" onClick={() => void removeBlock(block.block_id)}>Удалить</button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      <section className="utilityCard advancedCard">
        <strong>Библиотека блоков</strong>
'''
if "Медицина · источники дневников" not in u:
    if jsx_anchor not in u:
        raise SystemExit("ui jsx anchor missing")
    u = u.replace(jsx_anchor, jsx, 1)
ui.write_text(u, encoding="utf-8")
