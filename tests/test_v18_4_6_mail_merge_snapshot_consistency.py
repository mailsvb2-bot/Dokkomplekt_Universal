from pathlib import Path

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


def test_mail_merge_revalidates_before_publish_and_again_before_commercial_commit() -> None:
    body = _mail_merge_body()
    guard = "ensure_mail_merge_templates_current(&template_inputs)"
    publish = "publish_stage_to_unique_directory(&stage, &desired)"
    commit = "commit_generation_access(&app, &permit)"
    assert body.count(guard) == 2
    first_verify = body.index(guard)
    publish_at = body.index(publish)
    second_verify = body.index(guard, first_verify + 1)
    commit_at = body.index(commit)
    assert first_verify < publish_at < second_verify < commit_at

    before_publish = body[first_verify:publish_at]
    assert "remove_dir_all(&stage)" in before_publish
    assert "rollback_counter_reservations" in before_publish
    assert "rollback_generation_access" in before_publish

    before_commit = body[second_verify:commit_at]
    assert "remove_dir_all(&published)" in before_commit
    assert "rollback_counter_reservations" in before_commit
    assert "rollback_generation_access" in before_commit


def test_mail_merge_snapshot_helper_extracts_text_from_immutable_snapshot() -> None:
    text = MAIL_MERGE.read_text(encoding="utf-8")
    helper = text.split("fn capture_mail_merge_template_snapshot(", 1)[1].split(
        "fn ensure_mail_merge_templates_current", 1
    )[0]
    assert "TemplateSnapshot::capture(app, configured_path, button_label)" in helper
    assert "extract_docx_text(snapshot.path())" in helper
    assert "resolve_user_path" not in helper
