use dokkomplekt_core::{set_user_value, SemanticCase};
use dokkomplekt_docx::{create_docx_from_text, extract_docx_text, render_docx_file};

#[test]
fn real_docx_renderer_accepts_fields_from_multiple_domains() {
    let cases = [
        ("document.number", "L1"),
        ("employee.name", "Employee"),
        ("amount.total", "125000"),
        ("education.student_name", "Student"),
        ("custom.project", "Project"),
    ];
    let root = std::env::temp_dir().join(format!("dokkomplekt-docx-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create temp directory");

    for (index, (field_id, value)) in cases.into_iter().enumerate() {
        let template = format!("{{{{{field_id}}}}}");
        let template_path = root.join(format!("template-{index}.docx"));
        let output_path = root.join(format!("output-{index}.docx"));
        create_docx_from_text(&template_path, &template).expect("create DOCX template");

        let mut case = SemanticCase::default();
        set_user_value(&mut case, field_id, value);
        let result =
            render_docx_file(&template_path, &output_path, &case, true).expect("render DOCX");
        assert!(result.missing_fields.is_empty());
        assert!(result.unknown_fields.is_empty());
        assert!(result.template_errors.is_empty());

        let rendered = extract_docx_text(&output_path).expect("read rendered DOCX");
        assert!(rendered.contains(value));
        assert!(!rendered.contains("{{"));
    }

    let _ = std::fs::remove_dir_all(root);
}
