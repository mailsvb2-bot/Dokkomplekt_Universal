//! Zero-touch «Созданные документы» batch orchestrator.
//!
//! A specialist drops one filled source document and the program builds the entire
//! configured document set.  The function is pure: it returns either the complete
//! rendered set or one attention result; callers perform filesystem side effects.

use serde::{Deserialize, Serialize};

use crate::{
    build_output_folder_name, missing_output_folder_fields, plan_workflow, render_text_template,
    required_blocks_for, sanitize_folder_name, sanitize_path_component, title_for_field,
    unmet_blocks, DocumentTemplateSpec, DomainKind, FolderNamePart, SemanticCase, WorkflowFlags,
};

pub const ATTENTION_SUFFIX: &str = "_ТРЕБУЕТ_ВНИМАНИЯ.txt";
pub const ATTENTION_TITLE: &str = "НЕ ХВАТАЕТ ДАННЫХ В ИСХОДНОМ ДОКУМЕНТЕ";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedOutput {
    pub document_id: String,
    pub button_label: String,
    pub file_name: String,
    pub rendered_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfiguredDocument {
    pub spec: DocumentTemplateSpec,
    pub template_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CreatedDocumentsBatch {
    Ready {
        patient_folder_name: String,
        source_target_name: String,
        outputs: Vec<PlannedOutput>,
    },
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
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

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

pub fn plan_created_documents_batch(
    case: &SemanticCase,
    documents: &[ConfiguredDocument],
    flags: &WorkflowFlags,
    folder_parts: &[FolderNamePart],
    source_stem: &str,
    source_file_name: &str,
) -> CreatedDocumentsBatch {
    let mut missing = Vec::new();
    for field_id in missing_output_folder_fields(case, folder_parts) {
        missing.push(format!("Папка результата: {}", title_for_field(&field_id)));
    }
    if documents.is_empty() {
        missing.push("не настроен ни один документ для автоматического создания".to_string());
    }

    let mut outputs = Vec::new();
    for configured in documents {
        let label = &configured.spec.button_label;
        let plan = plan_workflow(&configured.spec, case, flags);
        if plan.blocked {
            for reason in &plan.block_reasons {
                missing.push(format!("{label}: {reason}"));
            }
        }
        let workflow_blockers = crate::workflow_publication_blockers(case, &plan);
        if !workflow_blockers.is_empty() {
            for blocker in workflow_blockers {
                missing.push(format!("{label}: {blocker}"));
            }
        }

        // MSE and sick-leave VK can coexist in one case with independent protocol
        // requisites. Old user templates still contain generic medical.protocol_*
        // placeholders, so project only the current role's scoped values onto those
        // legacy ids for this render. Persistent case data is never mutated.
        let render_case = if matches!(configured.spec.category, DomainKind::Medical) {
            crate::domains::medical_semantics::case_for_medical_document_render(
                case,
                &configured.spec.role_id,
            )
        } else {
            case.clone()
        };
        let result = render_text_template(&configured.template_text, &render_case, true);
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

        let blocks = required_blocks_for(&configured.spec, &configured.template_text);
        let unmet = unmet_blocks(&blocks, &render_case, &result.output_text);
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
    use crate::domains::medical_document_plan::{build_medical_render_plan, MedicalDocumentRole};
    use crate::domains::medical_semantics::*;
    use crate::{SemanticValue, ValueSource};
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
        for (field_id, value_text) in pairs {
            let mut semantic_value = value(value_text);
            semantic_value.field_id = (*field_id).to_string();
            values.insert((*field_id).to_string(), semantic_value);
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
                required_fields: placeholders
                    .iter()
                    .map(|field| (*field).to_string())
                    .collect(),
                placeholders: placeholders
                    .iter()
                    .map(|field| (*field).to_string())
                    .collect(),
                is_static_copy: false,
                popup_fields: Vec::new(),
                popup_configured: false,
            },
            template_text: template.into(),
        }
    }

    fn medical_doc(
        id: &str,
        label: &str,
        role_id: &str,
        template: &str,
        required_fields: Vec<String>,
    ) -> ConfiguredDocument {
        ConfiguredDocument {
            spec: DocumentTemplateSpec {
                id: id.into(),
                button_label: label.into(),
                template_path: format!("templates/{id}.docx"),
                category: DomainKind::Medical,
                role_id: role_id.into(),
                required_fields,
                placeholders: Vec::new(),
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
            &WorkflowFlags::default(),
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
                assert!(outputs
                    .iter()
                    .all(|output| !output.rendered_text.contains("{{")));
                assert!(outputs
                    .iter()
                    .any(|output| output.rendered_text.contains("ООО Ромашка")));
                assert_eq!(source_target_name, "Первичный.docx");
                assert!(patient_folder_name.contains("Иванов"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn missing_folder_naming_value_blocks_zero_touch_before_publication() {
        let case = case_with(&[("document.number", "42")]);
        let docs = vec![doc("d1", "Справка", "Готовый текст", &[])];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "Источник",
            "Источник.docx",
        );
        match batch {
            CreatedDocumentsBatch::Attention { missing, .. } => {
                assert!(missing.iter().any(|item| item.contains("Папка результата")));
                assert!(missing.iter().any(|item| item.contains("Имя")));
            }
            other => panic!("expected attention, got {other:?}"),
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
            &WorkflowFlags::default(),
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
                assert!(missing.iter().any(|item| item.contains("Договор")));
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
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        assert!(matches!(batch, CreatedDocumentsBatch::Attention { .. }));
    }

    #[test]
    fn medical_document_without_required_block_is_refused() {
        let case = case_with(&[("subject.name", "Иванов Иван")]);
        let plan = build_medical_render_plan(MedicalDocumentRole::DischargeEpicrisis, false, false);
        let docs = vec![medical_doc(
            "d1",
            "Выписной эпикриз",
            "discharge",
            "Пациент {{subject.name}}",
            plan.required_fields,
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        match batch {
            CreatedDocumentsBatch::Attention { missing, .. } => {
                assert!(missing.iter().any(|item| item.contains("Диагноз")));
                assert!(missing
                    .iter()
                    .any(|item| item.contains("Подпись врача-психиатра")));
                assert!(missing
                    .iter()
                    .any(|item| item.contains("Подпись заведующего отделением")));
            }
            other => panic!("expected Attention, got {other:?}"),
        }
    }

    #[test]
    fn medical_document_with_all_blocks_is_created() {
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "12345"),
            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
            ("medical.workplace", "ООО Ромашка"),
            ("medical.position", "инженер"),
        ]);
        let plan = build_medical_render_plan(MedicalDocumentRole::DischargeEpicrisis, false, false);
        let docs = vec![medical_doc(
            "d1",
            "Выписной эпикриз",
            "discharge",
            concat!(
                "Пациент {{subject.name}}\n",
                "История болезни {{medical.case_number}}\n",
                "Диагноз {{medical.diagnosis}}\n",
                "Лечение {{medical.treatment}}\n",
                "Дата поступления {{medical.admission_date}}\n",
                "Дата выписки {{medical.discharge_date}}\n",
                "Экспертный анамнез {{medical.expert_anamnesis}}\n",
                "Зав. отд. Петров П.П.\n",
                "Врач-психиатр Иванов И.И."
            ),
            plan.required_fields,
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        assert!(matches!(batch, CreatedDocumentsBatch::Ready { .. }));
    }

    #[test]
    fn legacy_derived_expert_field_renders_from_current_run_sources() {
        let mut case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "12345"),
            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
            ("medical.workplace", "ООО Ромашка"),
            ("medical.position", "инженер"),
            ("medical.sick_leave_number", "ЛН-77"),
        ]);
        set_medical_sick_leave_choice(&mut case, true);
        let docs = vec![medical_doc(
            "legacy-discharge",
            "Выписной эпикриз",
            "discharge",
            concat!(
                "Пациент {{subject.name}}\n",
                "История болезни {{medical.case_number}}\n",
                "Диагноз {{medical.diagnosis}}\n",
                "Лечение {{medical.treatment}}\n",
                "Дата поступления {{medical.admission_date}}\n",
                "Дата выписки {{medical.discharge_date}}\n",
                "Экспертный анамнез {{medical.expert_anamnesis}}\n",
                "Зав. отд. Петров П.П.\n",
                "Врач-психиатр Иванов И.И."
            ),
            vec![MEDICAL_EXPERT_ANAMNESIS.into()],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags {
                sick_leave_enabled: true,
            },
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        let CreatedDocumentsBatch::Ready { outputs, .. } = batch else {
            panic!("expected the legacy derived field to render from source facts, got {batch:?}");
        };
        assert_eq!(outputs.len(), 1);
        let text = &outputs[0].rendered_text;
        assert!(text.contains("Работает в ООО Ромашка, в должности инженер."));
        assert!(text.contains("Больничный лист № ЛН-77."));
        assert!(text.contains("Срок лечения с 01.06.2026 по 12.06.2026, 12 дней."));
        assert!(!text.contains("{{medical.expert_anamnesis}}"));
    }

    #[test]
    fn mse_and_sick_leave_vk_render_their_own_protocols_in_one_batch() {
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "777"),
            ("medical.admission_date", "10.06.2026"),
            ("medical.diagnosis", "I10"),
            ("medical.treatment", "Терапия"),
            (VK_MSE_COMMISSION_DATE, "13.06.2026"),
            (VK_MSE_PROTOCOL_NUMBER, "MSE-42"),
            (VK_MSE_PROTOCOL_DATE, "14.06.2026"),
            ("medical.workplace", "Общая организация"),
            ("medical.position", "Общая должность"),
            (SICK_LEAVE_VK_COMMISSION_DATE, "15.06.2026"),
            (SICK_LEAVE_VK_PROTOCOL_NUMBER, "BL-99"),
            (SICK_LEAVE_VK_PROTOCOL_DATE, "16.06.2026"),
            ("medical.sick_leave_commission_date", "17.06.2026"),
        ]);
        let common = concat!(
            "{{subject.name}} | {{medical.case_number}} | {{medical.admission_date}} | ",
            "{{medical.diagnosis}} | {{medical.treatment}} | "
        );
        let signatures = "\nЗав. отд. Петров П.П.\nЛечащий врач Иванов И.И.";
        let mse_template = format!(
            "{common}{{{{medical.commission_date}}}} | {{{{medical.protocol_number}}}} | \
             {{{{medical.protocol_date}}}} | {{{{medical.workplace}}}} | {{{{medical.position}}}}{signatures}"
        );
        let sick_template = format!(
            "{common}{{{{medical.commission_date}}}} | {{{{medical.protocol_number}}}} | \
             {{{{medical.protocol_date}}}} | {{{{medical.workplace}}}} | {{{{medical.position}}}} | \
             {{{{medical.sick_leave_commission_date}}}}{signatures}"
        );
        let docs = vec![
            medical_doc(
                "mse",
                "ВК на МСЭ",
                "vk_mse",
                &mse_template,
                build_medical_render_plan(MedicalDocumentRole::VkMse, false, false).required_fields,
            ),
            medical_doc(
                "sick",
                "ВК больничный",
                "sick_leave_vk",
                &sick_template,
                build_medical_render_plan(MedicalDocumentRole::SickLeaveCommission, false, false)
                    .required_fields,
            ),
        ];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "Первичный",
            "Первичный.docx",
        );
        let CreatedDocumentsBatch::Ready { outputs, .. } = batch else {
            panic!("expected both role-scoped documents to be ready");
        };
        assert_eq!(outputs.len(), 2);
        let mse = outputs
            .iter()
            .find(|output| output.document_id == "mse")
            .unwrap();
        let sick = outputs
            .iter()
            .find(|output| output.document_id == "sick")
            .unwrap();
        assert!(mse.rendered_text.contains("MSE-42"));
        assert!(mse.rendered_text.contains("Общая организация"));
        assert!(mse.rendered_text.contains("Общая должность"));
        assert!(!mse.rendered_text.contains("BL-99"));
        assert!(sick.rendered_text.contains("BL-99"));
        assert!(sick.rendered_text.contains("Общая организация"));
        assert!(sick.rendered_text.contains("Общая должность"));
        assert!(!sick.rendered_text.contains("MSE-42"));
    }

    #[test]
    fn angle_brackets_and_shift_operators_are_not_placeholders() {
        let case = case_with(&[
            ("subject.name", "Тестовый субъект"),
            ("org.name", "ООО Ромашка"),
        ]);
        let docs = vec![doc(
            "d1",
            "Технический отчёт",
            "Компания <<Ромашка>>; проверка: a << b и c >> d; {{org.name}}",
            &["org.name"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        match batch {
            CreatedDocumentsBatch::Ready { outputs, .. } => {
                assert!(outputs[0].rendered_text.contains("<<Ромашка>>"));
                assert!(outputs[0].rendered_text.contains("a << b"));
                assert!(outputs[0].rendered_text.contains("c >> d"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn legitimate_double_braces_in_inserted_value_do_not_block_zero_touch() {
        let case = case_with(&[
            ("subject.name", "Тестовый субъект"),
            ("custom.code", "const example = {{ nested_template }};"),
        ]);
        let docs = vec![doc(
            "d1",
            "Технический отчёт",
            "Фрагмент: {{custom.code}}",
            &["custom.code"],
        )];
        let batch = plan_created_documents_batch(
            &case,
            &docs,
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        match batch {
            CreatedDocumentsBatch::Ready { outputs, .. } => {
                assert!(outputs[0].rendered_text.contains("{{ nested_template }}"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn unknown_placeholder_left_in_text_is_refused() {
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
            &WorkflowFlags::default(),
            &[FolderNamePart::FullSubjectName],
            "src",
            "src.docx",
        );
        assert!(matches!(batch, CreatedDocumentsBatch::Attention { .. }));
    }
}
