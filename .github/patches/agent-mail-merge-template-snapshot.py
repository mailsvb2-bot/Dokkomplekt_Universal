from pathlib import Path

runtime_path = Path("src-tauri/src/subsystems/automation_runtime.rs")
runtime = runtime_path.read_text(encoding="utf-8")
mail_merge_path = Path("src-tauri/src/subsystems/automation_mail_merge.rs")
if mail_merge_path.exists():
    raise SystemExit("automation_mail_merge.rs already exists")

start_marker = "#[derive(Debug, Deserialize)]\nstruct RenderMailMergeRequest"
end_marker = "#[derive(Debug, Deserialize)]\nstruct ImportTemplateFileRequest"
if runtime.count(start_marker) != 1 or runtime.count(end_marker) != 1:
    raise SystemExit("mail-merge extraction markers are not unique")
prefix, remainder = runtime.split(start_marker, 1)
mail_merge_tail, suffix = remainder.split(end_marker, 1)
mail_merge = start_marker + mail_merge_tail
runtime = prefix.rstrip() + "\n\n" + end_marker + suffix
runtime_path.write_text(runtime, encoding="utf-8")

needle = '''    let count = documents
        .len()
        .checked_mul(table.rows.len())
'''
replacement = '''    let template_inputs = documents
        .iter()
        .map(|document| {
            capture_mail_merge_template_snapshot(
                &app,
                &document.button_label,
                &document.template_path,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = template_inputs
        .len()
        .checked_mul(table.rows.len())
'''
if mail_merge.count(needle) != 1:
    raise SystemExit(f"template capture insertion marker mismatch: {mail_merge.count(needle)}")
mail_merge = mail_merge.replace(needle, replacement, 1)

old_loop = '''            for doc in &documents {
                let template_path = resolve_user_path(&app, &doc.template_path)?;
                let text = extract_docx_text(&template_path).map_err(|e| e.to_string())?;
                let hydrated =
                    hydrate_case_with_persistent_template_data(&app, &row_case, &[text], true)?;
                counter_reservations.extend(hydrated.counter_reservations);
                let ext = template_path
                    .extension()
                    .and_then(|x| x.to_str())
                    .filter(|x| x.eq_ignore_ascii_case("docm"))
                    .unwrap_or("docx");
                let out = row_dir.join(format!(
                    "{}.{}",
                    sanitize_path_component(&doc.button_label),
                    ext
                ));
                render_docx_with_assets(
                    &app,
                    &template_path,
                    &out,
                    &hydrated.case,
                    req.strict,
                    permit.watermark.as_deref(),
                )
                .map_err(|e| format!("Строка {} / {}: {e}", row_index + 1, doc.button_label))?;
                files.push(out);
            }
'''
new_loop = '''            for template in &template_inputs {
                let template_path = template.snapshot.path();
                let hydrated = hydrate_case_with_persistent_template_data(
                    &app,
                    &row_case,
                    std::slice::from_ref(&template.text),
                    true,
                )?;
                counter_reservations.extend(hydrated.counter_reservations);
                let ext = template_path
                    .extension()
                    .and_then(|x| x.to_str())
                    .filter(|x| x.eq_ignore_ascii_case("docm"))
                    .unwrap_or("docx");
                let out = row_dir.join(format!(
                    "{}.{}",
                    sanitize_path_component(&template.button_label),
                    ext
                ));
                render_docx_with_assets(
                    &app,
                    template_path,
                    &out,
                    &hydrated.case,
                    req.strict,
                    permit.watermark.as_deref(),
                )
                .map_err(|e| {
                    format!(
                        "Строка {} / {}: {e}",
                        row_index + 1,
                        template.button_label
                    )
                })?;
                files.push(out);
            }
'''
if mail_merge.count(old_loop) != 1:
    raise SystemExit(f"live template loop marker mismatch: {mail_merge.count(old_loop)}")
mail_merge = mail_merge.replace(old_loop, new_loop, 1)

publish_marker = '''    let desired = root.join(format!(
        "Пакетная генерация {}",
        OffsetDateTime::now_utc().date()
    ));
'''
publish_guard = '''    if let Err(error) = ensure_mail_merge_templates_current(&template_inputs) {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let desired = root.join(format!(
        "Пакетная генерация {}",
        OffsetDateTime::now_utc().date()
    ));
'''
if mail_merge.count(publish_marker) != 1:
    raise SystemExit(f"publication marker mismatch: {mail_merge.count(publish_marker)}")
