from pathlib import Path
import re

ROOT = Path('.')


def read(path):
    return Path(path).read_text(encoding='utf-8')


def write(path, text):
    Path(path).write_text(text, encoding='utf-8')


def replace(path, old, new, count=1):
    text = read(path)
    actual = text.count(old)
    assert actual == count, f'{path}: expected {count} matches, got {actual}: {old[:80]!r}'
    write(path, text.replace(old, new, count))


def replace_in_function(path, fn_name, old, new):
    text = read(path)
    start = text.index(f'fn {fn_name}')
    next_fn = text.find('\n    #[test]', start + 1)
    end = len(text) if next_fn < 0 else next_fn
    chunk = text[start:end]
    assert old in chunk, f'{path}:{fn_name}: fragment not found'
    chunk = chunk.replace(old, new, 1)
    write(path, text[:start] + chunk + text[end:])


# ---------------------------------------------------------------------------
# Legacy role IDs: old saved button/profile metadata must remain meaningful.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/domains/medical.rs'
replace(p,
'        "discharge" | "discharge_epicrisis" | "выписной_эпикриз" | "выписка" | "эпикриз" => {\n',
'        "discharge" | "discharge_epicrisis" | "dischargeepicrisis" | "выписной_эпикриз" | "выписка" | "эпикриз" => {\n')
replace(p,
'        "diaries" | "diary" | "дневник" | "дневники" | "ежедневные_записи" => {\n',
'        "diaries" | "diary" | "medicaldiary" | "дневник" | "дневники" | "ежедневные_записи" => {\n')
replace(p,
'        "rvk_act" | "акт_для_рвк" | "акт_рвк" | "рвк" | "военный_комиссариат" | "военкомат" => {\n',
'        "rvk_act" | "rvkact" | "акт_для_рвк" | "акт_рвк" | "рвк" | "военный_комиссариат" | "военкомат" => {\n')
replace(p,
'        "commission" | "комиссионный_осмотр" | "комиссия" | "врачебная_комиссия" => {\n',
'        "commission" | "commissioninspection" | "jointmedicalexam" | "совместный_осмотр" | "комиссионный_осмотр" | "комиссия" | "врачебная_комиссия" => {\n')
replace(p,
'        "sick_leave_vk" | "вк_больничный" | "вк_по_больничному" | "продление_больничного" => {\n',
'        "sick_leave_vk" | "sickleavevk" | "вк_больничный" | "вк_по_больничному" | "продление_больничного" => {\n')
replace(p,
'        "vk_mse" | "вк_на_мсэ" | "мсэ" | "медико_социальная_экспертиза" => {\n',
'        "vk_mse" | "vkmse" | "вк_на_мсэ" | "мсэ" | "медико_социальная_экспертиза" => {\n')
replace(p,
'        | "reception_inspection"\n',
'        | "reception_inspection"\n        | "receptioninspection"\n')
replace(p,
'        "primary" | "первичный_осмотр" | "направление_на_госпитализацию" | "направление" => {\n',
'        "primary" | "primaryinspection" | "первичный_осмотр" | "направление_на_госпитализацию" | "направление" => {\n')
replace(p,
'        assert_eq!(canonical_medical_role("Акт для РВК"), "rvk_act");\n',
'''        assert_eq!(canonical_medical_role("Акт для РВК"), "rvk_act");
        for (legacy, canonical) in [
            ("primaryInspection", "primary"),
            ("dischargeEpicrisis", "discharge"),
            ("medicalDiary", "diaries"),
            ("rvkAct", "rvk_act"),
            ("jointMedicalExam", "commission"),
            ("commissionInspection", "commission"),
            ("sickLeaveVk", "sick_leave_vk"),
            ("vkMse", "vk_mse"),
            ("receptionInspection", "reception"),
        ] {
            assert_eq!(canonical_medical_role(legacy), canonical, "legacy role {legacy}");
        }
''')

