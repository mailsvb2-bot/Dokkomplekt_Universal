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


def test_static_template_requires_explicit_user_consent() -> None:
    setup = text("src/components/TemplateSetupModal.tsx")
    assert "allowStaticCopies" in setup
    assert "Создавать неразмеченные шаблоны как неизменяемые копии" in setup
    assert "Это неизменяемый документ без автоматически заполняемых полей" in setup
