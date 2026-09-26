from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = ROOT / "src-tauri" / "src" / "subsystems" / "template_transfer.rs"
APP = ROOT / "src" / "App.tsx"
RAIL = ROOT / "src" / "components" / "DocumentRail.tsx"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr15_transfer_package_is_template_only_and_integrity_checked() -> None:
    source = RUST.read_text(encoding="utf-8")
    for marker in (
        'dokkomplekt.template-transfer.v1',
        'manifest.json',
        'template_sha256',
        'validate_safe_template_file',
        'sha256_bytes',
        'ZipWriter',
        'ZipArchive',
        'pick_template_transfer_file',
    ):
        assert marker in source
    manifest_block = source[source.index("struct TemplateTransferManifest"):source.index("struct ExportTemplateTransferRequest")]
    assert "semantic_case" not in manifest_block
    assert "template_path" not in manifest_block
    assert "source_path" not in manifest_block
    assert 'import_root.join(format!("{index:04}.{extension}"))' in source
    assert '{index:04}-{id}' not in source


def test_fpr15_transfer_is_visible_in_primary_template_management_ui() -> None:
    app = APP.read_text(encoding="utf-8")
    rail = RAIL.read_text(encoding="utf-8")
    assert "useTemplateTransfer" in app
    assert "onExportTemplates={exportTemplates}" in app
    assert "onImportTemplates={importTemplates}" in app
    assert "Экспорт шаблонов" in rail
    assert "Импорт шаблонов" in rail


def test_fpr15_register_does_not_claim_runtime_closure_before_clean_profile_proof() -> None:
    register = REGISTER.read_text(encoding="utf-8")
    assert '"id": "FPR-15"' in register
    assert '"status": "needs-runtime-proof"' in register
    assert "clean-profile" in register.lower() or "чист" in register.lower()
