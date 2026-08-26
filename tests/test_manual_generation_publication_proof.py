from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_manual_batch_never_creates_visible_output_root_before_publication() -> None:
    source = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
    start = source.index("fn render_docx_batch(")
    end = source.index("struct ScannerRequest", start)
    batch = source[start:end]
    assert "std::fs::create_dir_all(&output_root)" not in batch
    assert "let stage_parent = output_root" in batch
    assert "let stage = stage_parent.join" in batch


def test_manual_batch_proves_final_docx_files_instead_of_fabricating_paths() -> None:
    source = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
    start = source.index("fn render_docx_batch(")
    end = source.index("struct ScannerRequest", start)
    batch = source[start:end]
    proof = (ROOT / "src-tauri/src/subsystems/publication_collision.rs").read_text(encoding="utf-8")
    assert "verify_published_batch_files" in source
    assert "published_path.is_file()" in proof
    assert "metadata.len() == 0" in proof
    assert "extract_docx_text(&published_path)" in proof
    assert ".filter_map(|path| path.file_name())" not in batch[batch.index("finalize_published_generation"): ]


def test_trust_report_is_ancillary_to_primary_docx_publication() -> None:
    source = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
    start = source.index("fn render_docx_batch(")
    end = source.index("struct ScannerRequest", start)
    batch = source[start:end]
    trust = batch[batch.index("if privacy.write_trust_report"):batch.index("Ok(paths)")]
    assert "write_trust_report(" in trust
    assert "ancillary_warnings.push" in trust
    assert "Для проверяемого отчёта сначала загрузите файл" not in trust


def test_desktop_publication_acceptance_creates_patient_subfolder_with_multiple_docx() -> None:
    source = (ROOT / "src-tauri/src/subsystems/publication_collision.rs").read_text(encoding="utf-8")
    assert "publishing_batch_creates_desktop_root_patient_subfolder_and_all_real_docx" in source
    assert 'canonical_default_output_root_under(&desktop)' in source
    assert 'FullSubjectName' in source
    assert 'AdmissionAndDischargeDates' in source
    assert 'Первичный осмотр.docx' in source
    assert 'Выписной эпикриз.docx' in source
    assert 'verify_published_batch_files(&published, &staged, 2)' in source
    assert 'extract_docx_text(&expected_primary)' in source
    assert 'extract_docx_text(&expected_discharge)' in source
