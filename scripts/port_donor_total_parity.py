from pathlib import Path
import re


def replace(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"expected block not found in {path}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")


replace(
    "crates/dokkomplekt-core/src/field_aliases.rs",
    '        "labs.results" | "labs.block" | "analysis.results" | "analyses.results" => {\n            "medical.labs".into()\n        }',
    '        "labs.results"\n        | "labs.block"\n        | "labs_block"\n        | "LAB_BLOCK"\n        | "laboratory.results"\n        | "analysis.results"\n        | "analyses.results" => "medical.labs".into(),',
)
replace(
    "crates/dokkomplekt-core/src/field_aliases.rs",
    '            "labs.results",\n            "labs.block",\n            "analysis.results",',
    '            "labs.results",\n            "labs.block",\n            "labs_block",\n            "LAB_BLOCK",\n            "laboratory.results",\n            "analysis.results",',
)
replace(
    "crates/dokkomplekt-core/src/field_registry.rs",
    '                "labs.results",\n                "labs.block",\n                "analysis.results",',
    '                "labs.results",\n                "labs.block",\n                "labs_block",\n                "LAB_BLOCK",\n                "laboratory.results",\n                "analysis.results",',
)

p = Path("crates/dokkomplekt-core/src/popup_profiles.rs")
text = p.read_text(encoding="utf-8")
needle = "    let mut fields = merged.into_values().collect::<Vec<_>>();"
if needle not in text:
    raise SystemExit("popup merged-fields anchor missing")
insert = '''    let document_uses_labs = document
        .placeholders
        .iter()
        .chain(document.required_fields.iter())
        .any(|field_id| canonical_storage_field_id(field_id) == "medical.labs");
    if matches!(document.category, DomainKind::Medical)
        && document_uses_labs
        && !merged.contains_key("medical.labs_without")
    {
        let mut config = popup_config_for_field(
            "medical.labs_without",
            false,
            &document.category,
            &document.role_id,
        );
        apply_profession_defaults(&mut config, &document.category, &document.role_id);
        config.ask_mode = PromptAskMode::Always;
        config.help_text = Some(
            "Выберите «Да», если исследований действительно нет; в документ будет записано «Нет анализов»."
                .into(),
        );
        merged.insert("medical.labs_without".into(), config);
    }
'''
text = text.replace(needle, insert + needle, 1)
needle = '    if id.contains("diagnosis") || id.contains("icd10") || id.ends_with(".icd") {'
if needle not in text:
    raise SystemExit("popup input-kind anchor missing")
text = text.replace(
    needle,
    '    if id == "medical.labs_without" {\n        return PromptInputKind::YesNo;\n    }\n' + needle,
    1,
)
text = text.replace(
    '        "medical.treatment" => 120,\n        DIARY_SCHEDULE_STYLE => 121,\n        DIARY_INTRADAY_RHYTHM => 122,\n        DIARY_DAY_START_TIME => 123,\n        DIARY_DAY_END_TIME => 124,',
    '        "medical.treatment" => 120,\n        "medical.labs" => 121,\n        "medical.labs_without" => 122,\n        DIARY_SCHEDULE_STYLE => 123,\n        DIARY_INTRADAY_RHYTHM => 124,\n        DIARY_DAY_START_TIME => 125,\n        DIARY_DAY_END_TIME => 126,',
    1,
)
p.write_text(text, encoding="utf-8")

p = Path("crates/dokkomplekt-core/src/workflow_engine.rs")
text = p.read_text(encoding="utf-8")
pattern = re.compile(r"fn selected_document_fields\(document: &DocumentTemplateSpec\) -> BTreeSet<String> \{.*?\n\}", re.S)
replacement = '''fn selected_document_fields(document: &DocumentTemplateSpec) -> BTreeSet<String> {
    let runtime_controls = profession_runtime_control_fields(&document.category, &document.role_id);
    let explicit_popup_fields = document
        .popup_configured
        .then_some(document.popup_fields.iter().map(|field| &field.field_id))
        .into_iter()
        .flatten();
    let mut fields = document
        .placeholders
        .iter()
        .chain(document.required_fields.iter())
        .chain(explicit_popup_fields)
        .chain(runtime_controls.iter())
        .filter(|field_id| is_valid_field_id(field_id))
        .map(|field_id| canonical_storage_field_id(field_id))
        .collect::<BTreeSet<_>>();
    if matches!(document.category, DomainKind::Medical) && fields.contains("medical.labs") {
        fields.insert("medical.labs_without".into());
    }
    fields
}'''
text, count = pattern.subn(replacement, text, count=1)
if count != 1:
    raise SystemExit("selected_document_fields replacement failed")
needle = '    let existing = case.get(&config.field_id).map(str::to_string);\n    let required = config.required || required_fields.contains(&config.field_id);'
if needle not in text:
    raise SystemExit("prompt existing anchor missing")
text = text.replace(
    needle,
    '    if config.field_id == "medical.labs_without" && case.get("medical.labs").is_some() {\n        return None;\n    }\n' + needle,
    1,
)
p.write_text(text, encoding="utf-8")

p = Path("crates/dokkomplekt-core/src/popup_engine.rs")
text = p.read_text(encoding="utf-8")
needle = '''    let by_id = answers
        .iter()
        .map(|answer| (answer.field_id.trim(), answer))
        .collect::<BTreeMap<_, _>>();
    let mut next = case.clone();'''
if needle not in text:
    raise SystemExit("popup answer map anchor missing")
