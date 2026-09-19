use dokkomplekt_core::{SemanticAtom, SemanticCase, SemanticRecord};
use dokkomplekt_docx::{
    create_docx_from_text_with_centered_exact_lines, extract_docx_text, render_docx_file,
};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;
use std::time::{SystemTime, UNIX_EPOCH};

const TEMPLATE: &str = dokkomplekt_core::MEDICAL_PROGRAM_CALENDAR_DIARY_TEMPLATE_TEXT;

fn unique_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dokkomplekt-diary-docx-{}-{nonce}",
        std::process::id()
    ))
}

fn read_document_xml(path: &Path) -> String {
    let file = File::open(path).expect("docx");
    let mut archive = ZipArchive::new(file).expect("zip");
    let mut entry = archive.by_name("word/document.xml").expect("document.xml");
    let mut xml = String::new();
    entry.read_to_string(&mut xml).expect("read document.xml");
    xml
}

fn diary_row(datetime: &str, text: Option<&str>, is_final: bool) -> SemanticRecord {
    let mut row = SemanticRecord::new();
    row.insert("datetime".into(), SemanticAtom::Text(datetime.into()));
    row.insert("is_final".into(), SemanticAtom::Boolean(is_final));
    if let Some(text) = text {
        row.insert("text".into(), SemanticAtom::Text(text.into()));
    }
    row.insert(
        "treating_physician_signature".into(),
        SemanticAtom::Text("Лечащий врач __________________ /____________/".into()),
    );
    row.insert(
        "department_head_signature".into(),
        SemanticAtom::Text("Заведующий отделением __________ /____________/".into()),
    );
    row
}

#[test]
fn program_calendar_template_becomes_a_real_text_diary_docx() {
    let root = unique_dir();
    std::fs::create_dir_all(&root).expect("temp dir");
    let template = root.join("program-calendar.docx");
    let output = root.join("patient-diaries.docx");

    create_docx_from_text_with_centered_exact_lines(&template, TEMPLATE, &["{{diary.datetime}}"])
        .expect("program template");

    let mut case = SemanticCase::default();
    case.set_collection(
        "diaries",
        vec![
            diary_row(
                "11.05.2026",
                Some("Профессиональный текст дневника."),
                false,
            ),
            diary_row("12.05.2026", None, true),
        ],
    );

    let result = render_docx_file(&template, &output, &case, true).expect("strict render");
    assert!(
        result.missing_fields.is_empty(),
        "{:?}",
        result.missing_fields
    );
    assert!(
        result.unknown_fields.is_empty(),
        "{:?}",
        result.unknown_fields
    );

    let text = extract_docx_text(&output).expect("rendered text");
    assert!(text.contains("11.05.2026"), "{text}");
    assert!(text.contains("Профессиональный текст дневника."), "{text}");
    assert!(text.contains("12.05.2026"), "{text}");
    assert!(text.contains("Состояние улучшилось."), "{text}");
    assert!(
        text.contains("На текущую дату оформлена выписка из стационара."),
        "{text}"
    );
    assert_eq!(text.matches("Лечащий врач").count(), 2, "{text}");
    assert_eq!(text.matches("Заведующий отделением").count(), 2, "{text}");
    assert!(!text.contains("{{"), "{text}");

    let xml = read_document_xml(&output);
    assert!(!xml.contains("<w:tbl"), "canonical diary output must remain paragraph text");
    for date in ["11.05.2026", "12.05.2026"] {
        let date_pos = xml.find(date).expect("rendered diary date");
        let paragraph_start = xml[..date_pos].rfind("<w:p").expect("date paragraph start");
        let paragraph_end = xml[date_pos..]
            .find("</w:p>")
            .map(|offset| date_pos + offset + "</w:p>".len())
            .expect("date paragraph end");
        let paragraph = &xml[paragraph_start..paragraph_end];
        assert!(
            paragraph.contains("<w:jc w:val=\"center\"/>"),
            "diary date must remain centered: {paragraph}"
        );
    }

    let _ = std::fs::remove_dir_all(root);
}
