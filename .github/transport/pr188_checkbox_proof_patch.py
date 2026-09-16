from pathlib import Path

contract = Path("tests/installer/windows_installer_contract.ps1")
text = contract.read_text(encoding="utf-8-sig")
old = """  $checkbox = Wait-UiElement -Description 'E2 learned document checkbox' -TimeoutSeconds 30 -Probe {
    $currentAppWindow = Find-LiveAppWindow
    if ($null -eq $currentAppWindow) { return $null }
    Find-E2NamedElement -Root $currentAppWindow -Name \"Добавить $e2ButtonLabel в комплект\"
  }
  if (-not $checkbox.Current.IsTogglePatternAvailable) { throw 'E2 learned document checkbox has no TogglePattern.' }
  $toggle = $checkbox.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
  if ($toggle.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On) { $toggle.Toggle() }

  $preflight = Invoke-UiActionWithObservedTransition `
"""
new = """  Invoke-UiActionWithObservedTransition `
    -Description 'E2 learned document checkbox' `
    -TransitionDescription 'E2 learned document selected for generation' `
    -ActionProbe {
      $currentAppWindow = Find-LiveAppWindow
      if ($null -eq $currentAppWindow) { return $null }
      $nameCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        \"Добавить $e2ButtonLabel в комплект\"
      )
      $typeCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::CheckBox
      )
      $condition = [System.Windows.Automation.AndCondition]::new($nameCondition, $typeCondition)
      $candidate = $currentAppWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
      if ($null -ne $candidate -and $candidate.Current.IsEnabled) { $candidate } else { $null }
    } `
    -TransitionProbe {
      $currentAppWindow = Find-LiveAppWindow
      if ($null -eq $currentAppWindow) { return $null }
      Find-ReadyButtonByNames -Root $currentAppWindow -Names @('Проверить и создать (1)', 'Создать документы (1)')
    } | Out-Null

  $preflight = Invoke-UiActionWithObservedTransition `
"""
if text.count(old) != 1:
    raise SystemExit(f"checkbox block changed: {text.count(old)}")
text = text.replace(old, new, 1)
contract.write_text(text, encoding="utf-8-sig", newline="\n")

test_path = Path("tests/test_windows_installed_generation_contract.py")
test_text = test_path.read_text(encoding="utf-8")
needle = '''    assert "@('Проверить и создать (1)', 'Создать документы (1)')" in source\n'''
replacement = '''    assert "@('Проверить и создать (1)', 'Создать документы (1)')" in source\n    assert "[System.Windows.Automation.ControlType]::CheckBox" in source\n    assert "E2 learned document selected for generation" in source\n    assert "E2 learned document checkbox has no TogglePattern." not in source\n'''
if test_text.count(needle) != 1:
    raise SystemExit(f"test assertion point changed: {test_text.count(needle)}")
test_text = test_text.replace(needle, replacement, 1)
test_path.write_text(test_text, encoding="utf-8", newline="\n")
