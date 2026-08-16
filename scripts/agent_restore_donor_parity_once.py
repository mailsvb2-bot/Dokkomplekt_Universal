from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, found {count}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


# 1) Diary runtime controls must survive specialist-customized popup layouts.
replace_once(
    "crates/dokkomplekt-core/src/popup_profiles.rs",
    '''    // Fail closed: even a custom popup cannot hide a field that the selected template
    // or workflow has declared strictly required.
    for field_id in &document.required_fields {''',
    '''    // Runtime controls are part of the profession workflow itself, not merely a
    // template-designer convenience. A previously customized popup must therefore
    // never be allowed to hide the doctor's diary schedule/rhythm confirmation.
    for field_id in profession_runtime_control_fields(&document.category, &document.role_id) {
        if merged.contains_key(&field_id) {
            continue;
        }
        let required = matches!(
            field_id.as_str(),
            DIARY_SCHEDULE_STYLE | DIARY_INTRADAY_RHYTHM
        );
        let mut config = popup_config_for_field(
            &field_id,
            required,
            &document.category,
            &document.role_id,
        );
        apply_profession_defaults(&mut config, &document.category, &document.role_id);
        merged.insert(field_id, config);
    }

    // Fail closed: even a custom popup cannot hide a field that the selected template
    // or workflow has declared strictly required.
    for field_id in &document.required_fields {''',
)

replace_once(
    "crates/dokkomplekt-core/src/popup_profiles.rs",
    '''                config.allow_custom_option = true;
                config.default_value = Some("Каждый день".into());
                config.help_text = Some(
                    "Можно ввести свои дни, например: 1, 4, 9. График задаёт специалист, а не количество строк в шаблоне".into(),
                );''',
    '''                config.allow_custom_option = true;
                // Donor contract: the specialist confirms the diary style for every
                // diary run. Never silently turn an absent answer into daily diaries.
                config.ask_mode = PromptAskMode::Always;
                config.required = true;
                config.default_value = None;
                config.help_text = Some(
                    "Выберите стиль как в рабочем Dokkomplekt: каждый день; 1, 2, 3, 7, затем 2 раза в неделю; каждый день по времени; либо введите свои дни, например 1, 4, 9.".into(),
                );''',
)

replace_once(
    "crates/dokkomplekt-core/src/popup_profiles.rs",
    '''                config.allow_custom_option = true;
                config.default_value = Some("Один раз в день".into());
                config.help_text = Some(
                    "Можно ввести свой интервал (например, 90 минут) или список времени 08:00, 20:00".into(),
                );''',
    '''                config.allow_custom_option = true;
                // The second donor popup is also a specialist confirmation, even when
                // the answer is "Один раз в день".
                config.ask_mode = PromptAskMode::Always;
                config.required = true;
                config.default_value = None;
                config.help_text = Some(
                    "Подтвердите ритм: один раз в день, каждые 4 часа, каждый час, 30/15/5 минут либо свой интервал/список времени.".into(),
                );''',
)

replace_once(
    "crates/dokkomplekt-core/src/popup_profiles.rs",
    '''            if matches!(plan.role, MedicalDocumentRole::Diary) {
                add(DIARY_SCHEDULE_STYLE, false);
                add(DIARY_INTRADAY_RHYTHM, false);
                add(DIARY_DAY_START_TIME, false);
                add(DIARY_DAY_END_TIME, false);
            }''',
    '''            if matches!(plan.role, MedicalDocumentRole::Diary) {
                // Same fail-closed contract as the donor wizard: style and rhythm
                // must be confirmed by the doctor before diaries can be generated.
                add(DIARY_SCHEDULE_STYLE, true);
                add(DIARY_INTRADAY_RHYTHM, true);
                add(DIARY_DAY_START_TIME, false);
                add(DIARY_DAY_END_TIME, false);
            }''',
)

# 2) No mysterious repository-relative output folder on a fresh installation.
replace_once(
    "src/lib/appSupport.ts",
    '''  return 'output/Готовые документы';''',
    '''  // First run must ask for a real user-visible destination. A relative
  // application working-directory path is impossible for an end user to locate.
  return '';''',
)

