from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_first_run_creates_buttons_without_mandatory_placeholder_markup():
    source = (ROOT / "src" / "components" / "TemplateSetupModal.tsx").read_text(encoding="utf-8")

    assert "Каждый DOCX или DOCM сразу станет отдельной кнопкой" in source
    assert "Сначала создайте кнопки и начните работать" in source
    assert "const batchReady = hasBatch && !invalidLabel" in source
    assert "const manualReady = Boolean(props.templateText.trim())" in source
    assert "hasConfirmedPlaceholder" not in source
    assert "allowStaticCopies" not in source


def test_template_scanner_is_optional_after_button_creation():
    source = (ROOT / "src" / "components" / "TemplateSetupModal.tsx").read_text(encoding="utf-8")

    assert "Необязательно: настроить автоматическое заполнение" in source
    assert "Этот шаг не нужен для создания кнопки. Его можно выполнить позже." in source
    assert "onStartGuidedPendingScanner" in source
    assert "PopupFieldEditor" in source


def test_empty_first_run_still_has_one_clear_create_buttons_action():
    rail = (ROOT / "src" / "components" / "DocumentRail.tsx").read_text(encoding="utf-8")

    assert "Создать свои кнопки" in rail
    assert "Один файл станет одной понятной кнопкой" in rail
