from __future__ import annotations

from source_helpers import project_text as text


def test_unicode_button_labels_never_slice_inside_utf8() -> None:
    source = text("crates/dokkomplekt-core/src/template_intelligence.rs")
    assert "label.chars().count() > 42" in source
    assert "label.chars().take(42).collect()" in source
    assert "label.truncate(42)" not in source
    assert "button_label_truncation_preserves_utf8_boundaries" in source


def test_manual_generation_uses_real_source_provenance() -> None:
    main = text("src-tauri/src/main.rs")
    commands = text("src-tauri/src/subsystems/document_commands.rs")
    desktop = text("src-tauri/src/subsystems/desktop_io.rs")
    intake = "\n".join((
        text("src-tauri/src/universal_intake.rs"),
        text("src-tauri/src/universal_intake/web.rs"),
    ))

    assert "manual-session" not in commands
    assert "struct SourceProvenance" in main
    assert "Sha256::digest(bytes)" in main
    assert "source_provenance: Mutex<Option<SourceProvenance>>" in main
    assert "Источник не содержит проверяемый SHA-256" in main
    assert "source_sha256: hex::encode(Sha256::digest(&bytes))" in intake
    assert "Отчёт проверяемости требует настоящий SHA-256 исходника" in desktop
    # Provenance remains mandatory for a trust report, but an ancillary report
    # must never roll back already rendered user DOCX files.
    assert "if privacy.write_trust_report" in commands
    assert "source_provenance" in commands
    assert "Документы созданы без служебного отчёта доверия" in commands
    assert "Для проверяемого отчёта сначала загрузите файл" not in commands


def test_second_launch_activates_primary_without_setup_panic() -> None:
    source = text("src-tauri/src/main.rs")
    for invariant in (
        "InstanceLockOutcome::AlreadyRunning",
        "activation-requests",
        "Uuid::new_v4()",
        "create_new(true)",
        "ACTIVATION_TEMP_MAX_AGE",
        "symlink_metadata",
        '.request"',
        "run_on_main_thread",
        "window.show()",
        "window.unminimize()",
        "window.set_focus()",
        "enqueue_activation_request(&handle)",
        "handle.exit(0)",
    ):
        assert invariant in source
    assert "std::io::ErrorKind::AlreadyExists, error" not in source


def test_ui_uses_real_system_tools_before_component_download() -> None:
    source = text("src/App.tsx")
    assert "getSidecarStatus" in source
    assert "requiredToolsAvailable" in source
    assert "['tesseract']" in source
    assert "['pdftotext']" in source
    assert "['soffice']" in source
    assert "компонент отмечен установленным, но требуемые программы не запускаются" in source


def test_linux_installer_gate_requires_rendered_named_window_without_extra_ci_packages() -> None:
    contract = "\n".join(
        (
            text("tests/installer/linux_installer_contract.sh"),
            text("tests/installer/linux_installer_contract_core.sh"),
        )
    )
    probe = text("scripts/verify_rendered_x11_window.py")

    for invariant in (
        "Dokkomplekt Universal",
        "verify_rendered_x11_window.py",
        '--title "Dokkomplekt Universal"',
        "--min-width 800",
        "--min-height 500",
        "--min-colors 64",
        "--screenshot",
        "verify_webkit_pixel_golden.py",
        "webkit-linux-golden.json",
        "did not prove native frontend IPC readiness",
    ):
        assert invariant in contract

    for invariant in (
        'ctypes.util.find_library("X11")',
        "XInternAtom",
        "XGetWindowProperty",
        "_NET_WM_NAME",
        "XQueryTree",
        "XFetchName",
        "XGetWindowAttributes",
        "XTranslateCoordinates",
        "XGetImage",
        "XGetPixel",
        "XDestroyImage",
        "minimum_colors",
        "P6",
        "attributes.map_state != self.IS_VIEWABLE",
        "self.root",
    ):
        assert invariant in probe

    for external_tool in (
        "xwininfo",
        "import -silent",
        "identify -format",
        "wmctrl",
        "scrot",
        "openbox",
    ):
        assert external_tool not in contract


def test_isolated_python_runner_does_not_capture_pipe_from_descendants() -> None:
    source = text("scripts/run_python_contracts_sharded.py")
    assert "NamedTemporaryFile" in source
    assert "process.wait(timeout=timeout_seconds)" in source
    assert "stdout=subprocess.PIPE" not in source
    assert "terminate_tree(process)" in source