# ---------------------------------------------------------------------------
# Popup contract: distinguish immutable template requirements from soft prompts.
# This ports the donor's strict critical-field dialog without removing an explicit
# "continue without" capability for genuinely soft workflow questions.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/types.rs'
replace(p,
'    pub required: bool,\n    pub current_value: Option<String>,\n',
'    pub required: bool,\n    #[serde(default)]\n    pub skippable: bool,\n    pub current_value: Option<String>,\n')

# Every existing literal gets the conservative backward-compatible compile value.
for path in list(ROOT.glob('crates/**/*.rs')) + list(ROOT.glob('src-tauri/**/*.rs')):
    text = path.read_text(encoding='utf-8')
    lines = text.splitlines(keepends=True)
    out = []
    in_literal = False
    changed = False
    for idx, line in enumerate(lines):
        if 'PromptSpec {' in line and 'struct PromptSpec' not in line:
            in_literal = True
        out.append(line)
        if in_literal and line.lstrip().startswith('required:'):
            next_line = lines[idx + 1].lstrip() if idx + 1 < len(lines) else ''
            if not next_line.startswith('skippable:'):
                indent = line[:len(line) - len(line.lstrip())]
                out.append(f'{indent}skippable: false,\n')
                changed = True
            in_literal = False
    if changed:
        path.write_text(''.join(out), encoding='utf-8')

p = 'crates/dokkomplekt-core/src/workflow_engine.rs'
replace(p,
'''    let optional = pipeline
        .workflow
        .optional
''',
'''    let hard_required = document
        .required_fields
        .iter()
        .filter(|field_id| is_valid_field_id(field_id))
        .map(|field_id| canonical_storage_field_id(field_id))
        .filter(|field_id| relevant.contains(field_id))
        .filter(|field_id| !suppressed.contains(field_id.as_str()))
        .collect::<BTreeSet<_>>();
    let optional = pipeline
        .workflow
        .optional
''')
replace(p,
'        .filter_map(|config| prompt_from_config(config, &required, case))\n',
'        .filter_map(|config| prompt_from_config(config, &required, &hard_required, case))\n')
replace(p,
'''fn prompt_from_config(
    config: PopupFieldConfig,
    required_fields: &BTreeSet<String>,
    case: &SemanticCase,
) -> Option<PromptSpec> {
''',
'''fn prompt_from_config(
    config: PopupFieldConfig,
    required_fields: &BTreeSet<String>,
    hard_required_fields: &BTreeSet<String>,
    case: &SemanticCase,
) -> Option<PromptSpec> {
''')
replace(p,
'''    let required = config.required || required_fields.contains(&config.field_id);
    Some(PromptSpec {
''',
'''    let required = config.required || required_fields.contains(&config.field_id);
    let skippable = required && !hard_required_fields.contains(&config.field_id);
    Some(PromptSpec {
''')
replace(p,
'''        required,
        skippable: false,
        current_value,
        validation_hint: config
            .help_text
            .clone()
            .or_else(|| Some("Заполните поле или явно разрешите продолжение без него".to_string())),
''',
'''        required,
        skippable,
        current_value,
        validation_hint: config.help_text.clone().or_else(|| {
            if required && !skippable {
                Some("Обязательное поле выбранного шаблона: заполните его перед созданием".to_string())
            } else if skippable {
                Some("Заполните поле или явно разрешите продолжение без него".to_string())
            } else {
                None
            }
        }),
''')
replace(p,
'''fn merge_prompt(existing: &mut PromptSpec, incoming: PromptSpec) {
    existing.required |= incoming.required;
''',
'''fn merge_prompt(existing: &mut PromptSpec, incoming: PromptSpec) {
    let existing_allows_skip = !existing.required || existing.skippable;
    let incoming_allows_skip = !incoming.required || incoming.skippable;
    existing.required |= incoming.required;
    existing.skippable = existing.required && existing_allows_skip && incoming_allows_skip;
''')

