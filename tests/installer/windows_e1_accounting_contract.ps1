param(
  [string]$BundleDir = "target\release\bundle",
  [string]$TauriConfig = "src-tauri\tauri.conf.json",
  [ValidateSet("", "downloadBootstrapper", "offlineInstaller")]
  [string]$ExpectedWebViewMode = ""
)

$ErrorActionPreference = "Stop"
$baseConfig = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$config = Get-Content $TauriConfig -Raw | ConvertFrom-Json
$webViewMode = [string]$config.bundle.windows.webviewInstallMode.type
if ($webViewMode -notin @("downloadBootstrapper", "offlineInstaller")) {
  throw "Unsupported Windows WebView2 installer mode: $webViewMode"
}
if (-not [string]::IsNullOrWhiteSpace($ExpectedWebViewMode) -and $webViewMode -ne $ExpectedWebViewMode) {
  throw "Windows WebView2 installer mode mismatch: expected $ExpectedWebViewMode, got $webViewMode"
}

$installer = Get-ChildItem -Path $BundleDir -Recurse -File -Filter "*.exe" |
  Where-Object { $_.DirectoryName -match "nsis" -and $_.Name -match "setup|Dokkomplekt" } |
  Select-Object -First 1
if (!$installer) { throw "NSIS setup.exe not found under $BundleDir" }

$installDir = Join-Path $env:RUNNER_TEMP "dokkomplekt-e1-accounting-$PID"
Remove-Item -LiteralPath $installDir -Recurse -Force -ErrorAction SilentlyContinue
$install = Start-Process -FilePath $installer.FullName -ArgumentList @("/S", "/D=$installDir") -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "E1 NSIS install failed with exit code $($install.ExitCode)" }

$productName = if (-not [string]::IsNullOrWhiteSpace([string]$config.productName)) { [string]$config.productName } else { [string]$baseConfig.productName }
$appCandidates = @(Get-ChildItem -Path $installDir -Recurse -File -Filter "*.exe" |
  Where-Object { $_.Name -notmatch "uninstall" } |
  Where-Object {
    $info = $_.VersionInfo
    ($info.ProductName -eq $productName) -or ($info.OriginalFilename -eq 'dokkomplekt-tauri.exe') -or
      ($_.Name -in @('Dokkomplekt Universal.exe', 'dokkomplekt-tauri.exe', 'Dokkomplekt.exe'))
  })
if ($appCandidates.Count -ne 1) { throw "E1 expected one installed application executable; found $($appCandidates.Count)" }
$app = $appCandidates[0]

$bundleIdentifier = if (-not [string]::IsNullOrWhiteSpace([string]$config.identifier)) { [string]$config.identifier } else { [string]$baseConfig.identifier }
$roamingAppData = [Environment]::GetFolderPath('ApplicationData')
$appDataRoot = Join-Path $roamingAppData $bundleIdentifier
if (-not (Test-Path -LiteralPath $appDataRoot -PathType Container)) {
  throw 'E1 Accounting lane requires the preceding installed baseline smoke to leave the persisted workspace intact.'
}
$desktopPath = [Environment]::GetFolderPath('Desktop')
$defaultOutputRoot = Join-Path $desktopPath 'Выписанные пациенты'
if (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container)) {
  throw 'E1 Accounting lane requires the canonical Desktop output root from the preceding baseline smoke.'
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class DokkomplektE1NativeMouse {
  [DllImport("user32.dll", SetLastError = true)]
  public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, string lParam);
  [DllImport("user32.dll", EntryPoint = "SendMessageW", SetLastError = true)]
  public static extern IntPtr SendMessagePtr(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder text, int maxCount);
}
"@

$desktop = [System.Windows.Automation.AutomationElement]::RootElement
$process = Start-Process -FilePath $app.FullName -PassThru

function Test-UiaTransientTimeout {
  param([Parameter(Mandatory = $true)]$ErrorRecord)
  return ([string]$ErrorRecord.Exception.Message) -match 'Operation timed out|0x80131505'
}

function Wait-UiElement {
  param(
    [Parameter(Mandatory = $true)][scriptblock]$Probe,
    [Parameter(Mandatory = $true)][string]$Description,
    [int]$TimeoutSeconds = 30
  )
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    try { $element = & $Probe } catch {
      if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
      $element = $null
    }
    if ($null -ne $element) { return $element }
    if ($process.HasExited) { throw "Application exited while waiting for $Description" }
    Start-Sleep -Milliseconds 150
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "E1 UI timeout: $Description"
}

