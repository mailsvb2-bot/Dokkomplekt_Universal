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
    assert "button_label_collision_key" in source
    assert "повторяющийся идентификатор документа" in source
    assert "конфликтующие названия кнопок" in source
    assert "template_entries" in source
    assert "write_transfer_archive_atomically" in source
    assert ".create_new(true)" in source
    assert ".sync_all()" in source
    assert "std::fs::hard_link(&temporary, output_path)" in source
    assert "std::fs::rename(&temporary, output_path)" not in source
    assert "Файл пакета переноса уже существует" in source
    assert "let publication = publish_pack_with_template_versions" in source
    assert "remove_dir_all(&import_root)" in source


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


def test_fpr15_installed_lane_requires_clean_profile_export_import_and_new_case_output() -> None:
    source = (ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1").read_text(encoding="utf-8-sig")
    for marker in (
        "FPR-15 / ACC-61: real installed clean-profile transfer proof.",
        "FPR-15 EXPORT PRIVACY PASS:",
        "FPR-15 CLEAN PROFILE PASS:",
        "FPR-15 IMPORT PASS:",
        "FPR15-NEW-CLEAN-PROFILE",
        "Remove-Item -LiteralPath $appDataRoot -Recurse -Force -ErrorAction Stop",
        "Set-OpenFileDialogPath -Dialog $importDialog -Path $fpr15Package.FullName",
        "FPR-15 INSTALLED PASS: export -> privacy read-back -> clean profile -> import -> new case -> physical DOCX -> committed receipt",
    ):
        assert marker in source
    assert source.index("FPR-15 CLEAN PROFILE PASS:") < source.index("FPR-15 IMPORT PASS:")
    assert source.index("FPR-15 IMPORT PASS:") < source.index("FPR-15 INSTALLED PASS:")
