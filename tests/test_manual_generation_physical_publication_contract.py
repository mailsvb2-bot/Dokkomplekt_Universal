from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
PUBLICATION = (ROOT / "src-tauri/src/manual_publication.rs").read_text(encoding="utf-8")
MAIN = (ROOT / "src-tauri/src/main.rs").read_text(encoding="utf-8")


def body() -> str:
    start = COMMANDS.index("fn render_docx_batch(")
    end = COMMANDS.index("#[derive(Debug, Deserialize)]\nstruct ScannerRequest", start)
    return COMMANDS[start:end]


def test_visible_output_root_is_not_created_before_success():
    batch = body()
    assert "manual_publication::prepare_stage_parent(&output_root)" in batch
    assert "let stage = stage_parent.join(format!(" in batch
    assert "std::fs::create_dir_all(&output_root)" not in batch
    assert "mod manual_publication;" in MAIN


def test_one_physical_file_is_required_for_every_requested_document():
    batch = body()
    assert "manual_publication::verify_staged_docx(&staged_paths, documents.len())" in batch
    assert "manual_publication::verify_published_docx(" in batch
    assert "staged_paths.len() != expected_count" in PUBLICATION
    assert "created_files.len() != expected_count" in PUBLICATION
    assert "std::fs::metadata(path)" in PUBLICATION
    assert "extract_docx_text(path)" in PUBLICATION
    assert "КРИТИЧЕСКАЯ ОШИБКА публикации" in PUBLICATION


def test_trust_report_is_ancillary_and_cannot_delete_docx():
    batch = body()
    assert "manual_publication::optional_trust_report_warning(" in batch
    assert "write_trust_report(" not in batch
    assert "crate::write_trust_report(" in PUBLICATION
    assert "Для проверяемого отчёта сначала загрузите файл" not in batch


def test_success_opens_exact_verified_publication_folder():
    batch = body()
    assert batch.index("manual_publication::verify_published_docx(") < batch.index("open_in_file_manager(")
    assert "manual_publication::path_strings(&created_files)" in batch
    assert "path: output_folder.display().to_string()" in batch
