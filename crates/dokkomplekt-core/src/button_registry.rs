use crate::{
    analyze_template_text_with_context, best_domain, create_document_spec,
    default_popup_fields_for_document, infer_legacy_template_fields, infer_workspace_profile,
    infer_workspace_workflow_shape, normalize_popup_fields,
    reinforce_workspace_inference_with_pack, DocumentPack, DocumentTemplateSpec, DomainKind,
    PopupFieldConfig, TemplateAnalysis, WorkspaceProfileInference, WorkspaceShapeDocumentInput,
    WorkspaceWorkflowShape,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateCandidate {
    pub document_id: String,
    pub template_path: String,
    pub extracted_text: String,
    pub preferred_button_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_override: Option<DomainKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateConfirmationRow {
    pub document_id: String,
    pub template_path: String,
    pub detected_title: String,
    pub suggested_button_label: String,
    pub editable_button_label: String,
    pub role_id: String,
    pub is_static_copy: bool,
    pub analysis: TemplateAnalysis,
    #[serde(default)]
    pub popup_fields: Vec<PopupFieldConfig>,
    #[serde(default)]
    pub popup_fields_edited: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_override: Option<DomainKind>,
    #[serde(default)]
    pub domain_override_is_explicit: bool,
    #[serde(default)]
    pub workspace_inference: WorkspaceProfileInference,
    #[serde(default)]
    pub workspace_shape: WorkspaceWorkflowShape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ButtonCreationResult {
    pub pack: DocumentPack,
    pub confirmations: Vec<TemplateConfirmationRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ButtonRegistryError {
    #[error("документ не найден: {0}")]
    DocumentNotFound(String),
    #[error("кнопка с таким названием уже существует: {0}")]
    DuplicateLabel(String),
}

/// Renames a configured button without touching or rewriting its source template.
pub fn rename_document_button(
    pack: &mut DocumentPack,
    document_id: &str,
    requested_label: &str,
) -> Result<String, ButtonRegistryError> {
    let normalized = normalize_label(requested_label);
    let mut resolved = normalized.clone();
    if pack.documents.iter().any(|document| {
        document.id != document_id && document.button_label.eq_ignore_ascii_case(&resolved)
    }) {
        for index in 2..500 {
            let candidate = format!("{normalized} ({index})");
            if !pack.documents.iter().any(|document| {
                document.id != document_id && document.button_label.eq_ignore_ascii_case(&candidate)
            }) {
                resolved = candidate;
                break;
            }
        }
        if resolved == normalized {
            resolved = format!("{normalized} ({})", pack.documents.len() + 1);
        }
    }
    let document = pack
        .documents
        .iter_mut()
        .find(|document| document.id == document_id)
        .ok_or_else(|| ButtonRegistryError::DocumentNotFound(document_id.to_string()))?;
    document.button_label = resolved.clone();
    Ok(resolved)
}

/// Removes only the button configuration. The underlying user template remains
/// on disk, so an accidental UI removal never destroys the original document.
pub fn remove_document_button(
    pack: &mut DocumentPack,
    document_id: &str,
) -> Result<DocumentTemplateSpec, ButtonRegistryError> {
    let index = pack
        .documents
        .iter()
        .position(|document| document.id == document_id)
        .ok_or_else(|| ButtonRegistryError::DocumentNotFound(document_id.to_string()))?;
    Ok(pack.documents.remove(index))
}

/// First-run contract: no built-in buttons. Only user-selected templates become buttons.
pub fn empty_first_run_pack(pack_id: impl Into<String>, name: impl Into<String>) -> DocumentPack {
    DocumentPack {
        pack_id: pack_id.into(),
        name: name.into(),
        documents: vec![],
    }
}

pub fn prepare_template_confirmations(
    candidates: &[TemplateCandidate],
) -> Vec<TemplateConfirmationRow> {
    prepare_template_confirmations_with_existing_pack(candidates, None)
}

pub fn prepare_template_confirmations_with_existing_pack(
    candidates: &[TemplateCandidate],
    existing_pack: Option<&DocumentPack>,
) -> Vec<TemplateConfirmationRow> {
    let analyzed = candidates
        .iter()
        .map(|candidate| {
            (
                candidate,
                analyze_template_text_with_context(
                    &candidate.extracted_text,
                    candidate.domain_override.as_ref(),
                    candidate.preferred_button_label.as_deref(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let workspace_inputs = analyzed
        .iter()
        .map(|(candidate, analysis)| (candidate.document_id.clone(), analysis.clone()))
        .collect::<Vec<_>>();
    let mut workspace_inference = infer_workspace_profile(&workspace_inputs);
    if let Some(pack) = existing_pack {
        workspace_inference = reinforce_workspace_inference_with_pack(workspace_inference, pack);
    }
    let automatic_domain = workspace_inference
        .auto_apply
        .then(|| workspace_inference.suggested_domain.clone())
        .flatten();

    let mut used = BTreeSet::new();
    let mut rows = analyzed
        .into_iter()
        .map(|(candidate, initial_analysis)| {
            let effective_domain = candidate
                .domain_override
                .clone()
                .or_else(|| automatic_domain.clone());
            let analysis = effective_domain
                .as_ref()
                .map(|domain| {
                    analyze_template_text_with_context(
                        &candidate.extracted_text,
                        Some(domain),
                        candidate.preferred_button_label.as_deref(),
                    )
                })
                .unwrap_or(initial_analysis);
            let base = candidate
                .preferred_button_label
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(analysis.suggested_button_label.as_str());
            let label = unique_label(base, &mut used);
            let preview_document = create_document_spec(
                candidate.document_id.as_str(),
                candidate.template_path.as_str(),
                &analysis,
                Some(label.as_str()),
            );
            TemplateConfirmationRow {
                document_id: candidate.document_id.clone(),
                template_path: candidate.template_path.clone(),
                detected_title: analysis.title.clone(),
                suggested_button_label: analysis.suggested_button_label.clone(),
                editable_button_label: label,
                role_id: analysis.role_id.clone(),
                is_static_copy: analysis.is_static,
                popup_fields: preview_document.popup_fields,
                popup_fields_edited: false,
                domain_override: effective_domain,
                domain_override_is_explicit: candidate.domain_override.is_some(),
                workspace_inference: workspace_inference.clone(),
                workspace_shape: WorkspaceWorkflowShape::default(),
                analysis,
            }
        })
        .collect::<Vec<_>>();

    let new_document_ids = rows
        .iter()
        .map(|row| row.document_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut shape_inputs = existing_pack
        .into_iter()
        .flat_map(|pack| pack.documents.iter())
        .filter(|document| !new_document_ids.contains(document.id.as_str()))
        .map(|document| {
            let mut field_ids = document.placeholders.clone();
            field_ids.extend(document.required_fields.iter().cloned());
            field_ids.extend(
                document
                    .popup_fields
                    .iter()
                    .map(|field| field.field_id.clone()),
            );
            WorkspaceShapeDocumentInput {
                document_id: document.id.clone(),
                title: document.button_label.clone(),
                role_id: document.role_id.clone(),
                domain: document.category.clone(),
                field_ids,
            }
        })
        .collect::<Vec<_>>();
    shape_inputs.extend(rows.iter().filter_map(|row| {
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.document_id == row.document_id)?;
        let domain = row
            .domain_override
            .clone()
            .unwrap_or_else(|| best_domain(&row.analysis));
        let mut field_ids = row.analysis.placeholders.clone();
        if field_ids.is_empty() {
            field_ids.extend(
                infer_legacy_template_fields(
                    &candidate.extracted_text,
                    Some(&domain),
                    Some(&row.role_id),
                )
                .into_iter()
                .map(|candidate| candidate.field_id),
            );
        }
        Some(WorkspaceShapeDocumentInput {
            document_id: row.document_id.clone(),
            title: row.editable_button_label.clone(),
            role_id: row.role_id.clone(),
            domain,
            field_ids,
        })
    }));
    let workspace_shape = infer_workspace_workflow_shape(&shape_inputs);
    for row in &mut rows {
        row.workspace_shape = workspace_shape.clone();
    }
    rows
}

pub fn create_pack_from_confirmations(
    pack_id: &str,
    name: &str,
    rows: &[TemplateConfirmationRow],
) -> ButtonCreationResult {
    let mut warnings = Vec::new();
    let documents = rows
        .iter()
        .map(|row| {
            let mut doc = create_document_spec(
                row.document_id.as_str(),
                row.template_path.as_str(),
                &row.analysis,
                Some(row.editable_button_label.as_str()),
            );
            let detected_category = doc.category.clone();
            let detected_popup_fields = normalize_popup_fields(&doc.popup_fields);
            let submitted_popup_fields = normalize_popup_fields(&row.popup_fields);
            let user_popup_changes = submitted_popup_fields
                .iter()
                .filter(|submitted| {
                    detected_popup_fields
                        .iter()
                        .find(|default| default.field_id == submitted.field_id)
                        != Some(*submitted)
                })
                .cloned()
                .collect::<Vec<_>>();

            if let Some(domain_override) = &row.domain_override {
                match domain_override {
                    DomainKind::Custom(profile) if profile.trim().is_empty() => warnings.push(
                        format!(
                            "{}: пустой пользовательский профиль проигнорирован; сохранено автоопределение",
                            doc.button_label
                        ),
                    ),
                    DomainKind::Custom(profile) => {
                        doc.category = DomainKind::Custom(profile.trim().to_string());
                    }
                    domain => {
                        doc.category = domain.clone();
                    }
                }
            }

            let category_changed = doc.category != detected_category;
            if category_changed {
                let mut rebuilt = default_popup_fields_for_document(&doc);
                for changed in &user_popup_changes {
                    if let Some(existing) = rebuilt
                        .iter_mut()
                        .find(|existing| existing.field_id == changed.field_id)
                    {
                        *existing = changed.clone();
                    } else {
                        rebuilt.push(changed.clone());
                    }
                }
                doc.popup_fields = normalize_popup_fields(&rebuilt);
                doc.popup_configured = !user_popup_changes.is_empty();
            } else if !row.popup_fields.is_empty() {
                doc.popup_fields = submitted_popup_fields;
                doc.popup_configured = true;
            }

            synchronize_required_fields(&mut doc);

            if doc.is_static_copy {
                warnings.push(format!(
                    "{}: статический шаблон без placeholder'ов; будет создана копия",
                    doc.button_label
                ));
            }
            doc
        })
        .collect::<Vec<DocumentTemplateSpec>>();
    ButtonCreationResult {
        pack: DocumentPack {
            pack_id: pack_id.to_string(),
            name: name.to_string(),
            documents,
        },
        confirmations: rows.to_vec(),
        warnings,
    }
}

/// A button's requirements are derived from the final template + final popup
/// configuration. Never carry requirements from a previously detected domain
/// after the user changes the profession/profile.
fn synchronize_required_fields(document: &mut DocumentTemplateSpec) {
    document.required_fields = document
        .placeholders
        .iter()
        .cloned()
        .chain(
            document
                .popup_fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.field_id.clone()),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
}

pub fn merge_document_pack(existing: &mut DocumentPack, incoming: DocumentPack) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut labels = existing
        .documents
        .iter()
        .map(|document| document.button_label.clone())
        .collect::<BTreeSet<_>>();
    for mut document in incoming.documents {
        if existing
            .documents
            .iter()
            .any(|current| same_template_identity(current, &document))
        {
            warnings.push(format!("Шаблон уже добавлен: {}", document.template_path));
            continue;
        }
        document.button_label = unique_label(&document.button_label, &mut labels);
        existing.documents.push(document);
    }
    warnings
}

fn same_template_identity(left: &DocumentTemplateSpec, right: &DocumentTemplateSpec) -> bool {
    left.id == right.id
        || left.template_path == right.template_path
        || match (
            content_addressed_template_sha256(&left.template_path),
            content_addressed_template_sha256(&right.template_path),
        ) {
            (Some(left_sha), Some(right_sha)) => left_sha == right_sha,
            _ => false,
        }
}

/// Returns whether a newly captured template is already represented by the pack.
///
/// Exact live paths are safe to compare directly. Content hashes are only compared
/// against Dokkomplekt's own content-addressed `template-versions` paths so a random
/// user filename that merely looks like a SHA-256 never becomes a false duplicate.
pub fn document_pack_contains_template_source(
    pack: &DocumentPack,
    template_path: &str,
    template_sha256: &str,
) -> bool {
    let normalized_sha = template_sha256.trim().to_ascii_lowercase();
    let valid_sha =
        normalized_sha.len() == 64 && normalized_sha.bytes().all(|byte| byte.is_ascii_hexdigit());

    pack.documents.iter().any(|document| {
        document.template_path == template_path
            || (valid_sha
                && content_addressed_template_sha256(&document.template_path)
                    .is_some_and(|published_sha| published_sha == normalized_sha))
    })
}

fn content_addressed_template_sha256(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    if !normalized
        .split('/')
        .any(|segment| segment.eq_ignore_ascii_case("template-versions"))
    {
        return None;
    }
    let file_name = normalized.rsplit('/').next()?;
    let stem = file_name.rsplit_once('.').map(|(stem, _)| stem)?;
    (stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| stem.to_ascii_lowercase())
}

fn unique_label(base: &str, used: &mut BTreeSet<String>) -> String {
    let clean = normalize_label(base);
    if used.insert(clean.clone()) {
        return clean;
    }
    for index in 2..500 {
        let candidate = format!("{clean} {index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    format!("{} {}", clean, used.len() + 1)
}

fn normalize_label(label: &str) -> String {
    let mut out = label.split_whitespace().collect::<Vec<_>>().join(" ");
    out = out
        .trim_matches(|character: char| matches!(character, ':' | ';' | ',' | '.' | ' '))
        .to_string();
    if out.is_empty() {
        "Документ".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_pack_has_no_built_in_documents() {
        let pack = empty_first_run_pack("default", "Пользовательский пакет");
        assert!(pack.documents.is_empty());
    }

    #[test]
    fn confirmations_use_detected_titles_and_unique_labels() {
        let rows = prepare_template_confirmations(&[
            TemplateCandidate {
                document_id: "a".into(),
                template_path: "a.docx".into(),
                extracted_text: "Выписной эпикриз\n{{subject.name}}".into(),
                preferred_button_label: None,
                domain_override: None,
            },
            TemplateCandidate {
                document_id: "b".into(),
                template_path: "b.docx".into(),
                extracted_text: "Выписной эпикриз\n{{subject.name}}".into(),
                preferred_button_label: None,
                domain_override: None,
            },
        ]);
        assert_eq!(rows[0].editable_button_label, "Выписной эпикриз");
        assert_eq!(rows[1].editable_button_label, "Выписной эпикриз 2");
    }

    #[test]
    fn coherent_workspace_profile_is_applied_to_every_created_button() {
        let rows = prepare_template_confirmations(&[
            TemplateCandidate {
                document_id: "primary".into(),
                template_path: "primary.docx".into(),
                extracted_text: "Первичный осмотр\nДиагноз\nЛечение\nМКБ-10\nИстория болезни"
                    .into(),
                preferred_button_label: None,
                domain_override: None,
            },
            TemplateCandidate {
                document_id: "discharge".into(),
                template_path: "discharge.docx".into(),
                extracted_text: "Выписной эпикриз\nДиагноз\nЛечение\nДата выписки".into(),
                preferred_button_label: None,
                domain_override: None,
            },
            TemplateCandidate {
                document_id: "consent".into(),
                template_path: "consent.docx".into(),
                extracted_text: "Согласие пациента\n{{subject.name}}\n{{Должность}}".into(),
                preferred_button_label: None,
                domain_override: None,
            },
        ]);

        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .all(|row| row.domain_override == Some(DomainKind::Medical)));
        assert!(rows.iter().all(|row| !row.domain_override_is_explicit));
        assert!(rows.iter().all(|row| row.workspace_inference.auto_apply));
        let consent = rows
            .iter()
            .find(|row| row.document_id == "consent")
            .unwrap();
        assert!(consent
            .analysis
            .placeholders
            .contains(&"medical.position".to_string()));
        assert!(!consent
            .analysis
            .placeholders
            .contains(&"employee.position".to_string()));

        let result = create_pack_from_confirmations("default", "Pack", &rows);
        assert!(result
            .pack
            .documents
            .iter()
            .all(|document| document.category == DomainKind::Medical));
    }

    #[test]
    fn ambiguous_single_template_keeps_automatic_profile_unset() {
        let rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "act".into(),
            template_path: "act.docx".into(),
            extracted_text: "Акт\nНомер {{document.number}}".into(),
            preferred_button_label: None,
            domain_override: None,
        }]);

        assert_eq!(rows[0].domain_override, None);
        assert!(!rows[0].workspace_inference.auto_apply);
        assert_eq!(
            rows[0].workspace_inference.level,
            crate::WorkspaceInferenceLevel::Low
        );
    }

    #[test]
    fn explicit_candidate_domain_is_distinguished_from_workspace_inference() {
        let rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "report".into(),
            template_path: "report.docx".into(),
            extracted_text: "Отчёт\n{{Должность}}".into(),
            preferred_button_label: Some("Отчёт".into()),
            domain_override: Some(DomainKind::Hr),
        }]);

        assert_eq!(rows[0].domain_override, Some(DomainKind::Hr));
        assert!(rows[0].domain_override_is_explicit);
    }

    #[test]
    fn explicit_custom_domain_override_is_saved_in_document_pack() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "custom-report".into(),
            template_path: "custom-report.docx".into(),
            extracted_text: "Report\n{{custom.project}}".into(),
            preferred_button_label: Some("Report".into()),
            domain_override: None,
        }]);
        rows[0].domain_override = Some(DomainKind::Custom("  architecture  ".into()));

        let result = create_pack_from_confirmations("default", "Pack", &rows);

        assert_eq!(
            result.pack.documents[0].category,
            DomainKind::Custom("architecture".into())
        );
    }

    #[test]
    fn empty_custom_domain_override_fails_closed_to_detected_domain() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "custom-report".into(),
            template_path: "custom-report.docx".into(),
            extracted_text: "Report\n{{custom.project}}".into(),
            preferred_button_label: Some("Report".into()),
            domain_override: None,
        }]);
        let detected = create_pack_from_confirmations("detected", "Pack", &rows)
            .pack
            .documents[0]
            .category
            .clone();
        rows[0].domain_override = Some(DomainKind::Custom("   ".into()));

        let result = create_pack_from_confirmations("default", "Pack", &rows);

        assert_eq!(result.pack.documents[0].category, detected);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("пустой пользовательский профиль")));
    }

    #[test]
    fn domain_override_rebuilds_unedited_popup_defaults_for_new_domain() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "profiled".into(),
            template_path: "profiled.docx".into(),
            extracted_text: "Выписной эпикриз\n{{subject.name}}".into(),
            preferred_button_label: Some("Report".into()),
            domain_override: None,
        }]);
        assert!(rows[0]
            .popup_fields
            .iter()
            .any(|field| field.field_id.starts_with("medical.")));
        rows[0].domain_override = Some(DomainKind::Custom("architecture".into()));

        let result = create_pack_from_confirmations("default", "Pack", &rows);
        let document = &result.pack.documents[0];

        assert_eq!(document.category, DomainKind::Custom("architecture".into()));
        assert!(!document.popup_configured);
        assert!(document
            .popup_fields
            .iter()
            .all(|field| !field.field_id.starts_with("medical.")));
        assert!(document
            .required_fields
            .iter()
            .all(|field| !field.starts_with("medical.")));
        assert!(document
            .required_fields
            .iter()
            .any(|field| field == "subject.name"));
    }

    #[test]
    fn explicit_template_field_survives_domain_override() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "profiled".into(),
            template_path: "profiled.docx".into(),
            extracted_text: "Выписной эпикриз\n{{medical.diagnosis}}".into(),
            preferred_button_label: Some("Report".into()),
            domain_override: None,
        }]);
        rows[0].domain_override = Some(DomainKind::Custom("architecture".into()));

        let result = create_pack_from_confirmations("default", "Pack", &rows);
        let document = &result.pack.documents[0];

        assert_eq!(document.category, DomainKind::Custom("architecture".into()));
        assert!(document
            .required_fields
            .iter()
            .any(|field| field == "medical.diagnosis"));
    }

    #[test]
    fn domain_override_preserves_only_user_changed_popup_fields() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "profiled".into(),
            template_path: "profiled.docx".into(),
            extracted_text: "Выписной эпикриз\n{{subject.name}}".into(),
            preferred_button_label: Some("Report".into()),
            domain_override: None,
        }]);
        rows[0]
            .popup_fields
            .push(PopupFieldConfig::new("custom.site", "Site"));
        rows[0].domain_override = Some(DomainKind::Custom("architecture".into()));

        let result = create_pack_from_confirmations("default", "Pack", &rows);
        let document = &result.pack.documents[0];

        assert!(document.popup_configured);
        assert!(document
            .popup_fields
            .iter()
            .any(|field| field.field_id == "custom.site"));
        assert!(document
            .popup_fields
            .iter()
            .all(|field| !field.field_id.starts_with("medical.")));
        assert!(document
            .required_fields
            .iter()
            .all(|field| !field.starts_with("medical.")));
    }

    fn persisted_document(id: &str, domain: DomainKind, role_id: &str) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: id.into(),
            button_label: format!("Persisted {id}"),
            template_path: format!("{id}.docx"),
            category: domain,
            role_id: role_id.into(),
            required_fields: vec!["subject.name".into()],
            placeholders: vec!["subject.name".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn persisted_workspace_correction_guides_ambiguous_future_template_and_full_shape() {
        let existing = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![persisted_document("claim", DomainKind::Legal, "claim")],
        };
        let rows = prepare_template_confirmations_with_existing_pack(
            &[TemplateCandidate {
                document_id: "act".into(),
                template_path: "act.docx".into(),
                extracted_text: "Акт\nНомер {{document.number}}".into(),
                preferred_button_label: Some("Акт".into()),
                domain_override: None,
            }],
            Some(&existing),
        );

        assert_eq!(rows[0].domain_override, Some(DomainKind::Legal));
        assert!(!rows[0].domain_override_is_explicit);
        assert!(rows[0].workspace_inference.auto_apply);
        assert_eq!(rows[0].workspace_shape.documents.len(), 2);
        assert!(rows[0]
            .workspace_shape
            .documents
            .iter()
            .any(
                |document| document.document_id == "claim" && document.domain == DomainKind::Legal
            ));
        assert!(rows[0]
            .workspace_shape
            .documents
            .iter()
            .any(|document| document.document_id == "act" && document.domain == DomainKind::Legal));
    }

    #[test]
    fn strong_new_profession_signal_starts_second_contour_instead_of_inheriting_old_domain() {
        let existing = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![persisted_document("claim", DomainKind::Legal, "claim")],
        };
        let rows = prepare_template_confirmations_with_existing_pack(
            &[TemplateCandidate {
                document_id: "hire".into(),
                template_path: "hire.docx".into(),
                extracted_text: "Приказ о приёме сотрудника\nРаботодатель\nРаботник\nДолжность\nОтдел\nКадровая служба".into(),
                preferred_button_label: Some("Приказ о приёме".into()),
                domain_override: None,
            }],
            Some(&existing),
        );

        assert_ne!(rows[0].domain_override, Some(DomainKind::Legal));
        assert_eq!(best_domain(&rows[0].analysis), DomainKind::Hr);
        assert!(rows[0].workspace_shape.mixed_workflows);
        assert!(rows[0]
            .workspace_shape
            .groups
            .iter()
            .any(|group| group.domain == DomainKind::Legal));
        assert!(rows[0]
            .workspace_shape
            .groups
            .iter()
            .any(|group| group.domain == DomainKind::Hr));
    }

    #[test]
    fn mixed_existing_workspace_does_not_force_ambiguous_new_template() {
        let existing = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![
                persisted_document("claim", DomainKind::Legal, "claim"),
                persisted_document("hire", DomainKind::Hr, "employment_order"),
            ],
        };
        let rows = prepare_template_confirmations_with_existing_pack(
            &[TemplateCandidate {
                document_id: "note".into(),
                template_path: "note.docx".into(),
                extracted_text: "Документ\nНомер {{document.number}}".into(),
                preferred_button_label: Some("Документ".into()),
                domain_override: None,
            }],
            Some(&existing),
        );

        assert_eq!(rows[0].domain_override, None);
        assert!(!rows[0].workspace_inference.auto_apply);
        assert_eq!(rows[0].workspace_shape.documents.len(), 3);
        assert!(rows[0].workspace_shape.mixed_workflows);
    }

    #[test]
    fn merge_rejects_same_content_addressed_template_under_new_document_id() {
        let sha = "a".repeat(64);
        let mut existing = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![persisted_document("old", DomainKind::Legal, "claim")],
        };
        existing.documents[0].template_path = format!("C:/app/template-versions/old/{sha}.docx");
        let mut duplicate = persisted_document("new", DomainKind::Legal, "claim");
        duplicate.template_path = format!("C:/app/template-versions/new/{sha}.docx");

        let warnings = merge_document_pack(
            &mut existing,
            DocumentPack {
                pack_id: "incoming".into(),
                name: "incoming".into(),
                documents: vec![duplicate],
            },
        );

        assert_eq!(existing.documents.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Шаблон уже добавлен"));
    }

    #[test]
    fn captured_template_source_matches_published_content_hash() {
        let sha = "c".repeat(64);
        let mut published = persisted_document("old", DomainKind::Legal, "claim");
        published.template_path = format!("C:/app/template-versions/old/{sha}.docx");
        let pack = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![published],
        };

        assert!(document_pack_contains_template_source(
            &pack,
            "D:/incoming/claim.docx",
            &sha,
        ));
    }

    #[test]
    fn captured_template_source_keeps_exact_live_path_identity() {
        let mut published = persisted_document("old", DomainKind::Legal, "claim");
        published.template_path = "D:/incoming/claim.docx".into();
        let pack = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![published],
        };

        assert!(document_pack_contains_template_source(
            &pack,
            "D:/incoming/claim.docx",
            &"d".repeat(64),
        ));
    }

    #[test]
    fn captured_template_source_does_not_hash_match_normal_user_paths() {
        let sha = "e".repeat(64);
        let mut published = persisted_document("old", DomainKind::Legal, "claim");
        published.template_path = format!("C:/user/{sha}.docx");
        let pack = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![published],
        };

        assert!(!document_pack_contains_template_source(
            &pack,
            "D:/incoming/claim.docx",
            &sha,
        ));
    }

    #[test]
    fn merge_never_guesses_sha_identity_from_normal_user_paths() {
        let sha = "b".repeat(64);
        let mut existing = DocumentPack {
            pack_id: "default".into(),
            name: "workspace".into(),
            documents: vec![persisted_document("old", DomainKind::Legal, "claim")],
        };
        existing.documents[0].template_path = format!("C:/user/{sha}.docx");
        let mut incoming_document = persisted_document("new", DomainKind::Legal, "claim");
        incoming_document.template_path = format!("D:/other/{sha}.docx");

        let warnings = merge_document_pack(
            &mut existing,
            DocumentPack {
                pack_id: "incoming".into(),
                name: "incoming".into(),
                documents: vec![incoming_document],
            },
        );

        assert!(warnings.is_empty());
        assert_eq!(existing.documents.len(), 2);
    }

    #[test]
    fn rename_and_remove_change_registry_but_preserve_template_reference() {
        let mut pack = DocumentPack {
            pack_id: "default".into(),
            name: "Pack".into(),
            documents: vec![DocumentTemplateSpec {
                id: "doc".into(),
                button_label: "Old".into(),
                template_path: "templates/original.docm".into(),
                category: crate::DomainKind::Generic,
                role_id: "generic".into(),
                required_fields: Vec::new(),
                placeholders: vec!["document.number".into()],
                is_static_copy: false,
                popup_fields: Vec::new(),
                popup_configured: false,
            }],
        };
        assert_eq!(
            rename_document_button(&mut pack, "doc", "  New name. ").expect("rename"),
            "New name"
        );
        let removed = remove_document_button(&mut pack, "doc").expect("remove");
        assert_eq!(removed.template_path, "templates/original.docm");
        assert!(pack.documents.is_empty());
    }

    #[test]
    fn rename_collision_uses_donor_suffix_without_changing_document_identity() {
        let make = |id: &str, label: &str, template: &str| DocumentTemplateSpec {
            id: id.into(),
            button_label: label.into(),
            template_path: template.into(),
            category: crate::DomainKind::Generic,
            role_id: "generic".into(),
            required_fields: vec!["document.number".into()],
            placeholders: vec!["document.number".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };
        let mut pack = DocumentPack {
            pack_id: "default".into(),
            name: "Pack".into(),
            documents: vec![
                make("d1", "Акт", "templates/act.docx"),
                make("d2", "Эпикриз", "templates/epicrisis.docx"),
            ],
        };

        let label = rename_document_button(&mut pack, "d1", "Эпикриз").expect("rename");
        assert_eq!(label, "Эпикриз (2)");
        let renamed = pack
            .documents
            .iter()
            .find(|document| document.id == "d1")
            .unwrap();
        assert_eq!(renamed.id, "d1");
        assert_eq!(renamed.template_path, "templates/act.docx");
        assert_eq!(renamed.required_fields, vec!["document.number"]);
    }
}