# Cross-profession regression: actual selected-template requirements are strict.
insert_before = '''    #[test]
    fn accounting_profile_does_not_force_fields_absent_from_selected_template() {
'''
replace(p, insert_before, '''    #[test]
    fn hard_template_requirements_are_not_skippable_in_any_profession() {
        for (domain, role, field) in [
            (DomainKind::Medical, "discharge", "medical.discharge_date"),
            (DomainKind::Legal, "contract", "contract.number"),
            (DomainKind::Hr, "order", "hr.order_number"),
            (DomainKind::Accounting, "invoice", "accounting.invoice_number"),
            (DomainKind::Education, "certificate", "document.number"),
            (DomainKind::Custom("engineering".into()), "report", "custom.report_id"),
        ] {
            let mut doc = document("strict", field);
            doc.category = domain;
            doc.role_id = role.into();
            let plan = plan_workflow(&doc, &SemanticCase::default(), &WorkflowFlags::default());
            let prompt = plan
                .prompts
                .iter()
                .find(|prompt| prompt.field_id == field)
                .unwrap_or_else(|| panic!("missing prompt for {field}"));
            assert!(prompt.required, "{field} must remain required");
            assert!(!prompt.skippable, "{field} must not be bypassable");
        }
    }

    #[test]
    fn accounting_profile_does_not_force_fields_absent_from_selected_template() {
''')

p = 'crates/dokkomplekt-core/src/popup_engine.rs'
replace(p,
'''        if value.is_empty() {
            if answer.continue_without_value {
                next.skip(&prompt.field_id);
            } else if prompt.required {
                still_missing.push(prompt.clone());
            }
            continue;
        }
''',
'''        if value.is_empty() {
            if answer.continue_without_value {
                if prompt.required && !prompt.skippable {
                    still_missing.push(prompt.clone());
                    validation_errors.push(format!(
                        "{}: поле обязательно для выбранного шаблона и не может быть пропущено",
                        prompt.title
                    ));
                } else {
                    next.skip(&prompt.field_id);
                }
            } else if prompt.required {
                still_missing.push(prompt.clone());
            }
            continue;
        }
''')
replace_in_function(p, 'continue_without_required_allows_explicit_skip',
'fn continue_without_required_allows_explicit_skip()',
'fn explicitly_skippable_required_allows_explicit_skip()')
replace_in_function(p, 'explicitly_skippable_required_allows_explicit_skip',
'            skippable: false,\n',
'            skippable: true,\n')
# Add a strict counterpart immediately before the explicit-skip test.
marker = '    #[test]\n    fn explicitly_skippable_required_allows_explicit_skip() {\n'
replace(p, marker, '''    #[test]
    fn hard_required_field_rejects_continue_without_and_preserves_original_case() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "custom.required".into(),
                title: "Критическое поле".into(),
                required: true,
                skippable: false,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.required".into(),
                value: String::new(),
                continue_without_value: true,
            }],
        );
        assert!(!result.accepted);
        assert!(!result.semantic_case.is_skipped("custom.required"));
        assert!(case.skipped_fields.is_empty());
    }

    #[test]
    fn explicitly_skippable_required_allows_explicit_skip() {
''')

# TypeScript prompt contract + client UI.
p = 'src/lib/types.ts'
replace(p,
'  required: boolean;\n  current_value?: string | null;\n',
'  required: boolean;\n  skippable?: boolean;\n  current_value?: string | null;\n')
p = 'src/components/Workspace.tsx'
replace(p,
'          {prompt.required ? (\n',
'          {prompt.required && prompt.skippable ? (\n')