function Find-LiveAppWindow {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    [int]$process.Id
  )
  return $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
}

function Find-ButtonByNames {
  param([Parameter(Mandatory = $true)]$Root, [Parameter(Mandatory = $true)][string[]]$Names)
  foreach ($name in $Names) {
    $condition = [System.Windows.Automation.AndCondition]::new(
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button),
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $name)
    )
    $button = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -ne $button) { return $button }
  }
  return $null
}

function Find-ReadyButtonByNames {
  param([Parameter(Mandatory = $true)]$Root, [Parameter(Mandatory = $true)][string[]]$Names)
  $button = Find-ButtonByNames -Root $Root -Names $Names
  if ($null -ne $button -and $button.Current.IsEnabled) { return $button }
  return $null
}

function Invoke-UiElement {
  param([Parameter(Mandatory = $true)]$Element, [string]$Description = 'UI element')
  try {
    if (-not $Element.Current.IsEnabled) { throw "$Description is disabled" }
    if ($Element.Current.IsOffscreen -and $Element.Current.IsScrollItemPatternAvailable) {
      $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
      Start-Sleep -Milliseconds 100
    }
    if ($Element.Current.IsInvokePatternAvailable) {
      $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
      return
    }
    if ($Element.Current.IsLegacyIAccessiblePatternAvailable) {
      $Element.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern).DoDefaultAction()
      return
    }
    Invoke-UiElementPhysically -Element $Element -Description $Description
  } catch {
    throw "Live E1 UI action failed for '$Description': $($_.Exception.Message)"
  }
}

function Invoke-UiActionFromProbe {
  param([Parameter(Mandatory = $true)][scriptblock]$ActionProbe, [Parameter(Mandatory = $true)][string]$Description)
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  do {
    try { $action = & $ActionProbe } catch {
      if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
      $action = $null
    }
    if ($null -eq $action) { Start-Sleep -Milliseconds 100; continue }
    try {
      Invoke-UiElement -Element $action -Description $Description
      return
    } catch {
      if ([DateTime]::UtcNow -ge $deadline) {
        throw "E1 UI timeout invoking live action: $Description. Last error: $($_.Exception.Message)"
      }
      Start-Sleep -Milliseconds 100
    }
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "E1 UI timeout invoking live action: $Description"
}

function Invoke-UiActionWithObservedTransition {
  param(
    [Parameter(Mandatory = $true)][scriptblock]$ActionProbe,
    [Parameter(Mandatory = $true)][scriptblock]$TransitionProbe,
    [Parameter(Mandatory = $true)][string]$Description,
    [Parameter(Mandatory = $true)][string]$TransitionDescription,
    [int]$TransitionSeconds = 5
  )
  Invoke-UiActionFromProbe -ActionProbe $ActionProbe -Description $Description

  $deadline = [DateTime]::UtcNow.AddSeconds($TransitionSeconds)
  do {
    try { $transition = & $TransitionProbe } catch {
      if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
      $transition = $null
    }
    if ($null -ne $transition) { return $transition }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)

  $retryAction = $null
  $actionStateDeadline = [DateTime]::UtcNow.AddSeconds(2)
  do {
    try { $retryAction = & $ActionProbe } catch {
      if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
      $retryAction = $null
    }
    if ($null -ne $retryAction) { break }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $actionStateDeadline)

  if ($null -eq $retryAction) {
    Write-Host "E1 UI action '$Description' is already in-flight; waiting for '$TransitionDescription' without a duplicate click."
    return Wait-UiElement -Description $TransitionDescription -TimeoutSeconds 30 -Probe $TransitionProbe
  }

  Write-Host "E1 UI action '$Description' produced no observable transition and remains actionable; retrying once with physical input."
  Invoke-UiActionPhysicallyFromProbe -ActionProbe $ActionProbe -Description "$Description physical retry"
  return Wait-UiElement -Description $TransitionDescription -TimeoutSeconds 30 -Probe $TransitionProbe
}

function Invoke-UiElementPhysically {
  param([Parameter(Mandatory = $true)]$Element, [string]$Description = 'UI element')
  if (-not $Element.Current.IsEnabled) { throw "$Description is disabled" }
  if ($Element.Current.IsOffscreen -and $Element.Current.IsScrollItemPatternAvailable) {
    $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
    Start-Sleep -Milliseconds 100
  }
  try {
    $point = $Element.GetClickablePoint()
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$point.X, [int]$point.Y)
    [DokkomplektE1NativeMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [DokkomplektE1NativeMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  } catch {
    $Element.SetFocus()
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  }
}

function Invoke-UiActionPhysicallyFromProbe {
  param([Parameter(Mandatory = $true)][scriptblock]$ActionProbe, [Parameter(Mandatory = $true)][string]$Description)
  $action = Wait-UiElement -Description $Description -Probe $ActionProbe
  Invoke-UiElementPhysically -Element $action -Description $Description
}

function Find-FileDialog {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Window
  )
  foreach ($window in $desktop.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)) {
    $edit = $window.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
    )
    if ($null -ne $edit) { return $window }
  }
  return $null
}

