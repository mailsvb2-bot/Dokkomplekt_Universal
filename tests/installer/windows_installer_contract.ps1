param(
  [string]$BundleDir = "target\release\bundle",
  [string]$TauriConfig = "src-tauri\tauri.conf.json",
  [ValidateSet("", "downloadBootstrapper", "offlineInstaller")]
  [string]$ExpectedWebViewMode = ""
)

$ErrorActionPreference = "Stop"

# Windows PowerShell 5.1 requires a BOM for this Cyrillic installer contract.
# Keep the large, already-proven implementation byte-for-byte immutable and
# apply the narrow E2 commit-boundary interaction patch fail-closed at launch.
$implementationPath = Join-Path $PSScriptRoot 'windows_installer_contract.impl.ps1'
if (-not (Test-Path -LiteralPath $implementationPath -PathType Leaf)) {
  throw "Windows installer contract implementation is missing: $implementationPath"
}
$implementation = Get-Content -LiteralPath $implementationPath -Raw -Encoding UTF8

$anchor = @'
  $e2CreateAction.SetFocus()
  Start-Sleep -Milliseconds 100
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')

  $e2ExpectedFile = "$e2ButtonLabel.docx"
'@
$replacement = @'
  $e2CreateAction.SetFocus()
  Start-Sleep -Milliseconds 100
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')

  # The canonical plan is re-read at the publication boundary. If that refresh
  # introduces output-folder identity fields, the product returns them to the
  # still-open preflight. Fill those real user-facing controls, then explicitly
  # confirm a second time instead of bypassing the naming contract.
  $e2DocumentNumberInput = Wait-UiElement -Description 'E2 refreshed document number' -TimeoutSeconds 30 -Probe {
    $currentAppWindow = Find-LiveAppWindow
    if ($null -eq $currentAppWindow) { return $null }
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      'workflow-document-number'
    )
    $currentAppWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
  }
  Set-UiValue -Element $e2DocumentNumberInput -Value 'E2-7708004767'
  $e2DocumentDateInput = Wait-UiElement -Description 'E2 refreshed document date' -TimeoutSeconds 30 -Probe {
    $currentAppWindow = Find-LiveAppWindow
    if ($null -eq $currentAppWindow) { return $null }
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      'workflow-document-date'
    )
    $currentAppWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
  }
  Set-UiValue -Element $e2DocumentDateInput -Value '18.09.2026'

  $e2CreateAction = Wait-UiElement -Description 'E2 Создать документы after refreshed preflight' -TimeoutSeconds 30 -Probe {
    $currentAppWindow = Find-LiveAppWindow
    if ($null -eq $currentAppWindow) { return $null }
    Find-ReadyButtonByNames -Root $currentAppWindow -Names @('Создать документы')
  }
  $currentAppWindow = Find-LiveAppWindow
  if ($null -eq $currentAppWindow) { throw 'Installed application window disappeared after refreshed E2 preflight.' }
  Activate-LiveAppWindow -Window $currentAppWindow
  if ($e2CreateAction.Current.IsOffscreen -and $e2CreateAction.Current.IsScrollItemPatternAvailable) {
    $scroll = $e2CreateAction.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern)
    $scroll.ScrollIntoView()
    Start-Sleep -Milliseconds 100
  }
  $e2CreateAction.SetFocus()
  Start-Sleep -Milliseconds 100
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')

  $e2ExpectedFile = "$e2ButtonLabel.docx"
'@

$anchorCount = ([regex]::Matches($implementation, [regex]::Escape($anchor))).Count
if ($anchorCount -ne 1) {
  throw "Windows installer E2 launcher expected exactly one commit-boundary anchor, found $anchorCount."
}
$effectiveContract = $implementation.Replace($anchor, $replacement)
$tempContract = Join-Path $env:RUNNER_TEMP "dokkomplekt-windows-installer-contract-$PID.ps1"
[System.IO.File]::WriteAllText(
  $tempContract,
  $effectiveContract,
  [System.Text.UTF8Encoding]::new($true)
)
try {
  $legacyPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  if (-not (Test-Path -LiteralPath $legacyPowerShell -PathType Leaf)) {
    throw "Windows PowerShell 5.1 is unavailable: $legacyPowerShell"
  }
  & $legacyPowerShell -NoProfile -ExecutionPolicy Bypass -File $tempContract -BundleDir $BundleDir -TauriConfig $TauriConfig -ExpectedWebViewMode $ExpectedWebViewMode
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }
} finally {
  Remove-Item -LiteralPath $tempContract -Force -ErrorAction SilentlyContinue
}