# ---------------------------------------------------------------------------
# Folder naming: restore donor-compatible formats as GENERAL output choices.
# Existing defaults and formats remain untouched.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/output_naming.rs'
replace(p,
'    PeriodStartMonth,\n    PeriodEndMonth,\n',
'''    PeriodStartMonth,
    PeriodEndMonth,
    ShortPeriodStartDate,
    ShortPeriodEndDate,
    ShortPeriodRange,
    PeriodStartMonthName,
    PeriodEndMonthName,
''')
replace(p,
'''            FolderNamePart::PeriodEndMonth | FolderNamePart::DischargeMonth => push(
                month_from_date(first(case, &["period.end_date", "medical.discharge_date"]))
                    .as_deref(),
                &mut chunks,
            ),
''',
'''            FolderNamePart::PeriodEndMonth | FolderNamePart::DischargeMonth => push(
                month_from_date(first(case, &["period.end_date", "medical.discharge_date"]))
                    .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::ShortPeriodStartDate => push(
                short_date(first(case, &["period.start_date", "medical.admission_date"])).as_deref(),
                &mut chunks,
            ),
            FolderNamePart::ShortPeriodEndDate => push(
                short_date(first(case, &["period.end_date", "medical.discharge_date"])).as_deref(),
                &mut chunks,
            ),
            FolderNamePart::ShortPeriodRange => {
                if let (Some(start), Some(end)) = (
                    short_date(first(case, &["period.start_date", "medical.admission_date"])),
                    short_date(first(case, &["period.end_date", "medical.discharge_date"])),
                ) {
                    chunks.push(format!("{start}-{end}"));
                }
            }
            FolderNamePart::PeriodStartMonthName => push(
                month_name_from_date(first(case, &["period.start_date", "medical.admission_date"]))
                    .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::PeriodEndMonthName => push(
                month_name_from_date(first(case, &["period.end_date", "medical.discharge_date"]))
                    .as_deref(),
                &mut chunks,
            ),
''')
replace(p,
'fn month_from_date(value: Option<&str>) -> Option<String> {\n',
'''fn short_date(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts[2].len() < 2 {
        return Some(value.to_string());
    }
    let year = &parts[2][parts[2].len() - 2..];
    Some(format!("{}.{}.{year}", parts[0], parts[1]))
}

fn month_name_from_date(value: Option<&str>) -> Option<String> {
    let mut parts = value?.split('.');
    let _day = parts.next()?;
    let month = parts.next()?.parse::<usize>().ok()?;
    let year = parts.next()?;
    let names = [
        "январь", "февраль", "март", "апрель", "май", "июнь",
        "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь",
    ];
    let name = names.get(month.checked_sub(1)?)?;
    Some(format!("{name} {year}"))
}

fn month_from_date(value: Option<&str>) -> Option<String> {
''')
replace(p,
'''    #[test]
    fn reserved_windows_names_and_trailing_dots_are_neutralized() {
''',
'''    #[test]
    fn donor_short_date_range_is_available_without_changing_long_range() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Петров Петр Петрович"),
            ("period.start_date", "01.06.2026"),
            ("period.end_date", "12.06.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[FolderNamePart::ShortInitials, FolderNamePart::ShortPeriodRange],
            ),
            "Петров П.П. 01.06.26-12.06.26"
        );
    }

    #[test]
    fn donor_word_month_is_profession_neutral() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Сидоров Сергей Сергеевич"),
            ("period.start_date", "01.06.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[FolderNamePart::FullSubjectName, FolderNamePart::PeriodStartMonthName],
            ),
            "Сидоров Сергей Сергеевич июнь 2026"
        );
    }

    #[test]
    fn reserved_windows_names_and_trailing_dots_are_neutralized() {
''')

p = 'src/lib/types.ts'
replace(p,
"  | 'PeriodEndMonth'\n",
"""  | 'PeriodEndMonth'
  | 'ShortPeriodStartDate'
  | 'ShortPeriodEndDate'
  | 'ShortPeriodRange'
  | 'PeriodStartMonthName'
  | 'PeriodEndMonthName'
""")
p = 'src/components/UtilityPanel.tsx'
replace(p,
"  { value: 'PeriodEndMonth', label: 'месяц окончания периода' },\n",
"""  { value: 'PeriodEndMonth', label: 'месяц окончания периода' },
  { value: 'ShortPeriodRange', label: 'период целиком · короткие даты' },
  { value: 'ShortPeriodStartDate', label: 'начало периода · короткая дата' },
  { value: 'ShortPeriodEndDate', label: 'окончание периода · короткая дата' },
  { value: 'PeriodStartMonthName', label: 'месяц начала периода · словом' },
  { value: 'PeriodEndMonthName', label: 'месяц окончания периода · словом' },
""")

print('general donor parity patch applied')
