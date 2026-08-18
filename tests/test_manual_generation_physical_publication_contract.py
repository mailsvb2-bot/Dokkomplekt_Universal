from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")


def body() -> str:
    start = COMMANDS.index("fn render_docx_batch(")
    end = COMMANDS.index("#[derive(Debug, Deserialize)]\nstruct ScannerRequest", start)
    return COMMANDS[start:end]


def test_visible_output_root_is_not_created_before_success():
    batch = body()
    assert "let stage_parent = output_root" in batch
    assert "let stage = stage_parent.join(format!(" in batch
    assert "std::fs::create_dir_all(&output_root)" not in batch


def test_one_physical_file_is_required_for_every_requested_document():
    batch = body()
    assert "if staged_paths.len() != documents.len()" in batch
    assert "if created_files.len() != documents.len()" in batch
    assert "std::fs::metadata(path)" in batch
    assert "extract_docx_text(path)" in batch
    assert "КРИТИЧЕСКАЯ ОШИБКА публикации" in batch


def test_trust_report_is_ancillary_and_cannot_delete_docx():
    batch = body()
    report_start = batch.index("if privacy.write_trust_report")
    report_end = batch.index("Ok(paths)", report_start)
    report = batch[report_start:report_end]
    assert "write_trust_report(" in report
    assert "render_warnings.push" in report
    assert "Для проверяемого отчёта сначала загрузите файл" not in report


def test_success_opens_exact_verified_publication_folder():
    batch = body()
    assert batch.index("extract_docx_text(path)") < batch.index("open_in_file_manager(")
    assert "path: output_folder.display().to_string()" in batch
