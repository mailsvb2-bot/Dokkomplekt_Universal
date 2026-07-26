use crate::data_schema_engine::{is_safe_field_id, UnifiedDataSchema};
use crate::domain_plugin_layer::{plugin_by_id, DomainPluginV2};
use crate::template_intelligence_engine::TemplateStructureAnalysisV2;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowFlagSetV2 {
    pub flags: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowFieldRequirementV2 {
    pub field_id: String,
    pub title: String,
    pub required: bool,
    pub reason: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonScenarioV2 {
    pub button: String,
    pub document_type: String,
    pub domain: String,
    pub requires: Vec<WorkflowFieldRequirementV2>,
    pub optional: Vec<WorkflowFieldRequirementV2>,
    pub produces: Vec<String>,
    pub blocked: bool,
    pub block_reasons: Vec<String>,
}

pub fn build_button_scenario_v2(
    template: &TemplateStructureAnalysisV2,
    data: &UnifiedDataSchema,
    flags: &WorkflowFlagSetV2,
) -> ButtonScenarioV2 {
    let plugin = plugin_by_id(&template.domain);
    let mut requires: BTreeMap<String, WorkflowFieldRequirementV2> = BTreeMap::new();
    let mut optional: BTreeMap<String, WorkflowFieldRequirementV2> = BTreeMap::new();
    let mut block_reasons = template
        .unsafe_fields
        .iter()
        .map(|x| format!("Небезопасный placeholder: {x}"))
        .collect::<Vec<_>>();

    for field in &template.placeholders {
        if is_safe_field_id(field) {
            requires.insert(
                field.clone(),
                requirement(
                    field,
                    &title_for(&plugin, field),
                    "placeholder in template",
                    data,
                    true,
                ),
            );
        } else {
            block_reasons.push(format!("Небезопасный placeholder: {field}"));
        }
    }

    for rule in &plugin.required_rules {
        if rule.role != "*" && rule.role != template.document_type {
            continue;
        }
        if let Some(flag) = &rule.when_flag {
            if !flags.flags.get(flag).copied().unwrap_or(false) {
                optional.insert(
                    rule.field_id.clone(),
                    requirement(
                        &rule.field_id,
                        &title_for(&plugin, &rule.field_id),
                        &format!("optional until flag {flag}"),
                        data,
                        false,
                    ),
                );
                continue;
            }
        }
        if let Some(unless) = &rule.unless_present {
            if data
                .values
                .get(unless)
                .is_some_and(|v| !v.value.trim().is_empty())
            {
                continue;
            }
        }
        requires.insert(
            rule.field_id.clone(),
            requirement(
                &rule.field_id,
                &title_for(&plugin, &rule.field_id),
                &format!("domain rule: {}", plugin.title),
                data,
                true,
            ),
        );
    }

    // Non-medical scenarios cannot inherit medical prompts. Medicine is a plugin, not core.
    if template.domain != crate::domain_plugin_layer::DomainPluginId::Medical {
        requires.retain(|field, _| !field.starts_with("medical."));
    }

    optional.retain(|field, _| !requires.contains_key(field));
    ButtonScenarioV2 {
        button: template.suggested_button_name.clone(),
        document_type: template.document_type.clone(),
        domain: format!("{:?}", template.domain).to_lowercase(),
        requires: requires.into_values().collect(),
        optional: optional.into_values().collect(),
        produces: if template.placeholders.is_empty() {
            vec!["copy".into()]
        } else {
            vec!["docx".into()]
        },
        blocked: !block_reasons.is_empty(),
        block_reasons,
    }
}

pub fn validate_scenario_answers_v2(
    scenario: &ButtonScenarioV2,
    answers: &BTreeMap<String, String>,
    allow_missing: &BTreeSet<String>,
) -> Vec<String> {
    scenario
        .requires
        .iter()
        .filter(|field| {
            field.required && !field.present && !allow_missing.contains(&field.field_id)
        })
        .filter(|field| {
            answers
                .get(&field.field_id)
                .map(|x| x.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|field| format!("{} ({})", field.title, field.field_id))
        .collect()
}

fn requirement(
    field: &str,
    title: &str,
    reason: &str,
    data: &UnifiedDataSchema,
    required: bool,
) -> WorkflowFieldRequirementV2 {
    WorkflowFieldRequirementV2 {
        field_id: field.into(),
        title: title.into(),
        required,
        reason: reason.into(),
        present: data
            .values
            .get(field)
            .is_some_and(|v| !v.value.trim().is_empty()),
    }
}

fn title_for(plugin: &DomainPluginV2, field: &str) -> String {
    plugin
        .field_definitions
        .iter()
        .find(|def| def.id == field)
        .map(|def| def.title.clone())
        .unwrap_or_else(|| field.rsplit('.').next().unwrap_or(field).replace('_', " "))
}
