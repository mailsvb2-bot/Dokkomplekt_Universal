from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_generation_is_one_visible_flow_and_uses_output_plan_for_single_document() -> None:
    app = text("src/App.tsx")
    workspace = text("src/components/Workspace.tsx")
    assert "Проверить и создать" in workspace
    assert "Сохранить ответы" not in workspace
    assert "Создать только этот документ" not in workspace
    assert "renderDocx(documentId, `output/${documentId}.docx`" not in app
    assert "renderDocxBatch(" in app
    assert "outputRoot.trim() || 'output/Готовые документы'" in app


def test_document_selection_is_not_silently_reset_by_the_rail() -> None:
    rail = text("src/components/DocumentRail.tsx")
    assert "previousDocumentIds" not in rail
    assert "newlyAddedSelectedIds" not in rail
    assert "onGenerateSelected" not in rail


def test_sick_leave_option_is_contextual_and_plain_language() -> None:
    app = text("src/App.tsx")
    workspace = text("src/components/Workspace.tsx")
    rail = text("src/components/DocumentRail.tsx")
    assert "showSickLeaveOption" in app
    assert "Оформляется больничный лист" in workspace
    assert "Учитывать дополнительные условия выбранных шаблонов" not in rail


def test_expert_settings_are_not_mixed_into_everyday_settings() -> None:
    utility = text("src/components/UtilityPanel.tsx")
    assert "Основные настройки" in utility
    assert "Экспертные и административные инструменты" in utility
    assert '<details className="expertSettings">' in utility
    assert "Папка готовых документов" in utility


def test_ordinary_word_template_creates_a_button_without_placeholder_gate() -> None:
    setup = text("src/components/TemplateSetupModal.tsx")
    backend = text("src-tauri/src/subsystems/document_commands.rs")
    assert "Один Word-файл создаёт одну кнопку" in setup
    assert "будет добавлен как рабочая кнопка" in setup
    assert "allowStaticCopies" not in setup
    assert "if req.rows.iter().any(|row| row.is_static_copy)" not in backend
    assert "ordinary_unmarked_word_template_is_valid_for_button_creation" in backend


def test_primary_interaction_matches_proven_word_first_flow() -> None:
    workspace = text("src/components/Workspace.tsx")
    backend = text("src-tauri/src/subsystems/document_commands.rs")
    assert "Перетащите первичный осмотр или направление" in workspace
    assert 'accept=".docx,.docm,.doc"' in workspace
    assert "Другой файл" in workspace
    assert "primaryDocumentDrop" in workspace
    assert "Один файл создаёт одну кнопку" in backend