function Set-UiValue {
  param([Parameter(Mandatory = $true)]$Element, [Parameter(Mandatory = $true)][string]$Value)
  $supportsValue = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty,
    $true
  )
  $valueElement = $Element.FindFirst(
    [System.Windows.Automation.TreeScope]::Subtree,
    $supportsValue
  )
  if ($null -ne $valueElement) {
    $valueElement.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
    return
  }

  # Hosted Windows runners can expose the common OpenFileDialog filename control
  # without UIA ValuePattern or focusability. Use the same native WM_SETTEXT path
  # already proven by the baseline installed-app smoke instead of SendKeys.
  $nativeHandle = [IntPtr]$Element.Current.NativeWindowHandle
  if ($nativeHandle -eq [IntPtr]::Zero) {
    throw 'UI value control exposes neither ValuePattern nor a native HWND.'
  }
  $null = [DokkomplektE1NativeMouse]::SendMessage($nativeHandle, 0x000C, [IntPtr]::Zero, $Value)
  Start-Sleep -Milliseconds 200
}

function Get-UiValue {
  param([Parameter(Mandatory = $true)]$Element)
  $supportsValue = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty,
    $true
  )
  $valueElement = $Element.FindFirst(
    [System.Windows.Automation.TreeScope]::Subtree,
    $supportsValue
  )
  if ($null -ne $valueElement) {
    return [string]$valueElement.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
  }

  # WebView2 can expose an input wrapper without ValuePattern while its live
  # accessibility descendant still carries the user-visible value. Read that
  # accessibility value before falling back to a native HWND.
  $supportsLegacyValue = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::IsLegacyIAccessiblePatternAvailableProperty,
    $true
  )
  $legacyElement = $Element.FindFirst(
    [System.Windows.Automation.TreeScope]::Subtree,
    $supportsLegacyValue
  )
  if ($null -ne $legacyElement) {
    $legacy = $legacyElement.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern)
    return [string]$legacy.Current.Value
  }

  # Native edit controls such as the Windows common file dialog can omit both
  # UIA value patterns. Read their visible text directly without forcing focus.
  $nativeHandle = [IntPtr]$Element.Current.NativeWindowHandle
  if ($nativeHandle -ne [IntPtr]::Zero) {
    $length = [DokkomplektE1NativeMouse]::GetWindowTextLength($nativeHandle)
    $builder = [System.Text.StringBuilder]::new([Math]::Max(1, $length + 1))
    $null = [DokkomplektE1NativeMouse]::GetWindowText($nativeHandle, $builder, $builder.Capacity)
    return $builder.ToString()
  }

  throw 'UI value control exposes neither ValuePattern, LegacyIAccessible value, nor a native HWND.'
}

function Normalize-UiValue {
  param([AllowNull()][string]$Value)
  if ($null -eq $Value) { return '' }
  $normalized = $Value -replace [char]0x00A0, ' '
  return ([regex]::Replace($normalized.Trim(), '\s+', ' '))
}

function Submit-OpenFileDialog {
  param([Parameter(Mandatory = $true)]$Dialog)

  # Common OpenFileDialog uses stable IDOK=1 independent of locale. Prefer the
  # exact AutomationId contract used by the main installed smoke instead of
  # depending on the localized button name or transient UIA IsEnabled state.
  $automationId = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
    '1'
  )
  $kind = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Button
  )
  $openButton = $Dialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.AndCondition]::new($automationId, $kind)
  )
  if ($null -ne $openButton) {
    Invoke-UiElement -Element $openButton -Description 'native Open button'
    return
  }

  # Hosted Windows runners can omit the localized Open button from UIA entirely.
  # WM_COMMAND/IDOK confirms the same native common dialog without bypassing the
  # application's real file-picker path.
  $dialogHandle = [IntPtr]$Dialog.Current.NativeWindowHandle
  if ($dialogHandle -eq [IntPtr]::Zero) {
    throw 'OpenFileDialog exposes neither AutomationId=1 nor a native HWND.'
  }
  $null = [DokkomplektE1NativeMouse]::SendMessagePtr(
    $dialogHandle,
    0x0111,
    [IntPtr]1,
    [IntPtr]::Zero
  )
  Start-Sleep -Milliseconds 500
}