text = text.replace(
    needle,
    '''    let by_id = answers
        .iter()
        .map(|answer| (answer.field_id.trim(), answer))
        .collect::<BTreeMap<_, _>>();
    let explicit_without_labs = by_id
        .get("medical.labs_without")
        .is_some_and(|answer| matches!(answer.value.trim().to_lowercase().as_str(), "да" | "yes" | "true"));
    let mut next = case.clone();''',
    1,
)
needle = '''    for prompt in &plan.prompts {
        let Some(answer) = by_id.get(prompt.field_id.as_str()) else {'''
if needle not in text:
    raise SystemExit("popup prompt loop anchor missing")
text = text.replace(
    needle,
    '''    for prompt in &plan.prompts {
        if prompt.field_id == "medical.labs" && explicit_without_labs {
            next.unskip("medical.labs");
            set_user_value(&mut next, "medical.labs", "Нет анализов");
            continue;
        }
        let Some(answer) = by_id.get(prompt.field_id.as_str()) else {''',
    1,
)
p.write_text(text, encoding="utf-8")

p = Path("src-tauri/src/universal_intake.rs")
text = p.read_text(encoding="utf-8")
needle = 'fn decode_text_bytes(bytes: &[u8]) -> String {\n    if bytes.starts_with(&[0xFF, 0xFE]) {'
if needle not in text:
    raise SystemExit("decode_text_bytes anchor missing")
text = text.replace(
    needle,
    'fn decode_text_bytes(bytes: &[u8]) -> String {\n    if let Some(without_bom) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {\n        if let Ok(text) = std::str::from_utf8(without_bom) {\n            return text.to_string();\n        }\n    }\n    if bytes.starts_with(&[0xFF, 0xFE]) {',
    1,
)
replace_old = '''    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|byte| *byte as char).collect())
}'''
replace_new = '''    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    bytes
        .iter()
        .map(|byte| decode_rtf_ansi_byte(*byte, 1251))
        .collect()
}'''
if replace_old not in text:
    raise SystemExit("decode fallback anchor missing")
text = text.replace(replace_old, replace_new, 1)
needle = '    #[test]\n    fn supported_formats_cover_requested_universal_intake() {'
if needle not in text:
    raise SystemExit("intake tests anchor missing")
text = text.replace(
    needle,
    '''    #[test]
    fn plain_text_decoder_preserves_utf8_bom_and_windows_1251() {
        assert_eq!(decode_text_bytes(b"\\xEF\\xBB\\xBFПривет"), "Привет");
        assert_eq!(
            decode_text_bytes(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]),
            "Привет"
        );
    }

''' + needle,
    1,
)
p.write_text(text, encoding="utf-8")

Path("crates/dokkomplekt-core/tests/donor_labs_legacy_parity.rs").write_text(r'''use dokkomplekt_core::{
    apply_popup_answers, canonical_storage_field_id, plan_workflow, storage_equivalent_field_ids,
    DocumentTemplateSpec, DomainKind, PopupAnswer, PromptInputKind, SemanticCase, WorkflowFlags,
};

fn labs_document() -> DocumentTemplateSpec {
    DocumentTemplateSpec {
        id: "legacy-labs".into(),
        button_label: "Старый шаблон с анализами".into(),
        template_path: "legacy-labs.docx".into(),
        category: DomainKind::Medical,
        role_id: "medical_generic".into(),
        required_fields: vec!["medical.labs".into()],
        placeholders: vec!["medical.labs".into()],
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    }
}

#[test]
fn predecessor_lab_placeholders_resolve_to_one_canonical_field() {
    for alias in ["laboratory.results", "LAB_BLOCK", "labs_block"] {
        assert_eq!(canonical_storage_field_id(alias), "medical.labs");
        assert!(storage_equivalent_field_ids("medical.labs").contains(&alias));
    }
}

#[test]
fn explicit_without_labs_satisfies_required_labs_in_one_popup() {
    let plan = plan_workflow(&labs_document(), &SemanticCase::default(), &WorkflowFlags::default());
    assert!(plan.prompts.iter().any(|p| p.field_id == "medical.labs"));
    let without = plan.prompts.iter().find(|p| p.field_id == "medical.labs_without").expect("no-labs choice");
    assert_eq!(without.input_kind, PromptInputKind::YesNo);
    let result = apply_popup_answers(&SemanticCase::default(), &plan, &[PopupAnswer {
        field_id: "medical.labs_without".into(),
        value: "Да".into(),
        continue_without_value: false,
    }]);
    assert!(result.accepted, "{:#?}", result.errors);
    assert_eq!(result.semantic_case.get("medical.labs"), Some("Нет анализов"));
    assert_eq!(result.semantic_case.get("medical.labs_without"), Some("Да"));
}

#[test]
fn declining_without_labs_keeps_required_labs_missing() {
    let plan = plan_workflow(&labs_document(), &SemanticCase::default(), &WorkflowFlags::default());
    let result = apply_popup_answers(&SemanticCase::default(), &plan, &[PopupAnswer {
        field_id: "medical.labs_without".into(),
        value: "Нет".into(),
        continue_without_value: false,
    }]);
    assert!(!result.accepted);
    assert!(result.still_missing.iter().any(|p| p.field_id == "medical.labs"));
}

#[test]
fn existing_labs_do_not_trigger_redundant_without_labs_question() {
    let mut case = SemanticCase::default();
    dokkomplekt_core::set_user_value(&mut case, "medical.labs", "ОАК: без отклонений");
    let plan = plan_workflow(&labs_document(), &case, &WorkflowFlags::default());
    assert!(!plan.prompts.iter().any(|p| p.field_id == "medical.labs_without"));
}
''', encoding="utf-8")
