from pathlib import Path
import re


def replace(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"expected block not found in {path}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")


# Historical laboratory placeholders are true aliases of the canonical labs block.
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

# A medical template that physically uses labs gets the predecessor's explicit
# "no analyses" decision in the same canonical popup, never a second dialog.
p = Path("crates/dokkomplekt-core/src/popup_profiles.rs")
text = p.read_text(encoding="utf-8")
anchor = "    let mut fields = merged.into_values().collect::<Vec<_>>();"
if anchor not in text:
    raise SystemExit("popup merged-fields anchor missing")
text = text.replace(
    anchor,
    '''    let document_uses_labs = document
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
''' + anchor,
    1,
)
anchor = '    if id.contains("diagnosis") || id.contains("icd10") || id.ends_with(".icd") {'
if anchor not in text:
    raise SystemExit("popup input-kind anchor missing")
text = text.replace(
    anchor,
    '    if id == "medical.labs_without" {\n        return PromptInputKind::YesNo;\n    }\n' + anchor,
    1,
)
old_order = '''        "medical.treatment" => 120,
        DIARY_SCHEDULE_STYLE => 121,
        DIARY_INTRADAY_RHYTHM => 122,
        DIARY_DAY_START_TIME => 123,
        DIARY_DAY_END_TIME => 124,'''
new_order = '''        "medical.treatment" => 120,
        "medical.labs" => 121,
        "medical.labs_without" => 122,
        DIARY_SCHEDULE_STYLE => 123,
        DIARY_INTRADAY_RHYTHM => 124,
        DIARY_DAY_START_TIME => 125,
        DIARY_DAY_END_TIME => 126,'''
if old_order not in text:
    raise SystemExit("popup ordering anchor missing")
text = text.replace(old_order, new_order, 1)
p.write_text(text, encoding="utf-8")

# Keep the companion decision inside the selected-document boundary and suppress
# it when real lab results are already present.
p = Path("crates/dokkomplekt-core/src/workflow_engine.rs")
text = p.read_text(encoding="utf-8")
pattern = re.compile(
    r"fn selected_document_fields\(document: &DocumentTemplateSpec\) -> BTreeSet<String> \{.*?\n\}",
    re.S,
)
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
anchor = '    let existing = case.get(&config.field_id).map(str::to_string);'
if anchor not in text:
    raise SystemExit("prompt existing anchor missing")
text = text.replace(
    anchor,
    '''    if config.field_id == "medical.labs_without" && case.get("medical.labs").is_some() {
        return None;
    }
''' + anchor,
    1,
)
p.write_text(text, encoding="utf-8")

# Explicit no-labs is semantic input: persist the decision and materialize the
# required labs block exactly as the predecessor did.
p = Path("crates/dokkomplekt-core/src/popup_engine.rs")
text = p.read_text(encoding="utf-8")
anchor = '''    let by_id = answers
        .iter()
        .map(|answer| (answer.field_id.trim(), answer))
        .collect::<BTreeMap<_, _>>();
    let mut next = case.clone();'''
if anchor not in text:
    raise SystemExit("popup answer map anchor missing")
text = text.replace(
    anchor,
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
anchor = '''    for prompt in &plan.prompts {
        let Some(answer) = by_id.get(prompt.field_id.as_str()) else {'''
if anchor not in text:
    raise SystemExit("popup prompt loop anchor missing")
text = text.replace(
    anchor,
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

# Restore the predecessor's plain-text codec chain: UTF-8 BOM / UTF-8 / CP1251.
p = Path("src-tauri/src/universal_intake.rs")
text = p.read_text(encoding="utf-8")
anchor = 'fn decode_text_bytes(bytes: &[u8]) -> String {\n    if bytes.starts_with(&[0xFF, 0xFE]) {'
if anchor not in text:
    raise SystemExit("decode_text_bytes anchor missing")
text = text.replace(
    anchor,
    '''fn decode_text_bytes(bytes: &[u8]) -> String {
    if let Some(without_bom) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(text) = std::str::from_utf8(without_bom) {
            return text.to_string();
        }
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {''',
    1,
)
old = '''    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|byte| *byte as char).collect())
}'''
new = '''    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    bytes
        .iter()
        .map(|byte| decode_rtf_ansi_byte(*byte, 1251))
        .collect()
}'''
if old not in text:
    raise SystemExit("decode fallback anchor missing")
text = text.replace(old, new, 1)
anchor = '    #[test]\n    fn supported_formats_cover_requested_universal_intake() {'
if anchor not in text:
    raise SystemExit("intake test anchor missing")
text = text.replace(
    anchor,
    '''    #[test]
    fn plain_text_decoder_preserves_utf8_bom_and_windows_1251() {
        assert_eq!(decode_text_bytes(b"\\xEF\\xBB\\xBFПривет"), "Привет");
        assert_eq!(
            decode_text_bytes(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]),
            "Привет"
        );
    }

''' + anchor,
    1,
)
p.write_text(text, encoding="utf-8")

# Public cross-module locks ensure this never becomes a UI-only compatibility shim.
Path("crates/dokkomplekt-core/tests/donor_labs_legacy_parity.rs").write_text(
    r'''use dokkomplekt_core::{
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
    assert!(plan.prompts.iter().any(|prompt| prompt.field_id == "medical.labs"));
    let without = plan
        .prompts
        .iter()
        .find(|prompt| prompt.field_id == "medical.labs_without")
        .expect("medical labs template must expose explicit no-labs choice");
    assert_eq!(without.input_kind, PromptInputKind::YesNo);
    let result = apply_popup_answers(
        &SemanticCase::default(),
        &plan,
        &[PopupAnswer {
            field_id: "medical.labs_without".into(),
            value: "Да".into(),
            continue_without_value: false,
        }],
    );
    assert!(result.accepted, "{:#?}", result.errors);
    assert_eq!(result.semantic_case.get("medical.labs"), Some("Нет анализов"));
    assert_eq!(result.semantic_case.get("medical.labs_without"), Some("Да"));
}

#[test]
fn declining_without_labs_keeps_required_labs_missing() {
    let plan = plan_workflow(&labs_document(), &SemanticCase::default(), &WorkflowFlags::default());
    let result = apply_popup_answers(
        &SemanticCase::default(),
        &plan,
        &[PopupAnswer {
            field_id: "medical.labs_without".into(),
            value: "Нет".into(),
            continue_without_value: false,
        }],
    );
    assert!(!result.accepted);
    assert!(result
        .still_missing
        .iter()
        .any(|prompt| prompt.field_id == "medical.labs"));
}

#[test]
fn existing_labs_do_not_trigger_redundant_without_labs_question() {
    let mut case = SemanticCase::default();
    dokkomplekt_core::set_user_value(&mut case, "medical.labs", "ОАК: без отклонений");
    let plan = plan_workflow(&labs_document(), &case, &WorkflowFlags::default());
    assert!(!plan
        .prompts
        .iter()
        .any(|prompt| prompt.field_id == "medical.labs_without"));
}
''',
    encoding="utf-8",
)