mail_merge = mail_merge.replace(publish_marker, publish_guard, 1)

helper = r'''/// Immutable template material used by one entire mail-merge operation.
///
/// Text extraction and DOCX rendering both read the same snapshot. The live
/// template is consulted again only at the publication boundary.
struct MailMergeTemplateSnapshot {
    button_label: String,
    snapshot: template_snapshot::TemplateSnapshot,
    text: String,
}

fn capture_mail_merge_template_snapshot(
    app: &tauri::AppHandle,
    button_label: &str,
    configured_path: &str,
) -> Result<MailMergeTemplateSnapshot, String> {
    let snapshot =
        template_snapshot::TemplateSnapshot::capture(app, configured_path, button_label)?;
    let text = extract_docx_text(snapshot.path()).map_err(|error| {
        format!(
            "Не удалось прочитать стабилизированный шаблон «{button_label}»: {error}"
        )
    })?;
    Ok(MailMergeTemplateSnapshot {
        button_label: button_label.to_string(),
        snapshot,
        text,
    })
}

fn ensure_mail_merge_templates_current(
    templates: &[MailMergeTemplateSnapshot],
) -> Result<(), String> {
    for template in templates {
        template.snapshot.ensure_current()?;
    }
    Ok(())
}
'''
mail_merge_path.write_text(helper.rstrip() + "\n\n" + mail_merge.lstrip(), encoding="utf-8")

main_path = Path("src-tauri/src/main.rs")
main = main_path.read_text(encoding="utf-8")
include_marker = 'include!("subsystems/automation_consistency.rs");\n'
include_replacement = include_marker + 'include!("subsystems/automation_mail_merge.rs");\n'
if main.count(include_marker) != 1:
    raise SystemExit(f"mail-merge include marker mismatch: {main.count(include_marker)}")
main_path.write_text(main.replace(include_marker, include_replacement, 1), encoding="utf-8")

test_path = Path("tests/test_v18_4_6_mail_merge_snapshot_consistency.py")
if test_path.exists():
    raise SystemExit("mail-merge consistency contract already exists")
test_path.write_text(r'''from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "src-tauri" / "src" / "subsystems" / "automation_runtime.rs"
MAIL_MERGE = ROOT / "src-tauri" / "src" / "subsystems" / "automation_mail_merge.rs"
MAIN = ROOT / "src-tauri" / "src" / "main.rs"


def _mail_merge_body() -> str:
    text = MAIL_MERGE.read_text(encoding="utf-8")
    return text.split("fn render_mail_merge(", 1)[1]


def test_mail_merge_is_split_out_of_runtime_and_included_once() -> None:
    runtime = RUNTIME.read_text(encoding="utf-8")
    main = MAIN.read_text(encoding="utf-8")
    assert "fn render_mail_merge(" not in runtime
    assert main.count('include!("subsystems/automation_mail_merge.rs");') == 1


def test_mail_merge_captures_templates_before_row_loop_and_never_rereads_live_paths() -> None:
    body = _mail_merge_body()
    assert body.index("capture_mail_merge_template_snapshot(") < body.index("for row_index in")
    assert "resolve_user_path(&app, &doc.template_path)" not in body
    assert "extract_docx_text(&template_path)" not in body
    assert "let template_path = template.snapshot.path();" in body
    assert "std::slice::from_ref(&template.text)" in body
    assert "render_docx_with_assets(\n                    &app,\n                    template_path," in body


def test_mail_merge_revalidates_all_live_templates_before_atomic_publish() -> None:
    body = _mail_merge_body()
    verify = body.index("ensure_mail_merge_templates_current(&template_inputs)")
    publish = body.index("publish_stage_to_unique_directory(&stage, &desired)")
    assert verify < publish
    guarded = body[verify:publish]
    assert "remove_dir_all(&stage)" in guarded
    assert "rollback_counter_reservations" in guarded
    assert "rollback_generation_access" in guarded


def test_mail_merge_snapshot_helper_extracts_text_from_immutable_snapshot() -> None:
    text = MAIL_MERGE.read_text(encoding="utf-8")
    helper = text.split("fn capture_mail_merge_template_snapshot(", 1)[1].split(
        "fn ensure_mail_merge_templates_current", 1
    )[0]
    assert "TemplateSnapshot::capture(app, configured_path, button_label)" in helper
    assert "extract_docx_text(snapshot.path())" in helper
    assert "resolve_user_path" not in helper
''', encoding="utf-8")
