//! Zero-touch «Созданные документы» batch orchestrator.
//!
//! The essence of the product: a specialist drops one fully-filled primary
//! document into the watched folder and the program builds the *entire*
//! configured document set by itself — no popups, no manual fixes.
//!
//! This module is the pure decision core. It never touches the filesystem: it
//! takes the parsed source case plus the doctor-configured documents (with their
//! template text) and returns either
//! * `Ready` — the full set is renderable, with a fresh output folder name
//!   and every output already rendered, or
//! * `Attention` — required data is missing; nothing is created and a single
//!   `*_ТРЕБУЕТ_ВНИМАНИЯ.txt` note is described for the caller to drop next to
//!   the untouched source.
//!
//! Hard guarantees (mirrored from the Python zero-touch contract):
//! * Never fabricate missing facts — a missing required field blocks the run.
//! * Never produce a partial set — if any output is incomplete, none are written.
//! * Never emit a document that still contains unfilled placeholders.
//!
//! The filesystem side effects (create folder, move source, write files/note) live
//! in the thin Tauri command; the "process once / ignore temp & duplicates" rule
//! lives in [`crate::intake_agent::IntakeDeduplicator`].

use serde::{Deserialize, Serialize};

use crate::{
    build_output_folder_name, plan_workflow, render_text_template, required_blocks_for,
    sanitize_folder_name, sanitize_path_component, title_for_field, unmet_blocks,
    DocumentTemplateSpec, FolderNamePart, SemanticCase, WorkflowFlags,
};

pub const ATTENTION_SUFFIX: &str = "_ТРЕБУЕТ_ВНИМАНИЯ.txt";
pub const ATTENTION_TITLE: &str = "НЕ ХВАТАЕТ ДАННЫХ В ИСХОДНОМ ДОКУМЕНТЕ";

/// One rendered target document, ready to be written into the output folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedOutput {
    pub document_id: String,
    pub button_label: String,
    pub file_name: String,
    pub rendered_text: String,
}

/// A configured target the batch can create: its spec plus the template text
/// (read from the user's DOCX by the caller, or supplied inline in tests).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfiguredDocument {
    pub spec: DocumentTemplateSpec,
    pub template_text: String,
}

/// Outcome of a single dropped source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CreatedDocumentsBatch {
    /// Full set is renderable — write everything into `patient_folder_name`.
    Ready {
        patient_folder_name: String,
        source_target_name: String,
        outputs: Vec<PlannedOutput>,
    },
    /// Not enough data — create nothing, drop `attention_file_name` beside the source.
    Attention {
        title: String,
        missing: Vec<String>,
        attention_file_name: String,
        attention_text: String,
    },
}

pub fn attention_file_name(source_stem: &str) -> String {
    format!("{source_stem}{ATTENTION_SUFFIX}")
}