# 3) First-run folder dialog must configure *where* files go, not only how a
#    subfolder is named.
replace_once(
    "src/components/FolderNamingOnboarding.tsx",
    '''export function FolderNamingOnboarding(props: {
  currentParts: FolderNamePartDto[];
  onConfirm(parts: FolderNamePartDto[]): void;
}) {''',
    '''export function FolderNamingOnboarding(props: {
  currentRoot: string;
  currentParts: FolderNamePartDto[];
  onPickRoot(): void;
  onConfirm(parts: FolderNamePartDto[]): void;
}) {''',
)

replace_once(
    "src/components/FolderNamingOnboarding.tsx",
    '''  const preview = useMemo(() => {
    const byId = new Map(PARTS.map(part => [part.value, part.example]));
    const chunks = selected.map(part => byId.get(part)).filter((value): value is string => Boolean(value));
    return chunks.join(' ') || 'Созданные документы';
  }, [selected]);''',
    '''  const preview = useMemo(() => {
    const byId = new Map(PARTS.map(part => [part.value, part.example]));
    const chunks = selected.map(part => byId.get(part)).filter((value): value is string => Boolean(value));
    return chunks.join(' ') || 'Созданные документы';
  }, [selected]);
  const root = props.currentRoot.trim();''',
)

replace_once(
    "src/components/FolderNamingOnboarding.tsx",
    '''        <p className="hint">Выберите правило один раз. Оно относится к любому профессиональному профилю и будет сохранено. Его всегда можно изменить в настройках.</p>

        <div className="folderNamingPresets" role="group" aria-label="Готовые правила имени папки">''',
    '''        <p className="hint">Сначала выберите реальную папку на компьютере, куда программа будет складывать готовые документы. Затем задайте правило имени подпапки. Оба значения сохраняются.</p>

        <div className="folderNamingPreview" data-testid="output-root-choice">
          <span>Куда сохранять готовые документы</span>
          <strong title={root}>{root || 'Папка ещё не выбрана'}</strong>
          <button type="button" className="softBtn" onClick={props.onPickRoot}>Выбрать папку на компьютере</button>
          <small>После создания программа отдельно покажет точный путь и список созданных файлов.</small>
        </div>

        <div className="folderNamingPresets" role="group" aria-label="Готовые правила имени папки">''',
)

replace_once(
    "src/components/FolderNamingOnboarding.tsx",
    '''          <small>Нужно выбрать хотя бы один элемент.</small>
          <span className="spacer" />
          <button type="button" className="primaryBtn" disabled={!selected.length} onClick={() => props.onConfirm(selected)}>Сохранить правило</button>''',
    '''          <small>{root ? 'Папка и правило будут сохранены.' : 'Сначала выберите папку на компьютере.'}</small>
          <span className="spacer" />
          <button type="button" className="primaryBtn" disabled={!root || !selected.length} onClick={() => props.onConfirm(selected)}>Сохранить папку и правило</button>''',
)

# Wire the existing native folder picker into onboarding.
replace_once(
    "src/App.tsx",
    '''        <FolderNamingOnboarding currentParts={folderParts} onConfirm={(parts) => { updateFolderParts(parts); setStatus('Правило имени папки комплекта сохранено.'); }} />''',
    '''        <FolderNamingOnboarding
          currentRoot={outputRoot}
          currentParts={folderParts}
          onPickRoot={() => void chooseFolder(outputRoot, setOutputRoot, 'Папка готовых документов')}
          onConfirm={(parts) => {
            updateFolderParts(parts);
            setStatus(`Папка готовых документов сохранена: ${outputRoot}. Правило подпапки тоже сохранено.`);
          }}
        />''',
)