function New-AccountingSourceDocx {
  param([Parameter(Mandatory = $true)][string]$Path)
  Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::CreateNew)
  try {
    $archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create, $false)
    try {
      $documentXml = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>' +
        '<w:p><w:r><w:t>АКТ ОКАЗАННЫХ УСЛУГ № E1-17 от 13.09.2026</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Исполнитель: ООО «Альфа»</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Заказчик: ООО «Бета»</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Договор № D-77</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Дата договора: 01.09.2026</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Предмет договора: Консультационные услуги</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Сумма: 125 000,00</w:t></w:r></w:p>' +
        '<w:sectPr/></w:body></w:document>'
      $parts = @{
        '[Content_Types].xml' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'
        '_rels/.rels' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
        'word/document.xml' = $documentXml
      }
      foreach ($name in $parts.Keys) {
        $entry = $archive.CreateEntry($name, [System.IO.Compression.CompressionLevel]::Optimal)
        $writer = [System.IO.StreamWriter]::new($entry.Open(), [System.Text.UTF8Encoding]::new($false))
        try { $writer.Write($parts[$name]) } finally { $writer.Dispose() }
      }
    } finally { $archive.Dispose() }
  } finally { $stream.Dispose() }
}

$appWindow = Wait-UiElement -Description 'installed E1 application window' -TimeoutSeconds 30 -Probe { Find-LiveAppWindow }
$addTemplates = Wait-UiElement -Description 'persisted workspace with Add templates' -TimeoutSeconds 30 -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Добавить шаблоны')
}
if ($null -eq $addTemplates) { throw 'Persisted workspace was not restored for E1 Accounting.' }

$fixtureDir = Join-Path $env:RUNNER_TEMP "dokkomplekt-e1-accounting-fixtures-$PID"
New-Item -ItemType Directory -Force -Path $fixtureDir | Out-Null
$accountingTemplate = (Resolve-Path 'content-packs\tier1-accounting-ru\templates\service_act.docx').Path
$accountingSource = Join-Path $fixtureDir 'бухгалтерский источник без ндс и валюты.docx'
New-AccountingSourceDocx -Path $accountingSource
$accountingLabel = 'Акт оказанных услуг'
$accountingOutputName = "$accountingLabel.docx"
$completionReceiptRoot = Join-Path $appDataRoot 'generation-completion-receipts'

$templateDialog = Invoke-UiActionWithObservedTransition `
  -Description 'Добавить шаблоны for E1 Accounting' `
  -TransitionDescription 'native template picker for E1 Accounting' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Добавить шаблоны')
  } `
  -TransitionProbe { Find-FileDialog }
$templateEdit = $templateDialog.FindFirst(
  [System.Windows.Automation.TreeScope]::Descendants,
  [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
)
Set-UiValue -Element $templateEdit -Value $accountingTemplate
Submit-OpenFileDialog -Dialog $templateDialog

$labelInput = Wait-UiElement -Description 'Accounting template label input' -TimeoutSeconds 40 -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Название документа для service_act.docx')
  )
}
Set-UiValue -Element $labelInput -Value $accountingLabel
Invoke-UiActionPhysicallyFromProbe -Description 'Создать Accounting button' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Создать кнопки (1)')
}
$null = Wait-UiElement -Description 'Accounting document button' -TimeoutSeconds 90 -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ButtonByNames -Root $window -Names @($accountingLabel)
}

$sourceDialog = Invoke-UiActionWithObservedTransition `
  -Description 'Replace source with E1 Accounting source' `
  -TransitionDescription 'native source picker for E1 Accounting' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Заменить исходный файл', 'Выбрать исходный файл')
  } `
  -TransitionProbe { Find-FileDialog }
$sourceEdit = $sourceDialog.FindFirst(
  [System.Windows.Automation.TreeScope]::Descendants,
  [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
)
Set-UiValue -Element $sourceEdit -Value $accountingSource
Submit-OpenFileDialog -Dialog $sourceDialog
$sourceFileName = [System.IO.Path]::GetFileName($accountingSource)
$null = Wait-UiElement -Description 'E1 Accounting source accepted' -TimeoutSeconds 40 -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $sourceFileName)
  )
}

