use crate::core::{SourceDocument, TargetTemplate};
use crate::{
    effective_popup_fields, is_valid_field_id, popup_config_for_field, resolve_popup_default,
    run_universal_constructor_pipeline, DocumentTemplateSpec, DomainKind, PopupFieldConfig,
    PromptAskMode, PromptSpec, SemanticCase, UniversalDomain, UniversalPipelineFlags,
    UniversalPipelineInput, WorkflowFlags, WorkflowPlan,
};
use std::collections::{BTreeMap, BTreeSet};

/// Builds one merged popup plan for a selected document.
///
/// The popup is profile-aware and user-configurable. Pipeline requirements, template
/// placeholders and specialist-authored popup fields are merged into one deterministic plan.
pub fn plan_workflow(
    document: &DocumentTemplateSpec,
    case: &SemanticCase,
    flags: &WorkflowFlags,
) -> WorkflowPlan {
    if document.is_static_copy && document.popup_fields.is_empty() {
        return WorkflowPlan {
            document_id: document.id.clone(),
            prompts: Vec::new(),
            blocked: false,
            block_reasons: Vec::new(),
        };
    }

    let pipeline = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument {
            id: "semantic_case_snapshot".into(),
            text: semantic_case_snapshot_text(case),
            metadata: Default::default(),
        },
        target_template: TargetTemplate {
            id: document.id.clone(),
            path: document.template_path.clone(),
            text: workflow_template_text(document),
        },
        domain_hint: domain_hint_from_kind(&document.category),
        flags: UniversalPipelineFlags {
            sick_leave_enabled: flags.sick_leave_enabled,
        },
    });

    let suppressed = suppressed_prompt_fields(document);
    let required = pipeline
        .workflow
        .requires
        .iter()
        .chain(document.required_fields.iter())
        .filter(|field_id| is_valid_field_id(field_id))
        .filter(|field_id| !suppressed.contains(field_id.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    let optional = pipeline
        .workflow
        .optional
        .iter()
        .chain(document.placeholders.iter())
        .filter(|field_id| is_valid_field_id(field_id))
        .filter(|field_id| !suppressed.contains(field_id.as_str()))
        .filter(|field_id| !required.contains(*field_id))
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut configs = effective_popup_fields(document)
        .into_iter()
        .filter(|config| !suppressed.contains(config.field_id.as_str()))
        .map(|config| (config.field_id.clone(), config))
        .collect::<BTreeMap<_, _>>();
    for field_id in required.iter().chain(optional.iter()) {
        configs.entry(field_id.clone()).or_insert_with(|| {
            popup_config_for_field(
                field_id,
                required.contains(field_id),
                &document.category,
                &document.role_id,
            )
        });
    }

    let mut prompts = configs
        .into_values()
        .filter_map(|config| prompt_from_config(config, &required, case))
        .collect::<Vec<_>>();
    prompts.sort_by(|a, b| a.order.cmp(&b.order).then(a.field_id.cmp(&b.field_id)));
    prompts.dedup_by(|a, b| a.field_id == b.field_id);

    let unsafe_fields = document
        .placeholders
        .iter()
        .chain(pipeline.template_structure.fields.iter())
        .filter(|field_id| !is_valid_field_id(field_id))
        .cloned()
        .collect::<Vec<_>>();

    WorkflowPlan {
        document_id: document.id.clone(),
        prompts,
        blocked: !unsafe_fields.is_empty(),
        block_reasons: unsafe_fields
            .into_iter()
            .map(|field| format!("Небезопасный placeholder: {field}"))
            .collect(),
    }
}

fn suppressed_prompt_fields(document: &DocumentTemplateSpec) -> BTreeSet<&'static str> {
    let role = document.role_id.trim().to_lowercase();
    if matches!(document.category, DomainKind::Medical)
        && (role.contains("diar") || role.contains("днев"))
    {
        return BTreeSet::from(["medical.treatment", "medical.sick_leave_number"]);
    }
    BTreeSet::new()
}

/// Builds one popup for a whole selected document set. Duplicate semantic fields are asked once,
/// while the strictest requirement and ask mode win.
pub fn plan_workflow_batch(
    documents: &[DocumentTemplateSpec],
    case: &SemanticCase,
    flags: &WorkflowFlags,
) -> WorkflowPlan {
    let mut prompts = BTreeMap::<String, PromptSpec>::new();
    let mut blocked_reasons = BTreeSet::<String>::new();
    let mut ids = Vec::new();

    for document in documents {
        ids.push(document.id.clone());
        let plan = plan_workflow(document, case, flags);
        blocked_reasons.extend(plan.block_reasons);
        for prompt in plan.prompts {
            match prompts.get_mut(&prompt.field_id) {
                Some(existing) => merge_prompt(existing, prompt),
                None => {
                    prompts.insert(prompt.field_id.clone(), prompt);
                }
            }
        }
    }

    let mut merged = prompts.into_values().collect::<Vec<_>>();
    merged.sort_by(|a, b| a.order.cmp(&b.order).then(a.field_id.cmp(&b.field_id)));
    WorkflowPlan {
        document_id: format!("batch:{}", ids.join(",")),
        prompts: merged,
        blocked: !blocked_reasons.is_empty(),
        block_reasons: blocked_reasons.into_iter().collect(),
    }
}

