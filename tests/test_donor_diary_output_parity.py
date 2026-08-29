from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def test_first_run_requires_visible_output_folder_and_generation_lists_files():
    support = read("src/lib/appSupport.ts")
    onboarding = read("src/components/FolderNamingOnboarding.tsx")
    app = read("src/App.tsx")
    output_flow = read("src/lib/outputFlow.ts")
    workspace = read("src/components/Workspace.tsx")

    assert "return '';" in support
    assert "Куда сохранять готовые документы" in onboarding
    assert "Выбрать папку на компьютере" in onboarding
    assert "disabled={!root}" in onboarding
    assert "disabled={!root || !selected.length}" not in onboarding
    assert "Созданные документы" in onboarding
    assert "currentRoot={outputRoot}" in app
    assert "onPickRoot={() => void chooseAndCommitOutputFolder()" in app
    assert "(!folderNamingConfirmed || !outputRoot.trim())" in app
    assert "outputRoot.trim() || 'output/Готовые документы'" not in app
    assert "Сначала выберите папку готовых документов. Ничего не создано." in output_flow
    assert "Создано документов:" in workspace
    assert "<strong>Папка:</strong>" in workspace
    assert "Созданные файлы" in workspace
    assert "Открыть папку с документами" in workspace
    assert "openCreatedOutputFolderSilently(res.output_folder, openInFileManager)" in app
    assert "await openFolder(target)" in output_flow
    assert "return { opened: false, error: errorText(error) };" in output_flow


def test_diary_popup_is_fail_closed_and_donor_style_is_present():
    popup = read("crates/dokkomplekt-core/src/popup_profiles.rs")
    diary_popup = read("crates/dokkomplekt-core/src/diary_popup_profile.rs")
    assert '"1, 2, 3, 7, затем 2 раза в неделю"' in popup
    assert "config.ask_mode = PromptAskMode::Always;" in popup
    assert "config.default_value = None;" in popup
    assert "add(DIARY_SCHEDULE_STYLE, true);" in popup
    assert "add(DIARY_INTRADAY_RHYTHM, true);" in popup
    assert "DIARY_SICK_LEAVE_EPICRISIS" in diary_popup
    assert "DIARY_TREATMENT_CORRECTION" in diary_popup
    assert '"Лечится по больничному листу?"' in diary_popup
    assert 'config.linked_to = Some(DIARY_SICK_LEAVE_EPICRISIS.into());' in diary_popup
    assert "popup_profiles::effective_popup_fields(document)" in diary_popup


def test_normal_diary_route_uses_program_calendar_and_doctor_owned_texts():
    profile_sources = read("src-tauri/src/subsystems/profile_sources.rs")
    records = read("crates/dokkomplekt-core/src/diary_professional_records.rs")
    materials = read("src/components/AdditionalMaterialsPanel.tsx")
    preflight = read("src/components/GenerationPreflightModal.tsx")

    assert "MEDICAL_DIARY_PROGRAM_TEMPLATE_TEXT" in profile_sources
    assert "MEDICAL_DIARY_PROGRAM_TEMPLATE_VERSION" in profile_sources
    assert (
        "medical-diary-program-calendar-{MEDICAL_DIARY_PROGRAM_TEMPLATE_VERSION}.docx"
        in profile_sources
    )
    assert "program_calendar_diary_template(app).map(Some)" in profile_sources
    assert "select_diary_template_for_admission" not in profile_sources
    assert "MEDICAL_DIARY_DATE_TEMPLATES_BLOCK_ID" not in profile_sources
    assert "{{#each diaries}}" in profile_sources
    assert "{{diary.datetime}}" in profile_sources
    assert "{{else}}{{diary.text}}{{/if}}" in profile_sources
    assert "{{diary.treating_physician_signature}}" in profile_sources
    assert "{{diary.department_head_signature}}" in profile_sources
    assert "психотических расстройств" not in profile_sources.lower()
    assert "критика к состоянию" not in profile_sources.lower()
    assert "NEUTRAL_FINAL_DIARY_TEXT" in records
    assert "ensure_donor_final_diary_text(rows, case);" in records
    assert "ordinary rotating status" in records
    assert "specialist-owned final source" in records

    assert "даты берутся из даты поступления и выписки" in materials
    assert "Отдельная папка «Даты 01–31» для обычного создания не нужна" in materials
    assert "> Даты" not in materials
    assert "Тексты" in materials
    assert "сама построит календарь D0+1 → выписка" in materials

    assert "medical.diary_day_start_time" in preflight
    assert "medical.diary_day_end_time" in preflight
    assert "visiblePrompts = prompts.filter" in preflight


def test_intraday_internal_bounds_are_supplied_without_extra_doctor_questions():
    profile_sources = read("src-tauri/src/subsystems/profile_sources.rs")
    assert 'prompt.current_value = Some("00:00".into());' in profile_sources
    assert 'prompt.current_value = Some("23:59".into());' in profile_sources
    assert "The working donor applications ask the doctor for diary decisions" in profile_sources


def test_dynamic_sick_leave_epicrisis_is_donor_owned_and_not_a_frontend_second_brain():
    epicrisis = read("crates/dokkomplekt-core/src/diary_epicrisis.rs")
    records = read("crates/dokkomplekt-core/src/diary_professional_records.rs")
    preflight = read("src/components/GenerationPreflightModal.tsx")

    assert "base.checked_add_signed(Duration::days(10))" in epicrisis
    assert "donor_non_working_day" in epicrisis
    assert "MAX_DYNAMIC_EPICRISES: usize = 12" in epicrisis
    assert "Продолжение лечения по листу нетрудоспособности." in epicrisis
    assert "Заведующий отделением ____________________" in epicrisis
    assert "Лечащий врач ____________________" in epicrisis
    assert "merge_dynamic_epicrises" in records
    assert 'SemanticAtom::Text("dynamic_epicrisis".into())' in records
    assert "linked field whose source is Yes/No is a generic visibility dependency" in preflight
    assert "medical.diary_sick_leave_epicrisis" not in preflight
    assert "medical.diary_treatment_correction" not in preflight
