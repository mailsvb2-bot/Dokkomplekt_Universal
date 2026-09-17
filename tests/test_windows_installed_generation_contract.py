from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_LAUNCHER = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.impl.ps1"
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
    assert "Психический статус" in source
    assert "Шаблонный психический статус старого пациента" in source
    assert "Контактен, ориентирован, эмоционально напряжён" in source
    assert "medical.profile_status from the current primary source" in source
    assert "old template medical.profile_status" in source
    assert "compiler-owned medical.profile_status placeholder unresolved" in source
    assert "Психический статус: ______ после компиляции" in source
    assert "lost the literal suffix around compiler-owned medical.profile_status" in source
    assert "left the profile-status blank unresolved" in source
    assert "doctor-owned literal opener used by the profile-status regression" in source
    assert "-Role 'sick_leave_vk'" in source
    assert "medical.sick_leave_vk.position" in source
    assert "New-MedicalStoryDocxFixture -Path $medicalSource -Variant 'source' -Role 'sick_leave_vk'" in source
    assert "if ($Role -ne 'sick_leave_vk')" in source
    assert "'medical.position' = 'инженер'" in source
    assert "canonical shared medical.position prompt" in source
    assert "Installed sick_leave_vk generation did not render the current protocol number" in source
    assert "Installed sick_leave_vk generation left medical.sick_leave_vk.position unresolved" in source
    assert "Installed end-to-end document generation OK" in source


def test_quality_windows_installer_exercises_blank_diary_filler_discharge() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")
    assert "$env:DOKKOMPLEKT_REQUIRE_AUTHENTICODE -eq '0'" in source
    assert "The unsigned Preview is an independent installed-app lane" in source
    assert "function New-BlankDischargeDocxFixture" in source
    assert "Дата, время      Выписной эпикриз №" in source
    assert "Находился на лечении в ГБУЗ НО «НКЦПЗ» диспансер №2  с по" in source
    assert "Психический статус при поступлении:" in source
    assert "Сомато-неврологический статус: Нормального питания." in source
    assert "if ($adversarialMedicalRole -eq 'discharge')" in source
    assert "Installed discharge preflight did not expose medical.discharge_date" in source
    assert "Blank discharge template did not render the current diagnosis" in source
    assert "Blank discharge compiler consumed the following somatic-status section" in source
    assert "Blank discharge generation left a semantic placeholder unresolved" in source


def test_e2_installed_learning_observes_pair_selections_and_final_blank_acceptance() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")
    start = source.index("function Open-E2FileSelection")
    end = source.index("function Set-E2NamedValue", start)
    picker = source[start:end]

    assert "[string[]]$ExpectedUiNames = @()" in picker
    assert "expected UI evidence count must match selected path count" in picker
    assert 'Wait-UiElement -Description "$Label accepted selection $($selectionIndex + 1)"' in picker
    assert "$actionButtonName = if ($selectionIndex -eq 0 -or $ExpectedUiNames.Count -eq 0)" in picker
    assert "Find-ReadyButtonByNames -Root $currentAppWindow -Names @($actionButtonName)" in picker
    assert "Find-ButtonByNames -Root $currentAppWindow -Names @($expectedUiName)" in picker
    assert "Start-Sleep -Milliseconds 150" not in picker
    assert "Open-E2FileSelection -Label 'Пустой DOCX/DOCM' -Paths @($e2Blank)" in source
    assert "-Paths @($e2Blank) -ExpectedUiNames" not in source
    assert "React only renders" in source
    for expected in (
        "4–10 правильных результатов. Собрано: Correct Output 1/4–10, Source 0/4–10.",
        "4–10 правильных результатов. Собрано: Correct Output 4/4–10, Source 0/4–10.",
        "4–10 исходных документов Source. Собрано: Correct Output 4/4–10, Source 1/4–10.",
        "4–10 исходных документов Source. Готово к проверке. Пар: 4.",
    ):
        assert expected in source


def test_e2_installed_learning_timeout_emits_fail_only_ui_diagnostic() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "function Write-E2LearningUiDiagnostic" in source
    assert "E2 UI diagnostic begin" in source
    assert "ошиб|обуч|провер|карт|готов|валид|publish|шаблон|source|correct" in source
    assert "Write-E2LearningUiDiagnostic -Root $currentAppWindow" in source
    assert "throw $learningFailure" in source
    assert "publishable held-out learning result" in source


def test_e2_launcher_returns_refreshed_output_identity_prompts_to_real_ui() -> None:
    launcher = WINDOWS_LAUNCHER.read_text(encoding="utf-8-sig")
    implementation = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "windows_installer_contract.impl.ps1" in launcher
    assert "expected exactly one commit-boundary anchor" in launcher
    assert "$implementation.Replace($anchor, $replacement)" in launcher
    assert "workflow-document-number" in launcher
    assert "workflow-document-date" in launcher
    assert "E2-7708004767" in launcher
    assert "18.09.2026" in launcher
    assert "E2 Создать документы after refreshed preflight" in launcher
    assert "confirm a second time instead of bypassing the naming contract" in launcher
    anchor = """  $e2CreateAction.SetFocus()\n  Start-Sleep -Milliseconds 100\n  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')\n\n  $e2ExpectedFile = \"$e2ButtonLabel.docx\""""
    assert implementation.count(anchor) == 1