fn dedup_preserve(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

/// Build the visible, non-technical note left next to an unprocessed source.
pub fn build_attention_text(title: &str, missing: &[String]) -> String {
    let mut lines = vec![
        title.to_string(),
        String::new(),
        "Автоматическое создание документов остановлено безопасно.".to_string(),
        "Исходный документ не удалён и не перемещён.".to_string(),
        String::new(),
        "Не найдены обязательные данные:".to_string(),
    ];
    for item in missing {
        let text = item.trim();
        if !text.is_empty() {
            lines.push(format!("- {text}"));
        }
    }
    lines.push(String::new());
    lines.push(
        "После исправления исходного файла программа увидит новую версию и повторит обработку автоматически."
            .to_string(),
    );
    let body = lines.join("\n");
    format!("{}\n", body.trim_end())
}

/// Decide the whole outcome for one dropped source.
///
/// * `case` — semantic values parsed from the source document.
/// * `documents` — every doctor-configured output (spec + template text).
/// * `folder_parts` — how to name the case subfolder.
/// * `source_stem` / `source_file_name` — used for the attention note and the
///   in-folder copy of the source.
pub fn plan_created_documents_batch(
    case: &SemanticCase,
    documents: &[ConfiguredDocument],
    flags: &WorkflowFlags,
    folder_parts: &[FolderNamePart],
    source_stem: &str,
    source_file_name: &str,
) -> CreatedDocumentsBatch {
    let mut missing: Vec<String> = Vec::new();

    if documents.is_empty() {
        missing.push("не настроен ни один документ для автоматического создания".to_string());
    }

    // Completeness is render-driven: a document is creatable iff its template
    // renders with no unfilled required placeholders. This is the real zero-touch
    // guarantee ("never emit an unfilled document") and avoids over-requiring
    // fields that only matter for the interactive popup path. Genuine hard blocks
    // from the workflow engine still stop the run.
    let mut outputs: Vec<PlannedOutput> = Vec::new();
    for configured in documents {
        let label = &configured.spec.button_label;
        let plan = plan_workflow(&configured.spec, case, flags);
        if plan.blocked {
            for reason in &plan.block_reasons {
                missing.push(format!("{label}: {reason}"));
            }
        }
        let result = render_text_template(&configured.template_text, case, true);
        if !result.missing_fields.is_empty() {
            for field in &result.missing_fields {
                missing.push(format!("{label}: {}", title_for_field(field)));
            }
            continue;
        }
        if !result.unknown_fields.is_empty() {
            for field in &result.unknown_fields {
                missing.push(format!("{label}: неизвестный плейсхолдер — {field}"));
            }
            continue;
        }
        if !result.template_errors.is_empty() {
            for error in &result.template_errors {
                missing.push(format!("{label}: ошибка шаблона — {error}"));
            }
            continue;
        }
        // Do not scan the rendered text for a raw `{{` substring. A value or a
        // deliberately escaped literal may legitimately contain double braces.
        // The real parser above is the source of truth: missing fields, unknown
        // fields and malformed/unclosed tags are already reported structurally.
        // Composite-block completeness: a fully-substituted template can still be an
        // incomplete document of its kind (e.g. an epicrisis with no diagnosis or no
        // physician signature block). Any unmet mandatory block blocks the whole run.
        let blocks = required_blocks_for(&configured.spec, &configured.template_text);
        let unmet = unmet_blocks(&blocks, case, &result.output_text);
        if !unmet.is_empty() {
            for block_title in unmet {
                missing.push(format!(
                    "{label}: обязательный блок отсутствует — {block_title}"
                ));
            }
            continue;
        }
        outputs.push(PlannedOutput {
            document_id: configured.spec.id.clone(),
            button_label: label.clone(),
            file_name: format!("{}.docx", sanitize_path_component(label)),
            rendered_text: result.output_text,
        });
    }

    let missing = dedup_preserve(missing);
    if !missing.is_empty() {
        let attention_text = build_attention_text(ATTENTION_TITLE, &missing);
        return CreatedDocumentsBatch::Attention {
            title: ATTENTION_TITLE.to_string(),
            missing,
            attention_file_name: attention_file_name(source_stem),
            attention_text,
        };
    }

    let folder = sanitize_folder_name(&build_output_folder_name(case, folder_parts));
    CreatedDocumentsBatch::Ready {
        patient_folder_name: folder,
        source_target_name: source_file_name.to_string(),
        outputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DomainKind, SemanticValue, ValueSource};
    use std::collections::BTreeMap;

    fn value(v: &str) -> SemanticValue {
        SemanticValue {
            field_id: String::new(),
            value: v.to_string(),
            source: ValueSource::UserConfirmed,
            confidence: 1.0,
            evidence: Vec::new(),
        }
    }

    fn case_with(pairs: &[(&str, &str)]) -> SemanticCase {
        let mut values = BTreeMap::new();
        for (k, v) in pairs {
            let mut sv = value(v);
            sv.field_id = (*k).to_string();
            values.insert((*k).to_string(), sv);
        }
        SemanticCase {
            values,
            active_domains: vec![],
            ..Default::default()
        }
    }

    fn doc(id: &str, label: &str, template: &str, placeholders: &[&str]) -> ConfiguredDocument {
        ConfiguredDocument {
            spec: DocumentTemplateSpec {
                id: id.into(),
                button_label: label.into(),
                template_path: format!("templates/{id}.docx"),
                category: DomainKind::Generic,
                role_id: "generic".into(),
                required_fields: placeholders.iter().map(|s| s.to_string()).collect(),
                placeholders: placeholders.iter().map(|s| s.to_string()).collect(),
                is_static_copy: false,
                popup_fields: Vec::new(),
                popup_configured: false,
            },
            template_text: template.into(),
        }
    }

    #[test]
    fn complete_source_creates_whole_set_in_one_folder() {
        let case = case_with(&[("subject.name", "Иванов Иван"), ("org.name", "ООО Ромашка")]);
        let docs = vec![
            doc("d1", "Договор", "Организация {{org.name}}", &["org.name"]),
            doc(
                "d2",
                "Справка",
                "Пациент {{subject.name}}",
                &["subject.name"],
            ),
        ];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        match batch {
            CreatedDocumentsBatch::Ready {
                outputs,
                patient_folder_name,
                source_target_name,
            } => {
                assert_eq!(outputs.len(), 2);
                assert!(outputs.iter().all(|o| !o.rendered_text.contains("{{")));
                assert!(outputs
                    .iter()
                    .any(|o| o.rendered_text.contains("ООО Ромашка")));
                assert_eq!(source_target_name, "Первичный.docx");
                assert!(patient_folder_name.contains("Иванов"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn missing_required_field_blocks_and_creates_nothing() {
        let case = case_with(&[("subject.name", "Иванов Иван")]);
        let docs = vec![doc(
            "d1",
            "Договор",
            "Организация {{org.name}}",
            &["org.name"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        match batch {
            CreatedDocumentsBatch::Attention {
                missing,
                attention_file_name,
                attention_text,
                ..
            } => {
                assert!(missing.iter().any(|m| m.contains("Договор")));
                assert_eq!(attention_file_name, "Первичный_ТРЕБУЕТ_ВНИМАНИЯ.txt");
                assert!(attention_text.contains("остановлено безопасно"));
            }
            other => panic!("expected Attention, got {other:?}"),
        }
    }

    #[test]
    fn no_configured_documents_is_attention_not_empty_success() {
        let case = case_with(&[("subject.full_name", "Иванов")]);
        let batch = plan_created_documents_batch(
            &case,
            &[],
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        assert!(matches!(batch, CreatedDocumentsBatch::Attention { .. }));
    }

    fn medical_doc(
        id: &str,
        label: &str,
        template: &str,
        placeholders: &[&str],
    ) -> ConfiguredDocument {
        let mut d = doc(id, label, template, placeholders);
        d.spec.role_id = "discharge".into();
        d.spec.category = DomainKind::Medical;
        d
    }

    #[test]
    fn medical_document_without_required_block_is_refused() {
        // Every placeholder is filled, but a discharge epicrisis needs a diagnosis and
        // a physician signature block; both are absent -> nothing is created.
        let case = case_with(&[("subject.name", "Иванов Иван")]);
        let docs = vec![medical_doc(
            "d1",
            "Выписной эпикриз",
            "Пациент {{subject.name}}",
            &["subject.name"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        match batch {
            CreatedDocumentsBatch::Attention { missing, .. } => {
                assert!(missing.iter().any(|m| m.contains("Диагноз")));
                assert!(missing.iter().any(|m| m.contains("подписи")));
            }
            other => panic!("expected Attention, got {other:?}"),
        }
    }

    #[test]
    fn medical_document_with_all_blocks_is_created() {
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "J06.9"),
        ]);
        let docs = vec![medical_doc(
            "d1",
            "Выписной эпикриз",
            "Пациент {{subject.name}}\nДиагноз {{medical.diagnosis}}\nЛечащий врач ______",
            &["subject.name", "medical.diagnosis"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        assert!(matches!(batch, CreatedDocumentsBatch::Ready { .. }));
    }

    #[test]
    fn angle_brackets_and_shift_operators_are_not_placeholders() {
        let case = case_with(&[("org.name", "ООО Ромашка")]);
        let docs = vec![doc(
            "d1",
            "Технический отчёт",
            "Компания <<Ромашка>>; проверка: a << b и c >> d; {{org.name}}",
            &["org.name"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        match batch {
            CreatedDocumentsBatch::Ready { outputs, .. } => {
                assert_eq!(outputs.len(), 1);
                assert!(outputs[0].rendered_text.contains("<<Ромашка>>"));
                assert!(outputs[0].rendered_text.contains("a << b"));
                assert!(outputs[0].rendered_text.contains("c >> d"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn legitimate_double_braces_in_inserted_value_do_not_block_zero_touch() {
        let case = case_with(&[("custom.code", "const example = {{ nested_template }};")]);
        let docs = vec![doc(
            "d1",
            "Технический отчёт",
            "Фрагмент: {{custom.code}}",
            &["custom.code"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        match batch {
            CreatedDocumentsBatch::Ready { outputs, .. } => {
                assert_eq!(outputs.len(), 1);
                assert!(outputs[0].rendered_text.contains("{{ nested_template }}"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn unknown_placeholder_left_in_text_is_refused() {
        // {{totally.invalid}} is not a valid field id -> stays in output -> must block.
        let case = case_with(&[("org.name", "ООО")]);
        let docs = vec![doc(
            "d1",
            "Бланк",
            "{{org.name}} и {{totally.invalid}}",
            &["org.name"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: false,
            },
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        assert!(matches!(batch, CreatedDocumentsBatch::Attention { .. }));
    }
}