# 4) Successful generation must be unmistakable and self-locating.
replace_once(
    "src/components/Workspace.tsx",
    '''            <h2>Создано документов: {props.lastOutput.files.length}</h2>
            <p title={props.lastOutput.folder || props.lastOutput.files[0]}>{props.lastOutput.folder || props.lastOutput.files[0]}</p>''',
    '''            <h2>Документы созданы: {props.lastOutput.files.length}</h2>
            <p className="resultFolder" title={props.lastOutput.folder || props.lastOutput.files[0]}>
              <strong>Папка:</strong> {props.lastOutput.folder || props.lastOutput.files[0]}
            </p>
            <details className="resultFiles" open>
              <summary>Созданные файлы</summary>
              <ul>
                {props.lastOutput.files.map((path) => (
                  <li key={path} title={path}>{path.split(/[\\\\/]/).filter(Boolean).pop() || path}</li>
                ))}
              </ul>
            </details>''',
)

replace_once(
    "src/components/Workspace.tsx",
    '''<i className="ti ti-folder-open" aria-hidden="true" /> Открыть комплект''',
    '''<i className="ti ti-folder-open" aria-hidden="true" /> Открыть папку с документами''',
)

# 5) Regression tests lock both the runtime diary contract and user-visible output contract.
Path("crates/dokkomplekt-core/tests/donor_diary_popup_parity.rs").write_text(r'''use dokkomplekt_core::{
    plan_workflow, DocumentTemplateSpec, DomainKind, PromptAskMode, SemanticCase, WorkflowFlags,
    DIARY_INTRADAY_RHYTHM, DIARY_SCHEDULE_STYLE,
};

#[test]
fn customized_diary_popup_cannot_hide_donor_schedule_confirmation() {
    let document = DocumentTemplateSpec {
        id: "medical-diaries".into(),
        button_label: "Дневники наблюдения".into(),
        template_path: "diaries.docx".into(),
        category: DomainKind::Medical,
        role_id: "diaries".into(),
        required_fields: Vec::new(),
        placeholders: Vec::new(),
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: true,
    };

    let plan = plan_workflow(&document, &SemanticCase::default(), &WorkflowFlags::default());
    for field_id in [DIARY_SCHEDULE_STYLE, DIARY_INTRADAY_RHYTHM] {
        let prompt = plan
            .prompts
            .iter()
            .find(|prompt| prompt.field_id == field_id)
            .unwrap_or_else(|| panic!("missing donor diary runtime prompt: {field_id}"));
        assert!(prompt.required, "{field_id} must be confirmed before generation");
        assert_eq!(prompt.ask_mode, PromptAskMode::Always);
        assert!(prompt.current_value.is_none(), "{field_id} must not silently default");
    }

    let style = plan
        .prompts
        .iter()
        .find(|prompt| prompt.field_id == DIARY_SCHEDULE_STYLE)
        .unwrap();
    assert!(style
        .options
        .iter()
        .any(|option| option.contains("1, 2, 3, 7") && option.contains("2 раза в неделю")));
    assert!(style.allow_custom_option);
}
''', encoding="utf-8")

Path("tests/test_donor_diary_output_parity.py").write_text(r'''from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def test_first_run_requires_visible_output_folder_and_generation_lists_files():
    support = read("src/lib/appSupport.ts")
    onboarding = read("src/components/FolderNamingOnboarding.tsx")
    app = read("src/App.tsx")
    workspace = read("src/components/Workspace.tsx")

    assert "return '';" in support
    assert "Куда сохранять готовые документы" in onboarding
    assert "Выбрать папку на компьютере" in onboarding
    assert "disabled={!root || !selected.length}" in onboarding
    assert "currentRoot={outputRoot}" in app
    assert "onPickRoot={() => void chooseFolder" in app
    assert "Документы созданы:" in workspace
    assert "<strong>Папка:</strong>" in workspace
    assert "Созданные файлы" in workspace
    assert "Открыть папку с документами" in workspace


def test_diary_popup_is_fail_closed_and_donor_style_is_present():
    popup = read("crates/dokkomplekt-core/src/popup_profiles.rs")
    assert '"1, 2, 3, 7, затем 2 раза в неделю"' in popup
    assert "config.ask_mode = PromptAskMode::Always;" in popup
    assert "config.default_value = None;" in popup
    assert "add(DIARY_SCHEDULE_STYLE, true);" in popup
    assert "add(DIARY_INTRADAY_RHYTHM, true);" in popup
''', encoding="utf-8")

print("donor diary/output parity patch applied")
