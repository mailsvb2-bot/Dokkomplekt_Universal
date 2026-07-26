use crate::output_naming::{build_output_folder_name, sanitize_folder_name};
use crate::{FolderNamePart, SemanticCase};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputPlan {
    pub root_folder: PathBuf,
    pub patient_folder: PathBuf,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
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
    OutputPlan {
        root_folder: root.to_path_buf(),
        patient_folder,
        files,
        warnings: vec![],
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