fn prompt_from_config(
    config: PopupFieldConfig,
    required_fields: &BTreeSet<String>,
    case: &SemanticCase,
) -> Option<PromptSpec> {
    let existing = case.get(&config.field_id).map(str::to_string);
    let include = match config.ask_mode {
        PromptAskMode::IfMissing => existing.is_none(),
        PromptAskMode::Confirm | PromptAskMode::Always => true,
    };
    if !include {
        return None;
    }
    let default_value = resolve_popup_default(config.default_value.as_deref());
    let current_value = match config.ask_mode {
        PromptAskMode::Always => default_value,
        PromptAskMode::Confirm | PromptAskMode::IfMissing => existing.or(default_value),
    };
    let required = config.required || required_fields.contains(&config.field_id);
    Some(PromptSpec {
        field_id: config.field_id,
        title: config.title,
        required,
        current_value,
        validation_hint: config
            .help_text
            .clone()
            .or_else(|| Some("Заполните поле или явно разрешите продолжение без него".to_string())),
        input_kind: config.input_kind,
        ask_mode: config.ask_mode,
        options: config.options,
        allow_custom_option: config.allow_custom_option,
        help_text: config.help_text,
        section: config.section,
        linked_to: config.linked_to,
        order: config.order,
    })
}

fn merge_prompt(existing: &mut PromptSpec, incoming: PromptSpec) {
    existing.required |= incoming.required;
    if ask_mode_rank(incoming.ask_mode) > ask_mode_rank(existing.ask_mode) {
        existing.ask_mode = incoming.ask_mode;
    }
    if existing.current_value.as_deref().unwrap_or("").is_empty() {
        existing.current_value = incoming.current_value;
    }
    if existing.help_text.is_none() {
        existing.help_text = incoming.help_text;
    }
    if existing.validation_hint.is_none() {
        existing.validation_hint = incoming.validation_hint;
    }
    if existing.linked_to.is_none() {
        existing.linked_to = incoming.linked_to.clone();
    }
    if existing.section.is_none() {
        existing.section = incoming.section;
    } else if existing.section != incoming.section {
        existing.section = Some("Общие данные комплекта".into());
    }
    if existing.options.is_empty() {
        existing.options = incoming.options;
        existing.allow_custom_option = incoming.allow_custom_option;
    } else {
        existing.options.extend(incoming.options);
        existing.options.sort();
        existing.options.dedup();
        existing.allow_custom_option |= incoming.allow_custom_option;
    }
    existing.order = existing.order.min(incoming.order);
}

fn ask_mode_rank(mode: PromptAskMode) -> u8 {
    match mode {
        PromptAskMode::IfMissing => 0,
        PromptAskMode::Confirm => 1,
        PromptAskMode::Always => 2,
    }
}

pub fn validate_required_values(
    plan: &WorkflowPlan,
    answers: &[(String, String)],
    allow_empty: &[String],
) -> Result<(), Vec<String>> {
    let missing = plan
        .prompts
        .iter()
        .filter(|prompt| prompt.required)
        .filter(|prompt| !allow_empty.iter().any(|field| field == &prompt.field_id))
        .filter(|prompt| {
            answers
                .iter()
                .find(|(field, _)| field == &prompt.field_id)
                .map(|(_, value)| value.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|prompt| format!("{} ({})", prompt.title, prompt.field_id))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn domain_hint_from_kind(kind: &DomainKind) -> Option<UniversalDomain> {
    match kind {
        DomainKind::Medical => Some(UniversalDomain::Medical),
        DomainKind::Legal => Some(UniversalDomain::Legal),
        DomainKind::Hr => Some(UniversalDomain::Hr),
        DomainKind::Education => Some(UniversalDomain::Education),
        DomainKind::Accounting => Some(UniversalDomain::Accounting),
        DomainKind::Generic | DomainKind::Custom(_) => None,
    }
}

fn workflow_template_text(document: &DocumentTemplateSpec) -> String {
    let role_or_title = if document.role_id.trim().is_empty() || document.role_id == "unknown" {
        document.button_label.as_str()
    } else {
        document.role_id.as_str()
    };
    let fields = document
        .placeholders
        .iter()
        .chain(document.required_fields.iter())
        .chain(document.popup_fields.iter().map(|field| &field.field_id))
        .map(|field_id| format!("{{{{{field_id}}}}}"))
        .collect::<Vec<_>>()
        .join("\n");
    if fields.is_empty() {
        role_or_title.to_string()
    } else {
        format!("{role_or_title}\n{fields}")
    }
}

fn semantic_case_snapshot_text(case: &SemanticCase) -> String {
    case.values
        .iter()
        .map(|(field_id, value)| format!("{field_id}: {}", value.value))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DomainKind, PopupFieldConfig, PromptAskMode};

    fn document(id: &str, field: &str) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: id.into(),
            button_label: id.into(),
            template_path: format!("{id}.docx"),
            category: DomainKind::Generic,
            role_id: "document".into(),
            required_fields: vec![field.into()],
            placeholders: vec![field.into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn batch_plan_deduplicates_shared_fields() {
        let docs = vec![
            document("one", "subject.name"),
            document("two", "subject.name"),
        ];
        let plan = plan_workflow_batch(&docs, &SemanticCase::default(), &WorkflowFlags::default());
        assert_eq!(
            plan.prompts
                .iter()
                .filter(|p| p.field_id == "subject.name")
                .count(),
            1
        );
    }

    #[test]
    fn always_prompt_is_shown_even_when_case_has_value() {
        let mut doc = document("one", "document.number");
        let mut config = PopupFieldConfig::new("document.number", "Номер");
        config.ask_mode = PromptAskMode::Always;
        doc.popup_fields = vec![config];
        let mut case = SemanticCase::default();
        crate::set_user_value(&mut case, "document.number", "OLD");
        let plan = plan_workflow(&doc, &case, &WorkflowFlags::default());
        assert!(plan
            .prompts
            .iter()
            .any(|prompt| prompt.field_id == "document.number"));
    }
}
