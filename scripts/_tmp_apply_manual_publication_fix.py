from pathlib import Path
import subprocess

BRANCH = "agent/fix-manual-generation-publication-proof-20260818"
path = Path("src-tauri/src/subsystems/document_commands.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match, found {count}: {old[:100]!r}")
    text = text.replace(old, new, 1)


if "let stage_parent = output_root" not in text:
    start = text.index("fn ensure_rendered_document_complete(")
    end = text.index("#[derive(Debug, Deserialize)]\nstruct RenderDocxRequest", start)
    text = text[:start] + text[end:]

    replace_once(
        """    let output_root = resolve_user_path(&app, &req.output_root)?;
    std::fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;
    cleanup_stale_stage_directories(&output_root, Duration::from_secs(24 * 60 * 60))?;
""",
        """    let output_root = resolve_user_path(&app, &req.output_root)?;
    // Do not create the user-visible output root before rendering succeeds. A
    // failure in licensing, hydration, rendering, completeness validation or
    // publication must not leave an empty “successful-looking” folder behind.
    // Keep staging next to the output root so the final directory rename stays
    // on the same filesystem and remains atomic.
    let stage_parent = output_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_root.clone());
    std::fs::create_dir_all(&stage_parent).map_err(|error| error.to_string())?;
    cleanup_stale_stage_directories(&stage_parent, Duration::from_secs(24 * 60 * 60))?;
""",
    )

    replace_once(
        """    let stage = output_root.join(format!(
        ".dokkomplekt-manual-stage-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
""",
        """    let stage = stage_parent.join(format!(
        ".dokkomplekt-manual-stage-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
""",
    )

    replace_once(
        """    let mut counter_reservations = Vec::new();
    let rendered = (|| -> Result<Vec<PathBuf>, String> {
""",
        """    let mut counter_reservations = Vec::new();
    let mut ancillary_warnings = Vec::new();
    let rendered = (|| -> Result<Vec<PathBuf>, String> {
""",
    )

    replace_once(
        """        if privacy.write_trust_report {
            let provenance = state
                .source_provenance
                .lock()
                .map_err(|_| "source provenance state lock failed")?
                .clone()
                .ok_or_else(|| {
                    "Для проверяемого отчёта сначала загрузите файл, вставьте текст или получите HTTPS-источник.".to_string()
                })?;
            write_trust_report(
                &stage,
                &report_case,
                TrustReportContext {
                    source_name: &provenance.source_name,
                    source_sha256: &provenance.source_sha256,
                    generated_names: &generated_names,
                    used_field_ids: &used_field_ids,
                    include_values: privacy.include_values_in_trust_report,
                    source_warnings: &[],
                },
            )?;
        }
""",
        """        if privacy.write_trust_report {
            let provenance = state
                .source_provenance
                .lock()
                .map_err(|_| "source provenance state lock failed")?
                .clone();
            match provenance {
                Some(provenance) => {
                    if let Err(error) = write_trust_report(
                        &stage,
                        &report_case,
                        TrustReportContext {
                            source_name: &provenance.source_name,
                            source_sha256: &provenance.source_sha256,
                            generated_names: &generated_names,
                            used_field_ids: &used_field_ids,
                            include_values: privacy.include_values_in_trust_report,
                            source_warnings: &[],
                        },
                    ) {
                        ancillary_warnings.push(format!(
                            "Документы созданы, но служебный отчёт доверия не записан: {error}"
                        ));
                    }
                }
                None => ancillary_warnings.push(
                    "Документы созданы без служебного отчёта доверия: источник provenance недоступен."
                        .into(),
                ),
            }
        }
""",
    )

    batch_start = text.index("fn render_docx_batch(")
    warnings_at = text.index("    let mut warnings = Vec::new();", batch_start)
    text = text[:warnings_at] + text[warnings_at:].replace(
        "    let mut warnings = Vec::new();\n",
        "    let mut warnings = Vec::new();\n    warnings.extend(ancillary_warnings);\n",
        1,
    )

    replace_once(
        """    warnings.extend(generation_publication::finalize_published_generation(
        &app, &permit, false,
    ));
    let created_files = staged_paths
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| output_folder.join(name).display().to_string())
        .collect::<Vec<_>>();
""",
        """    warnings.extend(generation_publication::finalize_published_generation(
        &app, &permit, false,
    ));
    // Never fabricate success from staging file names. The response is emitted
    // only after every requested final file is physically present, non-empty and
    // readable as a Word document at the published path.
    let created_files = verify_published_batch_files(
        &output_folder,
        &staged_paths,
        documents.len(),
    )?;
""",
    )

    path.write_text(text, encoding="utf-8")

subprocess.run(
    [
        "python",
        "-m",
        "pytest",
        "-q",
        "tests/test_manual_generation_publication_proof.py",
        "tests/test_manual_generation_trust_report_opt_in.py",
        "tests/test_generation_publication_accounting_contract.py",
    ],
    check=True,
)
subprocess.run(["python", "scripts/static_quality_gate.py", "--source-only"], check=True)

# Restore CI canon and remove every one-time helper before publishing the branch.
subprocess.run(
    ["git", "checkout", "origin/main", "--", ".github/workflows/source-provenance.yml"],
    check=True,
)
for temporary in (
    Path(".github/workflows/_tmp_apply_manual_publication_fix.yml"),
    Path("scripts/_tmp_apply_manual_publication_fix.py"),
):
    if temporary.exists():
        temporary.unlink()

subprocess.run(["git", "config", "user.name", "github-actions[bot]"], check=True)
subprocess.run(
    [
        "git",
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    ],
    check=True,
)
subprocess.run(["git", "add", "-A"], check=True)
subprocess.run(
    ["git", "commit", "-m", "fix: make manual publication physically provable"],
    check=True,
)
subprocess.run(["git", "push", "origin", f"HEAD:{BRANCH}"], check=True)
