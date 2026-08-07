from pathlib import Path

main_path = Path('src-tauri/src/main.rs')
main = main_path.read_text(encoding='utf-8')
old = '''fn template_text_for_document(
    app: &tauri::AppHandle,
    document: &DocumentTemplateSpec,
) -> Result<String, String> {
    extract_docx_text(&resolve_user_path(app, &document.template_path)?).map_err(|e| e.to_string())
}

'''
if main.count(old) != 1:
    raise SystemExit(f'expected one obsolete template_text_for_document helper, found {main.count(old)}')
main = main.replace(old, '', 1)
main_path.write_text(main, encoding='utf-8')
