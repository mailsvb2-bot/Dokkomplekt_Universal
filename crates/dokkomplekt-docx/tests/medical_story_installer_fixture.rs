use dokkomplekt_core::{
    analyze_template_text_with_domain_hint, missing_medical_template_render_paths,
    DocumentTemplateSpec, DomainKind,
};
use dokkomplekt_docx::{
    compile_labeled_template_file, extract_docx_story_texts, extract_docx_text,
    insert_text_paragraph_before_first_matching_file,
};
use std::fs::File;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn write_fixture(path: &std::path::Path) {
    let file = File::create(path).expect("fixture");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Первичный осмотр</w:t></w:r></w:p>
<w:p><w:r><w:t>Ф.И.О.: Иванов Иван Иванович</w:t></w:r></w:p>
<w:p><w:r><w:t>Номер истории болезни: 1111</w:t></w:r></w:p>
<w:p><w:r><w:t>Дата поступления: 20.08.2026</w:t></w:r></w:p>
<w:p><w:r><w:t>Диагноз: F20.0 шаблонная формулировка</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечение: старое лечение</w:t></w:r></w:p>
<w:p><w:r><w:t>Место работы: Старый завод</w:t></w:r></w:p>
<w:p><w:r><w:t>Должность: старый инженер</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечащий врач __________</w:t></w:r></w:p>
<w:p><w:r><w:t>Заведующий отделением __________</w:t></w:r></w:p>
</w:body></w:document>"#;
    for (name, data) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", body),
        (
            "word/header1.xml",
            r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>ГБУЗ НО «НКЦПЗ» диспансер №2</w:t></w:r></w:p></w:hdr>"#,
        ),
    ] {
        zip.start_file(name, options).expect("part");
        zip.write_all(data.as_bytes()).expect("part bytes");
    }
    zip.finish().expect("finish fixture");
}

fn write_tabular_primary_fixture(path: &std::path::Path) {
    let file = File::create(path).expect("fixture");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>20.08.2026 Первичный осмотр</w:t></w:r></w:p>
<w:p><w:r><w:t>Ф.И.О.: Иванов Иван Иванович</w:t></w:r></w:p>
<w:p><w:r><w:t>Дата поступления: 20.08.2026</w:t></w:r></w:p>
<w:tbl>
<w:tr><w:tc><w:p><w:r><w:t>История болезни №</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1111</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Диагноз</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>F20.0 шаблонная формулировка</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>План лечения</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>старое лечение</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Место работы</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Старый завод</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Должность</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>старый инженер</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>Лечащий врач __________</w:t></w:r></w:p>
<w:p><w:r><w:t>Заведующий отделением __________</w:t></w:r></w:p>
</w:body></w:document>"#;
    for (name, data) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", body),
    ] {
        zip.start_file(name, options).expect("part");
        zip.write_all(data.as_bytes()).expect("part bytes");
    }
    zip.finish().expect("finish fixture");
}

#[test]
fn semantic_paragraph_insertion_stays_in_body_and_precedes_signature() {
    let root = std::env::temp_dir().join(format!(
        "dok-medical-semantic-insert-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let input = root.join("filled.docx");
    let output = root.join("with-expert.docx");
    write_fixture(&input);

    let inserted = insert_text_paragraph_before_first_matching_file(
        &input,
        &output,
        &["Лечащий врач", "Заведующий отделением"],
        "Экспертный анамнез: {{medical.expert_anamnesis}}",
    )
    .expect("insert semantic paragraph");
    assert!(inserted);
    let stories = extract_docx_story_texts(&output).expect("stories");
    let body = &stories["word/document.xml"];
    assert!(
        body.find("Экспертный анамнез: {{medical.expert_anamnesis}}")
            < body.find("Лечащий врач __________")
    );
    assert!(!stories["word/header1.xml"].contains("Экспертный анамнез"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn installed_medical_fixture_compiles_required_primary_fields_without_touching_header() {
    let root = std::env::temp_dir().join(format!("dok-medical-installer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let input = root.join("filled.docx");
    let output = root.join("compiled.docx");
    write_fixture(&input);

    let report = compile_labeled_template_file(&input, &output, &DomainKind::Medical, "primary")
        .expect("compile filled primary template");
    assert!(report.binding_count >= 7, "{report:?}");

    let text = extract_docx_text(&output).expect("compiled text");
    let analysis = analyze_template_text_with_domain_hint(&text, Some(&DomainKind::Medical));
    let document = DocumentTemplateSpec {
        id: "primary-smoke".into(),
        button_label: "первичный smoke".into(),
        template_path: output.display().to_string(),
        category: DomainKind::Medical,
        role_id: "primary".into(),
        required_fields: Vec::new(),
        placeholders: analysis.placeholders,
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    };
    assert_eq!(
        missing_medical_template_render_paths(&document),
        Vec::<String>::new()
    );

    let stories = extract_docx_story_texts(&output).expect("stories");
    assert!(stories["word/header1.xml"].contains("НКЦПЗ"));
    assert!(!stories["word/header1.xml"].contains("{{"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tabular_primary_fixture_compiles_the_five_render_paths_from_real_word_cells() {
    let root = std::env::temp_dir().join(format!(
        "dok-medical-tabular-primary-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let input = root.join("первичный.docx");
    let output = root.join("compiled.docx");
    write_tabular_primary_fixture(&input);

    let report = compile_labeled_template_file(&input, &output, &DomainKind::Medical, "primary")
        .expect("compile tabular primary template");
    for field in [
        "medical.case_number",
        "medical.diagnosis",
        "medical.treatment",
        "medical.workplace",
        "medical.position",
    ] {
        assert!(
            report
                .applied_field_ids
                .iter()
                .any(|candidate| candidate == field),
            "missing compiled field {field}: {report:?}"
        );
    }

    let text = extract_docx_text(&output).expect("compiled text");
    let analysis = analyze_template_text_with_domain_hint(&text, Some(&DomainKind::Medical));
    let document = DocumentTemplateSpec {
        id: "primary-tabular".into(),
        button_label: "первичный".into(),
        template_path: output.display().to_string(),
        category: DomainKind::Medical,
        role_id: "primary".into(),
        required_fields: Vec::new(),
        placeholders: analysis.placeholders,
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    };
    assert_eq!(
        missing_medical_template_render_paths(&document),
        Vec::<String>::new()
    );
    assert!(!text.contains("Старый завод"));
    assert!(!text.contains("старый инженер"));
    assert!(!text.contains("старое лечение"));
    let _ = std::fs::remove_dir_all(root);
}
