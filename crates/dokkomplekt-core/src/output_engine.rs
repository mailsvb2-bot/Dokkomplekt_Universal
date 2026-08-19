use crate::output_naming::{build_output_folder_name, sanitize_folder_name};
use crate::{
    missing_output_folder_fields, popup_config_for_field, DocumentTemplateSpec, DomainKind,
    FolderNamePart, SemanticCase,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputPlan {
    pub root_folder: PathBuf,
    pub patient_folder: PathBuf,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub exists: bool,
}

pub fn output_folder_requirement_document(
    case: &SemanticCase,
    folder_parts: &[FolderNamePart],
) -> Option<DocumentTemplateSpec> {
    let missing = missing_output_folder_fields(case, folder_parts);
    if missing.is_empty() {
        return None;
    }
    let mut popup_fields = missing
        .iter()
        .map(|field_id| {
            let mut config =
                popup_config_for_field(field_id, true, &DomainKind::Generic, "output_folder");
            config.section = Some("Папка результата".into());
            config.help_text = Some(
                "Значение нужно только потому, что вы выбрали его частью имени итоговой папки."
                    .into(),
            );
            config
        })
        .collect::<Vec<_>>();
    popup_fields.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then(left.field_id.cmp(&right.field_id))
    });
    Some(DocumentTemplateSpec {
        id: "__output_folder__".into(),
        button_label: "Папка результата".into(),
        template_path: String::new(),
        category: DomainKind::Generic,
        role_id: "output_folder".into(),
        required_fields: missing.clone(),
        placeholders: missing,
        is_static_copy: false,
        popup_fields,
        popup_configured: true,
    })
}

pub fn plan_output_paths(
    root: &Path,
    case: &SemanticCase,
    folder_parts: &[FolderNamePart],
    button_labels: &[String],
) -> OutputPlan {
    let folder_name = build_output_folder_name(case, folder_parts);
    let patient_folder = root.join(sanitize_path_component(&folder_name));
    let files = button_labels
        .iter()
        .map(|label| patient_folder.join(format!("{}.docx", sanitize_path_component(label))))
        .collect();
    let exists = patient_folder.exists();
    OutputPlan {
        root_folder: root.to_path_buf(),
        patient_folder,
        files,
        warnings: vec![],
        exists,
    }
}

pub fn sanitize_path_component(value: &str) -> String {
    let sanitized = sanitize_folder_name(value);
    if sanitized.is_empty() {
        "Документы".into()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set_user_value;
    #[test]
    fn sanitizer_matches_display_name_and_windows_rules() {
        assert_eq!(sanitize_path_component("CON."), "_CON");
        assert_eq!(sanitize_path_component("NUL"), "_NUL");
        assert_eq!(sanitize_path_component("COM1.txt"), "_COM1.txt");
        assert_eq!(sanitize_path_component("Отчёт..."), "Отчёт");
    }

    #[test]
    fn selected_folder_parts_become_normal_workflow_requirements() {
        let case = SemanticCase::default();
        let doc = output_folder_requirement_document(
            &case,
            &[
                FolderNamePart::FullSubjectName,
                FolderNamePart::DocumentNumber,
            ],
        )
        .unwrap();
        assert_eq!(doc.category, DomainKind::Generic);
        assert!(doc.required_fields.contains(&"subject.name".to_string()));
        assert!(doc.required_fields.contains(&"document.number".to_string()));
        assert!(doc.popup_fields.iter().all(|field| {
            field.required && field.section.as_deref() == Some("Папка результата")
        }));
        let plan = crate::plan_workflow(&doc, &case, &crate::WorkflowFlags::default());
        assert_eq!(
            plan.prompts
                .iter()
                .map(|prompt| prompt.field_id.as_str())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from(["document.number", "subject.name"]),
        );
    }

    #[test]
    fn output_plan_reports_existing_target_without_mutating_it() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-output-plan-existing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);

        let mut case = SemanticCase::default();
        set_user_value(&mut case, "subject.name", "Иванов Иван Иванович");
        let folder_parts = [FolderNamePart::FullSubjectName];
        let expected_folder = root.join("Иванов Иван Иванович");
        std::fs::create_dir_all(&expected_folder).unwrap();
        let sentinel = expected_folder.join("existing.txt");
        std::fs::write(&sentinel, "keep").unwrap();

        let plan = plan_output_paths(&root, &case, &folder_parts, &[]);

        assert!(plan.exists);
        assert_eq!(plan.patient_folder, expected_folder);
        assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn planned_paths_do_not_use_service_files_or_underscores() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "subject.name", "Иванов Иван Иванович");
        let plan = plan_output_paths(
            Path::new("C:/Desktop/Выписанные пациенты"),
            &case,
            &[FolderNamePart::FullSubjectName],
            &["Выписной эпикриз".into()],
        );
        let text = plan.patient_folder.to_string_lossy();
        assert!(text.contains("Иванов Иван Иванович"));
        assert!(!text.contains("_medical_autofill_history"));
        assert!(!text.contains('_'));
    }
}
