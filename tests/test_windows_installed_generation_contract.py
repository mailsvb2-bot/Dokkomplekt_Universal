from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
WORKSPACE = ROOT / "src" / "components" / "Workspace.tsx"
FOLDER_ONBOARDING = ROOT / "src" / "components" / "FolderNamingOnboarding.tsx"


def test_windows_installer_smoke_drives_real_generation_to_physical_docx() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")
    workspace = WORKSPACE.read_text(encoding="utf-8")
    onboarding = FOLDER_ONBOARDING.read_text(encoding="utf-8")

    assert 'aria-label="Выбрать исходный файл"' in workspace
    assert 'autoFocus aria-keyshortcuts="Enter"' in onboarding
    assert "Сохранить папку и правило" in onboarding
    assert "function Activate-LiveAppWindow" in source
    assert "SetForegroundWindow" in source
    assert "ShowWindow($hwnd, 9)" in source
    assert "Activate-LiveAppWindow -Window $appWindow" in source
    assert '<form className="modal folderNamingOnboarding"' in onboarding
    assert 'type="submit" className="primaryBtn" autoFocus aria-keyshortcuts="Enter"' in onboarding
    assert "function Get-AppStateCipherFingerprint" in source
    assert "function Wait-AppStateCipherFingerprint" in source
    assert "$outputPreferenceStateKey = 'output_preferences_v2'" in source
    assert "[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')" in source
    assert "-DifferentFrom $beforeFolderRuleSave" in source
    assert "durable native state mutation" in source
    assert "Find-FocusedReadyButtonByNames" not in source
    assert "Get-FocusedElementForProcess" not in source
    assert "[System.Windows.Automation.TreeScope]::Descendants" in source
    assert "$saveFolderRule = Find-ButtonByNames" not in source
    assert "$createPreparedButton = Wait-UiElement" not in source
    assert "$generateButton = Wait-UiElement" not in source
    assert "UI element did not become enabled and actionable within 5 seconds." not in source
    assert "Выбрать исходный файл" in source
    assert "native source file picker" in source
    assert "Real source DOCX accepted by installed application" in source
    assert "$coldStartDeadlineSeconds = 20" in source
    assert "AddSeconds($coldStartDeadlineSeconds)" in source
    assert "$templateRegistrationDeadlineSeconds = 90" in source
    assert "$templateSetupTransitionDeadlineSeconds = 5" in source
    assert "UIA action produced no observable template-registration transition; retrying once with physical input." in source
    assert "Создать кнопки (1) physical retry" in source
    assert "-TimeoutSeconds $templateRegistrationDeadlineSeconds" in source
    assert "@('Проверить и создать (1)', 'Создать документы (1)')" in source
    assert "workflow-document-number" in source
    assert "workflow-document-date" in source
    assert "Создать документы" in source
    assert "Invoke-UiElementPhysically" in source
    assert "Invoke-UiActionWithObservedTransition" in source
    assert "function Invoke-UiActionPhysicallyFromProbe" in source
    assert "Never poll the same WebView2 AutomationElement" in source
    assert "function Invoke-UiActionFromProbe" in source
    assert "Invoke-UiActionFromProbe -ActionProbe $ActionProbe -Description $Description" in source
    assert "-Description 'Выбрать всё button'" in source
    assert "-TransitionDescription 'generation action for one selected document'" in source
    assert "Selecting all documents did not expose the one-document generation action." in source
    assert "$selectAllButton = Wait-UiElement" not in source
    assert "$actionStateDeadline = [DateTime]::UtcNow.AddSeconds(2)" in source
    assert "remained unavailable for 2 seconds and is treated as already in-flight; waiting for '$TransitionDescription' without a duplicate click" in source
    assert "remains actionable; retrying once with physical input" in source
    assert "-Description 'repeat generation action'" in source
    assert "-TransitionDescription 'repeat preflight'" in source
    assert "-Description 'existing-kit Другие варианты'" in source
    assert "-TransitionDescription 'Создать новую версию'" in source
    assert "-Description 'Создать новую версию'" in source
    assert 'Invoke-UiActionPhysicallyFromProbe -ActionProbe $ActionProbe' in source
    assert "$generationTransitionDeadlineSeconds = 5" in source
    assert "UIA action produced no observable generation transition; retrying once with physical input." in source
    assert "--- installed UI snapshot after generation timeout ---" in source
    assert '$expectedGeneratedFileName = "$expectedTemplateButtonName.docx"' in source
    assert "-Filter $expectedGeneratedFileName" in source
    assert "$newVersionAbsence = [pscustomobject]@{ Since = $null }" in source
    assert "$newVersionAbsence.Since = [DateTime]::UtcNow" in source
    assert ".TotalSeconds -ge 2" in source
    assert "Проверочная кнопка.docx" not in source
    assert "[System.IO.Compression.ZipFile]::OpenRead" in source
    assert "Created DOCX lost the template content" in source
    assert "Installed end-to-end document generation OK" in source
