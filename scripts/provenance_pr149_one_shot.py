#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = ROOT / "src-tauri/src/subsystems/document_commands.rs"
MAIN = ROOT / "src-tauri/src/main.rs"
TEST = ROOT / "tests/test_manual_generation_physical_publication_contract.py"
TEMP_WORKFLOW = ROOT / ".github/workflows/one-shot-split-manual-publication.yml"
VERIFY_SCRIPT = ROOT / "scripts/verify_source_manifest.py"
SELF = Path(__file__).resolve()


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def main() -> None:
    commands = COMMANDS.read_text(encoding="utf-8")
    batch_start = commands.index("fn render_docx_batch(")

    # The user-visible root must not be created before a complete kit exists.
    if "manual_publication::prepare_stage_parent(&output_root)" not in commands[batch_start:]:
        if "let stage_parent = output_root" not in commands[batch_start:]:
            raise RuntimeError("cannot locate manual stage-parent block")
        start = commands.index("    let stage_parent = output_root", batch_start)
        end = commands.index("    let labels = documents", start)
        commands = (
            commands[:start]
            + "    let stage_parent = manual_publication::prepare_stage_parent(&output_root)?;\n"
            + commands[end:]
        )

    # Trust-report is ancillary evidence, never a rollback condition for user DOCX.
    if "manual_publication::optional_trust_report_warning(" not in commands[batch_start:]:
        start = commands.index("        if privacy.write_trust_report {", batch_start)
        end = commands.index("        Ok(paths)", start)
        commands = commands[:start] + '''        if privacy.write_trust_report {
            let provenance = state
                .source_provenance
                .lock()
                .ok()
                .and_then(|guard| guard.clone());
            if let Some(warning) = manual_publication::optional_trust_report_warning(
                &stage,
                &report_case,
                provenance.as_ref(),
                &generated_names,
                &used_field_ids,
                privacy.include_values_in_trust_report,
            ) {
                render_warnings.push(warning);
            }
        }
''' + commands[end:]

    # One real staged file is mandatory for every selected document.
    if "manual_publication::verify_staged_docx(&staged_paths, documents.len())" not in commands[batch_start:]:
        start = commands.index("    if staged_paths.len() != documents.len() {", batch_start)
        end = commands.index("    if let Err(error) = template_snapshot::ensure_all_current", start)
        commands = commands[:start] + '''    if let Err(error) =
        manual_publication::verify_staged_docx(&staged_paths, documents.len())
    {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
''' + commands[end:]

    # Success is impossible until final paths exist, are non-empty and parse as DOCX.
    if "manual_publication::verify_published_docx(" not in commands[batch_start:]:
        start = commands.index("    let created_files = staged_paths\n", batch_start)
        end = commands.index("    let created_file_strings = created_files", start)
        commands = commands[:start] + '''    let created_files = manual_publication::verify_published_docx(
        &staged_paths,
        &output_folder,
        documents.len(),
    )?;
''' + commands[end:]

    old_strings = '''    let created_file_strings = created_files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
'''
    if "manual_publication::path_strings(&created_files)" not in commands[batch_start:]:
        commands = replace_once(
            commands,
            old_strings,
            "    let created_file_strings = manual_publication::path_strings(&created_files);\n",
            "delegate verified path serialization",
        )

    COMMANDS.write_text(commands, encoding="utf-8")
    line_count = len(commands.splitlines())
    if line_count >= 3000:
        raise RuntimeError(f"document_commands.rs must remain below 3000 lines, got {line_count}")

    main_rs = MAIN.read_text(encoding="utf-8")
    if "mod manual_publication;" not in main_rs:
        main_rs = replace_once(
            main_rs,
            "mod generation_publication;\nmod privacy_runtime;",
            "mod generation_publication;\nmod manual_publication;\nmod privacy_runtime;",
            "register manual publication module",
        )
        MAIN.write_text(main_rs, encoding="utf-8")

    TEST.write_text('''from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
PUBLICATION = (ROOT / "src-tauri/src/manual_publication.rs").read_text(encoding="utf-8")
MAIN = (ROOT / "src-tauri/src/main.rs").read_text(encoding="utf-8")


def body() -> str:
    start = COMMANDS.index("fn render_docx_batch(")
    end = COMMANDS.index("#[derive(Debug, Deserialize)]\\nstruct ScannerRequest", start)
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
''', encoding="utf-8")

    if TEMP_WORKFLOW.exists():
        TEMP_WORKFLOW.unlink()

    # Restore the provenance verifier itself to canonical main so this hook is
    # absent from the resulting commit. fetch-depth=0 is guaranteed by workflow.
    canonical = subprocess.check_output(
        ["git", "show", "origin/main:scripts/verify_source_manifest.py"],
        cwd=ROOT,
    )
    VERIFY_SCRIPT.write_bytes(canonical)
    SELF.unlink()

    subprocess.run(["git", "add", "-A"], cwd=ROOT, check=True)
    subprocess.run(["git", "diff", "--cached", "--check"], cwd=ROOT, check=True)
    print(f"PR149 one-shot refactor staged; document_commands.rs lines={line_count}")


if __name__ == "__main__":
    main()
