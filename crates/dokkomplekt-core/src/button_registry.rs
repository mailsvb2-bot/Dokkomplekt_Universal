use crate::{
    analyze_template_text, create_document_spec, default_popup_fields_for_document,
    normalize_popup_fields, DocumentPack, DocumentTemplateSpec, DomainKind, PopupFieldConfig,
    TemplateAnalysis,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateCandidate {
    pub document_id: String,
    pub template_path: String,
    pub extracted_text: String,
    pub preferred_button_label: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_override: Option<DomainKind>,
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
    if pack.documents.iter().any(|document| {
        document.id != document_id && document.button_label.eq_ignore_ascii_case(&normalized)
    }) {
        return Err(ButtonRegistryError::DuplicateLabel(normalized));
    }
    let document = pack
        .documents
        .iter_mut()
        .find(|document| document.id == document_id)
        .ok_or_else(|| ButtonRegistryError::DocumentNotFound(document_id.to_string()))?;
    document.button_label = normalized.clone();
    Ok(normalized)
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
    let mut used = BTreeSet::new();
    candidates
        .iter()
        .map(|candidate| {
            let analysis = analyze_template_text(&candidate.extracted_text);
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
                domain_override: None,
                analysis,
            }
        })
        .collect()
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
                    DomainKind::Custom(profile) if profile.trim().is_empty() => warnings.push(format!(
                        "{}: пустой пользовательский профиль проигнорирован; сохранено автоопределение",
                        doc.button_label
                    )),
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
                if doc.popup_configured {
                    doc.required_fields = doc
                        .popup_fields
                        .iter()
                        .filter(|field| field.required)
                        .map(|field| field.field_id.clone())
                        .chain(doc.required_fields.iter().cloned())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                }
            } else if !row.popup_fields.is_empty() {
                doc.popup_fields = submitted_popup_fields;
                doc.popup_configured = true;
                doc.required_fields = doc
                    .popup_fields
                    .iter()
                    .filter(|field| field.required)
                    .map(|field| field.field_id.clone())
                    .chain(doc.required_fields.iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
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

pub fn merge_document_pack(existing: &mut DocumentPack, incoming: DocumentPack) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut labels = existing
        .documents
        .iter()
        .map(|document| document.button_label.clone())
        .collect::<BTreeSet<_>>();
    for mut document in incoming.documents {
        if existing.documents.iter().any(|current| {
            current.id == document.id || current.template_path == document.template_path
        }) {
            warnings.push(format!("Шаблон уже добавлен: {}", document.template_path));
            continue;
        }
        document.button_label = unique_label(&document.button_label, &mut labels);
        existing.documents.push(document);
    }
    warnings
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
            },
            TemplateCandidate {
                document_id: "b".into(),
                template_path: "b.docx".into(),
                extracted_text: "Выписной эпикриз\n{{subject.name}}".into(),
                preferred_button_label: None,
            },
        ]);
        assert_eq!(rows[0].editable_button_label, "Выписной эпикриз");
        assert_eq!(rows[1].editable_button_label, "Выписной эпикриз 2");
    }

    #[test]
    fn explicit_custom_domain_override_is_saved_in_document_pack() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "custom-report".into(),
            template_path: "custom-report.docx".into(),
            extracted_text: "Report\n{{custom.project}}".into(),
            preferred_button_label: Some("Report".into()),
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
    }

    #[test]
    fn domain_override_preserves_only_user_changed_popup_fields() {
        let mut rows = prepare_template_confirmations(&[TemplateCandidate {
            document_id: "profiled".into(),
            template_path: "profiled.docx".into(),
            extracted_text: "Выписной эпикриз\n{{subject.name}}".into(),
            preferred_button_label: Some("Report".into()),
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
}
