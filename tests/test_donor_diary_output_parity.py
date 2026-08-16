from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def test_first_run_requires_visible_output_folder_and_generation_lists_files():
    support = read("src/lib/appSupport.ts")
    onboarding = read("src/components/FolderNamingOnboarding.tsx")
    app = read("src/App.tsx")
    workspace = read("src/components/Workspace.tsx")

    assert "return '';" in support
    assert "Куда сохранять готовые документы" in onboarding
    assert "Выбрать папку на компьютере" in onboarding
    assert "disabled={!root || !selected.length}" in onboarding
    assert "currentRoot={outputRoot}" in app
    assert "onPickRoot={() => void chooseFolder" in app
    assert "Документы созданы:" in workspace
    assert "<strong>Папка:</strong>" in workspace
    assert "Созданные файлы" in workspace
    assert "Открыть папку с документами" in workspace


def test_diary_popup_is_fail_closed_and_donor_style_is_present():
    popup = read("crates/dokkomplekt-core/src/popup_profiles.rs")
    assert '"1, 2, 3, 7, затем 2 раза в неделю"' in popup
    assert "config.ask_mode = PromptAskMode::Always;" in popup
    assert "config.default_value = None;" in popup
    assert "add(DIARY_SCHEDULE_STYLE, true);" in popup
    assert "add(DIARY_INTRADAY_RHYTHM, true);" in popup