$window = Find-LiveAppWindow
$clearSelection = Find-ReadyButtonByNames -Root $window -Names @('Снять выбор')
if ($null -ne $clearSelection) { Invoke-UiElementPhysically -Element $clearSelection -Description 'clear previous document selection' }
Start-Sleep -Milliseconds 250
Invoke-UiActionPhysicallyFromProbe -Description 'select Accounting service act' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, "Добавить $accountingLabel в комплект")
  )
}
$generationAction = Wait-UiElement -Description 'one-document Accounting generation action' -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Проверить и создать (1)', 'Создать документы (1)')
}
Invoke-UiElementPhysically -Element $generationAction -Description 'open E1 Accounting preflight'
$null = Wait-UiElement -Description 'E1 Accounting preflight' -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Проверка перед созданием')
  )
}

foreach ($requiredMissingId in @('workflow-amount-currency', 'workflow-amount-vat')) {
  $control = (Find-LiveAppWindow).FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $requiredMissingId)
  )
  if ($null -eq $control) { throw "E1 Accounting preflight did not expose required missing field: $requiredMissingId" }
  if (-not [string]::IsNullOrWhiteSpace((Get-UiValue -Element $control))) {
    throw "E1 Accounting source unexpectedly supplied intentionally missing field: $requiredMissingId"
  }
}

# Source-owned values must come from the actual source document. The installed
# proof is not allowed to repair these values through UI automation. When the
# backend exposes a satisfied prompt, verify its current value but never write it.
$sourceOwnedValues = [ordered]@{
  'document.number' = 'E1-17'; 'document.date' = '13.09.2026'; 'org.name' = 'ООО «Альфа»';
  'counterparty.name' = 'ООО «Бета»'; 'contract.number' = 'D-77'; 'contract.date' = '01.09.2026';
  'contract.subject' = 'Консультационные услуги'; 'amount.total' = '125 000,00'
}
foreach ($fieldId in $sourceOwnedValues.Keys) {
  $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
  $control = (Find-LiveAppWindow).FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $automationId)
  )
  if ($null -ne $control) {
    $actual = Normalize-UiValue -Value (Get-UiValue -Element $control)
    $expected = Normalize-UiValue -Value $sourceOwnedValues[$fieldId]
    if ($actual -ne $expected) { throw "E1 source-owned prompt drift for $fieldId`: expected '$expected', got '$actual'" }
  }
}

$docsBeforeBlocked = @(Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $accountingOutputName -ErrorAction SilentlyContinue).Count
$receiptsBeforeBlocked = if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
  @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
} else { 0 }
Invoke-UiActionPhysicallyFromProbe -Description 'deliberately incomplete Accounting Create' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Создать документы')
}
$null = Wait-UiElement -Description 'faulty Accounting draft blocked' -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Документы не созданы')
  )
}
if (@(Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $accountingOutputName -ErrorAction SilentlyContinue).Count -ne $docsBeforeBlocked) {
  throw 'E1 faulty Accounting draft produced a physical DOCX.'
}
$receiptsAfterBlocked = if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
  @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
} else { 0 }
if ($receiptsAfterBlocked -ne $receiptsBeforeBlocked) { throw 'E1 faulty Accounting draft produced a committed receipt.' }
Write-Host 'E1 ADVERSARIAL PASS: incomplete Accounting draft produced 0 DOCX and 0 committed receipts.'

$manualPromptValues = [ordered]@{
  'amount.currency' = 'RUB'; 'amount.vat' = '20 833,33'
}
foreach ($fieldId in $manualPromptValues.Keys) {
  $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
  $control = (Find-LiveAppWindow).FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $automationId)
  )
  if ($null -eq $control) { throw "E1 missing manual completion control: $fieldId" }
  Set-UiValue -Element $control -Value $manualPromptValues[$fieldId]
}
Invoke-UiActionPhysicallyFromProbe -Description 'complete Accounting Create' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Создать документы')
}

# The first adversarial attempt intentionally left an error banner visible.
# Require that stale UI state to clear before treating any later banner as a
# failure of the completed generation attempt.
$staleDeadline = [DateTime]::UtcNow.AddSeconds(15)
$staleFailure = $null
do {
  $window = Find-LiveAppWindow
  $staleFailure = if ($null -eq $window) { $null } else {
    $window.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Документы не созданы')
    )
  }
  if ($null -eq $staleFailure) { break }
  Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $staleDeadline)
if ($null -ne $staleFailure) { throw 'E1 stale blocked-draft error did not clear after completed preflight submission.' }

$deadline = [DateTime]::UtcNow.AddSeconds(60)
$accountingDoc = $null
while ($null -eq $accountingDoc -and [DateTime]::UtcNow -lt $deadline) {
  $accountingDoc = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $accountingOutputName -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -eq $accountingDoc) {
    $failure = (Find-LiveAppWindow).FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Документы не созданы')
    )
    if ($null -ne $failure) { throw 'E1 Accounting backend rejected the completed preflight.' }
    Start-Sleep -Milliseconds 250
  }
}
if ($null -eq $accountingDoc) { throw 'E1 Accounting did not publish a physical DOCX.' }

$archive = [System.IO.Compression.ZipFile]::OpenRead($accountingDoc.FullName)
try {
  $entry = $archive.GetEntry('word/document.xml')
  if ($null -eq $entry) { throw 'E1 Accounting output is not a readable DOCX package.' }
  $reader = [System.IO.StreamReader]::new($entry.Open(), [System.Text.Encoding]::UTF8)
  try { $xml = $reader.ReadToEnd() } finally { $reader.Dispose() }
  foreach ($value in @('E1-17', '13.09.2026', 'ООО «Альфа»', 'ООО «Бета»', 'D-77', '01.09.2026', 'Консультационные услуги', 'RUB', '20 833,33')) {
    if ($xml -notmatch [regex]::Escape($value)) { throw "E1 physical read-back is missing: $value" }
  }
  if ($xml -notmatch '125[\s\u00A0]000,00') { throw 'E1 physical read-back is missing amount.total.' }
  if ($xml -match '\{\{') { throw 'E1 physical read-back contains unresolved placeholders.' }
} finally { $archive.Dispose() }

$outputHash = (Get-FileHash -LiteralPath $accountingDoc.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$receiptDeadline = [DateTime]::UtcNow.AddSeconds(30)
$matchingReceipt = $null
while ($null -eq $matchingReceipt -and [DateTime]::UtcNow -lt $receiptDeadline) {
  if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
    foreach ($file in @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue)) {
      try {
        $raw = Get-Content -LiteralPath $file.FullName -Raw
        $data = $raw | ConvertFrom-Json
        if ($data.output_sha256 -eq $outputHash) {
          $matchingReceipt = [pscustomobject]@{ Data = $data; Raw = $raw }
          break
        }
      } catch { }
    }
  }
  if ($null -eq $matchingReceipt) { Start-Sleep -Milliseconds 250 }
}
if ($null -eq $matchingReceipt) { throw 'E1 physical DOCX has no matching committed GenerationReceipt.' }
if ($matchingReceipt.Data.schema -ne 1 -or $matchingReceipt.Data.status -ne 'committed') { throw 'E1 receipt schema/status mismatch.' }
if ($matchingReceipt.Data.receipt_id -notmatch '^[0-9a-f]{64}$' -or $matchingReceipt.Data.output_id -notmatch '^[0-9a-f]{64}$') { throw 'E1 receipt identities are not opaque SHA-256 values.' }
if ($matchingReceipt.Data.proof_contract -ne 'physical-output-sha256-v1' -or $matchingReceipt.Data.verifier_contract -ne 'published-readback-v1') { throw 'E1 receipt proof/verifier contract mismatch.' }
if ([string]$matchingReceipt.Data.plan_binding_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'E1 committed receipt is not bound to the frozen source/case/plan/template identity.' }
foreach ($forbidden in @($defaultOutputRoot, $sourceFileName, 'ООО «Альфа»', 'ООО «Бета»', 'D-77')) {
  if ($matchingReceipt.Raw.Contains($forbidden)) { throw "E1 receipt leaked path/user data: $forbidden" }
}
$receiptsAfterSuccess = @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
if ($receiptsAfterSuccess -ne ($receiptsBeforeBlocked + 1)) { throw 'E1 successful Accounting publication did not add exactly one committed receipt.' }
Write-Host "E1 INSTALLED PASS: Accounting source -> UI -> physical DOCX -> committed receipt: $($accountingDoc.FullName)"

Stop-Process -Id $process.Id -Force
$process.WaitForExit()
$uninstaller = Get-ChildItem -Path $installDir -Recurse -File -Filter '*.exe' | Where-Object { $_.Name -match 'uninstall' } | Select-Object -First 1
if (!$uninstaller) { throw 'E1 NSIS uninstaller missing.' }
$uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "E1 NSIS uninstall failed with exit code $($uninstall.ExitCode)" }
