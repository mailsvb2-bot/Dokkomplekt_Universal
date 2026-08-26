param(
  [string]$BundleDir = "target\release\bundle",
  [string]$TauriConfig = "src-tauri\tauri.conf.json",
  [ValidateSet("", "downloadBootstrapper", "offlineInstaller")]
  [string]$ExpectedWebViewMode = ""
)

$ErrorActionPreference = "Stop"
$adversarial = $env:DOKKOMPLEKT_ADVERSARIAL -eq '1'
$baseConfig = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$config = Get-Content $TauriConfig -Raw | ConvertFrom-Json
$webViewMode = [string]$config.bundle.windows.webviewInstallMode.type
if ($webViewMode -notin @("downloadBootstrapper", "offlineInstaller")) {
  throw "Unsupported Windows WebView2 installer mode: $webViewMode"
}
if (-not [string]::IsNullOrWhiteSpace($ExpectedWebViewMode) -and $webViewMode -ne $ExpectedWebViewMode) {
  throw "Windows WebView2 installer mode mismatch: expected $ExpectedWebViewMode, got $webViewMode"
}
$targets = if ($null -ne $config.bundle.targets) { @($config.bundle.targets) } else { @($baseConfig.bundle.targets) }
if ($targets -notcontains "nsis") {
  throw "NSIS target is not enabled"
}
if (!(Test-Path $BundleDir)) { throw "Bundle directory not found: $BundleDir" }

$installer = Get-ChildItem -Path $BundleDir -Recurse -File -Filter "*.exe" |
  Where-Object { $_.DirectoryName -match "nsis" -and $_.Name -match "setup|Dokkomplekt" } |
  Select-Object -First 1
if (!$installer) { throw "NSIS setup.exe not found under the nsis bundle directory" }

$installDir = Join-Path $env:RUNNER_TEMP "dokkomplekt-installer-smoke-$PID"
Remove-Item $installDir -Recurse -Force -ErrorAction SilentlyContinue

# NSIS requires /D to be the final argument. A path without spaces avoids quoting ambiguity.
$install = Start-Process -FilePath $installer.FullName -ArgumentList @("/S", "/D=$installDir") -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "NSIS silent install failed with exit code $($install.ExitCode)" }
if (!(Test-Path $installDir)) { throw "NSIS completed but install directory was not created: $installDir" }

$productName = if (-not [string]::IsNullOrWhiteSpace([string]$config.productName)) {
  [string]$config.productName
} else {
  [string]$baseConfig.productName
}
$appCandidates = @(Get-ChildItem -Path $installDir -Recurse -File -Filter "*.exe" |
  Where-Object { $_.Name -notmatch "uninstall" } |
  Where-Object {
    $info = $_.VersionInfo
    $productMatches = -not [string]::IsNullOrWhiteSpace($productName) -and $info.ProductName -eq $productName
    $originalMatches = $info.OriginalFilename -eq 'dokkomplekt-tauri.exe'
    $nameMatches = $_.Name -in @('Dokkomplekt Universal.exe', 'dokkomplekt-tauri.exe', 'Dokkomplekt.exe')
    $productMatches -or $originalMatches -or $nameMatches
  })
if ($appCandidates.Count -ne 1) {
  $names = ($appCandidates | ForEach-Object FullName) -join '; '
  throw "Expected exactly one installed Dokkomplekt application executable; found $($appCandidates.Count): $names"
}
$app = $appCandidates[0]
if (-not [string]::IsNullOrWhiteSpace($productName) -and
    -not [string]::IsNullOrWhiteSpace($app.VersionInfo.ProductName) -and
    $app.VersionInfo.ProductName -ne $productName) {
  throw "Installed executable product name mismatch: $($app.VersionInfo.ProductName)"
}
if ($env:DOKKOMPLEKT_REQUIRE_AUTHENTICODE -eq '1') {
  $installerSignature = Get-AuthenticodeSignature -FilePath $installer.FullName
  if ($installerSignature.Status -ne 'Valid') { throw "Installer signature is invalid: $($installerSignature.Status)" }
  $appSignature = Get-AuthenticodeSignature -FilePath $app.FullName
  if ($appSignature.Status -ne 'Valid') { throw "Installed application signature is invalid: $($appSignature.Status)" }
}

# Donor-derived installed-app contract: the canonical Desktop output root must
# physically exist before the user creates the first document. Remove it first
# so this smoke proves the installed application recreates it, not a test fixture.
$desktopPath = [Environment]::GetFolderPath('Desktop')
if ([string]::IsNullOrWhiteSpace($desktopPath)) { throw "Windows Desktop path is unavailable" }
New-Item -ItemType Directory -Force -Path $desktopPath | Out-Null
$defaultOutputRoot = Join-Path $desktopPath 'Выписанные пациенты'
Remove-Item -LiteralPath $defaultOutputRoot -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $defaultOutputRoot) {
  throw "Could not remove the pre-existing Desktop output root before launch smoke: $defaultOutputRoot"
}

$process = Start-Process -FilePath $app.FullName -PassThru
Start-Sleep -Seconds 5
if ($process.HasExited) {
  $earlyExitCode = $process.ExitCode
  throw "Installed application exited early during launch smoke with code $earlyExitCode"
}

$outputDeadline = [DateTime]::UtcNow.AddSeconds(20)
while (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container) -and [DateTime]::UtcNow -lt $outputDeadline) {
  if ($process.HasExited) {
    throw "Installed application exited before creating the canonical Desktop output root"
  }
  Start-Sleep -Milliseconds 250
}
if (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container)) {
  throw "Installed application did not create the canonical Desktop output root: $defaultOutputRoot"
}
Write-Host "Desktop output root created by installed application: $defaultOutputRoot"

# Product-level first-run proof: invoke the real WebView button and require the
# native Windows OpenFileDialog to appear. A browser-only mock cannot prove this.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class DokkomplektNativeMouse {
  [DllImport("user32.dll", SetLastError = true)]
  public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, string lParam);
  [DllImport("user32.dll", EntryPoint = "SendMessageW", SetLastError = true)]
  public static extern IntPtr SendMessagePtr(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
}
"@

function Wait-UiElement {
  param(
    [Parameter(Mandatory = $true)][scriptblock]$Probe,
    [Parameter(Mandatory = $true)][string]$Description,
    [int]$TimeoutSeconds = 25
  )
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    $element = & $Probe
    if ($null -ne $element) { return $element }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "UI smoke timeout: $Description"
}

function Invoke-UiElement {
  param([Parameter(Mandatory = $true)]$Element)
  try {
    $pattern = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
    return
  } catch {
    $point = $Element.GetClickablePoint()
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$point.X, [int]$point.Y)
    [DokkomplektNativeMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [DokkomplektNativeMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  }
}


function New-PlainDocxFixture {
  param([Parameter(Mandatory = $true)][string]$Path)
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::CreateNew)
  try {
    $archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create, $false)
    try {
      $parts = @{
        '[Content_Types].xml' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'
        '_rels/.rels' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
        'word/document.xml' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Проверочная кнопка</w:t></w:r></w:p><w:p><w:r><w:t>Обычный статический шаблон без технической разметки.</w:t></w:r></w:p><w:sectPr/></w:body></w:document>'
      }
      foreach ($name in $parts.Keys) {
        $entry = $archive.CreateEntry($name, [System.IO.Compression.CompressionLevel]::Optimal)
        $writer = [System.IO.StreamWriter]::new($entry.Open(), [System.Text.UTF8Encoding]::new($false))
        try { $writer.Write($parts[$name]) } finally { $writer.Dispose() }
      }
    } finally {
      $archive.Dispose()
    }
  } finally {
    $stream.Dispose()
  }
}

function Find-ButtonByNames {
  param(
    [Parameter(Mandatory = $true)]$Root,
    [Parameter(Mandatory = $true)][string[]]$Names
  )
  foreach ($candidate in $Names) {
    $name = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      $candidate
    )
    $kind = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Button
    )
    $found = $Root.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.AndCondition]::new($name, $kind)
    )
    if ($null -ne $found) { return $found }
  }
  return $null
}

function Set-UiValue {
  param(
    [Parameter(Mandatory = $true)]$Element,
    [Parameter(Mandatory = $true)][string]$Value
  )
  $supportsValue = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty,
    $true
  )
  $valueElement = $Element.FindFirst(
    [System.Windows.Automation.TreeScope]::Subtree,
    $supportsValue
  )
  if ($null -ne $valueElement) {
    $pattern = $valueElement.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    $pattern.SetValue($Value)
    return
  }

  # The modern Windows OpenFileDialog exposes the file-name ComboBox without
  # UIA ValuePattern on hosted runners. Set its native window text directly.
  $nativeHandle = [IntPtr]$Element.Current.NativeWindowHandle
  if ($nativeHandle -eq [IntPtr]::Zero) {
    throw 'OpenFileDialog file-name control exposes neither ValuePattern nor a native HWND.'
  }
  $null = [DokkomplektNativeMouse]::SendMessage($nativeHandle, 0x000C, [IntPtr]::Zero, $Value)
  Start-Sleep -Milliseconds 200
}

function Wait-FileDialog {
  param([Parameter(Mandatory = $true)][string]$Description)

  return Wait-UiElement -Description $Description -TimeoutSeconds 30 -Probe {
    $fileNameCondition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      '1148'
    )
    $windows = $desktop.FindAll(
      [System.Windows.Automation.TreeScope]::Children,
      [System.Windows.Automation.Condition]::TrueCondition
    )
    foreach ($candidate in $windows) {
      $fileNameControl = $candidate.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        $fileNameCondition
      )
      if ($null -ne $fileNameControl) { return $candidate }
    }
    return $null
  }
}

function Submit-OpenFileDialog {
  param([Parameter(Mandatory = $true)]$Dialog)

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
    Invoke-UiElement -Element $openButton
    return
  }

  # Hosted Windows runners may hide the localized Open button from UIA.
  # IDOK=1 is the stable native command for confirming a common dialog.
  $dialogHandle = [IntPtr]$Dialog.Current.NativeWindowHandle
  if ($dialogHandle -eq [IntPtr]::Zero) {
    throw 'OpenFileDialog exposes neither AutomationId=1 nor a native HWND.'
  }
  $null = [DokkomplektNativeMouse]::SendMessagePtr(
    $dialogHandle,
    0x0111,
    [IntPtr]1,
    [IntPtr]::Zero
  )
  Start-Sleep -Milliseconds 500
}

$desktop = [System.Windows.Automation.AutomationElement]::RootElement
$appWindow = Wait-UiElement -Description 'installed Dokkomplekt window' -Probe {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    [int]$process.Id
  )
  $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
}
if ($adversarial) {
  $secondProcess = Start-Process -FilePath $app.FullName -PassThru
  $secondDeadline = [DateTime]::UtcNow.AddSeconds(12)
  while (-not $secondProcess.HasExited -and [DateTime]::UtcNow -lt $secondDeadline) {
    Start-Sleep -Milliseconds 250
  }
  if (-not $secondProcess.HasExited) {
    Stop-Process -Id $secondProcess.Id -Force -ErrorAction SilentlyContinue
    throw 'Adversarial single-instance check failed: second UI process remained alive.'
  }
  if ($process.HasExited) { throw 'Adversarial single-instance check killed the primary UI process.' }
  Write-Host 'ADVERSARIAL OK: second launch exited and primary UI stayed alive.'
}

# Confirm the first-run output naming rule before exercising generation. The
# default rule is deterministic: document number + document date.
$saveFolderRule = Find-ButtonByNames -Root $appWindow -Names @('Сохранить папку и правило')
if ($null -ne $saveFolderRule) {
  Invoke-UiElement -Element $saveFolderRule
  Write-Host 'Default output folder and subfolder naming rule confirmed.'
}

$createButton = Wait-UiElement -Description 'Создать свои кнопки button' -Probe {
  $name = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    'Создать свои кнопки'
  )
  $kind = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Button
  )
  $condition = [System.Windows.Automation.AndCondition]::new($name, $kind)
  $appWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}
Invoke-UiElement -Element $createButton

$templateDialog = Wait-UiElement -Description 'native Word template picker' -Probe {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    'Выберите шаблоны Word'
  )
  $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
}

# Create button from a real unmarked DOCX through the installed application's native picker.
if ($adversarial) {
  $fixtureDir = Join-Path $env:RUNNER_TEMP 'Документы с пробелами'
  New-Item -ItemType Directory -Force -Path $fixtureDir | Out-Null
  $plainTemplate = Join-Path $fixtureDir 'исходник проверка № 1.docx'
} else {
  $plainTemplate = Join-Path $env:RUNNER_TEMP 'button-smoke.docx'
}
New-PlainDocxFixture -Path $plainTemplate
$fileNameEdit = Wait-UiElement -Description 'OpenFileDialog file name field' -Probe {
  $automationId = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
    '1148'
  )
  $templateDialog.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $automationId)
}
Set-UiValue -Element $fileNameEdit -Value $plainTemplate
Submit-OpenFileDialog -Dialog $templateDialog
Write-Host 'Native first-run template picker OK: real DOCX selected.'

$createPreparedButton = Wait-UiElement -Description 'Создать кнопки (1) button' -TimeoutSeconds 40 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Создать кнопки (1)')
}
Invoke-UiElement -Element $createPreparedButton

$createdDocumentButton = Wait-UiElement -Description 'created static template button' -TimeoutSeconds 40 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Проверочная кнопка')
}
if ($null -eq $createdDocumentButton) { throw 'The real plain DOCX did not become a document button.' }
Write-Host 'Create button from a real unmarked DOCX OK.'

# A template is not a case source. Exercise the installed source picker separately
# so the generation stage is reached through the same order as a real user.
$sourceButton = Wait-UiElement -Description 'Выбрать исходный файл button' -TimeoutSeconds 30 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Выбрать исходный файл')
}
Invoke-UiElement -Element $sourceButton
$sourceDialog = Wait-FileDialog -Description 'native source file picker'
$sourceFileNameEdit = Wait-UiElement -Description 'source OpenFileDialog file name field' -Probe {
  $automationId = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
    '1148'
  )
  $sourceDialog.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $automationId)
}
Set-UiValue -Element $sourceFileNameEdit -Value $plainTemplate
Submit-OpenFileDialog -Dialog $sourceDialog
$sourceAccepted = Wait-UiElement -Description 'Источник принят after native source selection' -TimeoutSeconds 40 -Probe {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    'Источник принят'
  )
  $appWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}
if ($null -eq $sourceAccepted) { throw 'Installed application did not accept the real source DOCX.' }
Write-Host 'Real source DOCX accepted by installed application.'

if ($adversarial) {
  # A failed replacement must never erase the already accepted good source.
  $brokenSource = Join-Path $fixtureDir 'повреждённый источник.docx'
  [System.IO.File]::WriteAllText($brokenSource, 'this is deliberately not a DOCX archive')
  $replaceSource = Wait-UiElement -Description 'Заменить исходный файл button' -Probe {
    Find-ButtonByNames -Root $appWindow -Names @('Заменить исходный файл')
  }
  Invoke-UiElement -Element $replaceSource
  $brokenDialog = Wait-FileDialog -Description 'native picker for broken source'
  $brokenEdit = $brokenDialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
  )
  Set-UiValue -Element $brokenEdit -Value $brokenSource
  Submit-OpenFileDialog -Dialog $brokenDialog
  Start-Sleep -Seconds 2
  if ($process.HasExited) { throw 'Application crashed after a corrupt DOCX replacement.' }
  $acceptedAfterBroken = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Источник принят')
  )
  if ($null -eq $acceptedAfterBroken) { throw 'Corrupt replacement erased the previously accepted source state.' }
  Write-Host 'ADVERSARIAL OK: corrupt DOCX rejected without losing previous source.'

  # Cancelling the native picker is a no-op, not a destructive source reset.
  $replaceSource = Find-ButtonByNames -Root $appWindow -Names @('Заменить исходный файл')
  Invoke-UiElement -Element $replaceSource
  $cancelDialog = Wait-FileDialog -Description 'native picker cancellation'
  $cancelHandle = [IntPtr]$cancelDialog.Current.NativeWindowHandle
  if ($cancelHandle -eq [IntPtr]::Zero) { throw 'Cancellation dialog has no native HWND.' }
  $null = [DokkomplektNativeMouse]::SendMessagePtr($cancelHandle, 0x0111, [IntPtr]2, [IntPtr]::Zero)
  Start-Sleep -Seconds 1
  if ($process.HasExited) { throw 'Application crashed after native picker cancellation.' }
  $acceptedAfterCancel = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Источник принят')
  )
  if ($null -eq $acceptedAfterCancel) { throw 'Native picker cancellation erased the accepted source.' }
  Write-Host 'ADVERSARIAL OK: source picker cancellation preserved current case.'

  # Oversized source is rejected before byte loading and must preserve current case.
  $oversizedSource = Join-Path $fixtureDir 'слишком большой источник.docx'
  $oversizedStream = [System.IO.File]::Open($oversizedSource, [System.IO.FileMode]::Create)
  try { $oversizedStream.SetLength(101MB) } finally { $oversizedStream.Dispose() }
  $replaceSource = Find-ButtonByNames -Root $appWindow -Names @('Заменить исходный файл')
  Invoke-UiElement -Element $replaceSource
  $oversizedDialog = Wait-FileDialog -Description 'native picker for oversized source'
  $oversizedEdit = $oversizedDialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
  )
  Set-UiValue -Element $oversizedEdit -Value $oversizedSource
  Submit-OpenFileDialog -Dialog $oversizedDialog
  Start-Sleep -Seconds 2
  if ($process.HasExited) { throw 'Application crashed after oversized source selection.' }
  $acceptedAfterOversized = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Источник принят')
  )
  if ($null -eq $acceptedAfterOversized) { throw 'Oversized replacement erased the previously accepted source.' }
  Write-Host 'ADVERSARIAL OK: >100MB source rejected without losing previous source.'
}

# End-to-end installed generation proof: select the real created button, open the
# real preflight, fill deterministic folder fields when the backend asks for them,
# click Create, then require a physical readable DOCX in the Desktop output subfolder.
$selectAllButton = Wait-UiElement -Description 'Выбрать всё button' -TimeoutSeconds 30 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Выбрать всё')
}
Invoke-UiElement -Element $selectAllButton

$preflightButton = Wait-UiElement -Description 'generation action for one selected document' -TimeoutSeconds 40 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Проверить и создать (1)', 'Создать документы (1)')
}
Invoke-UiElement -Element $preflightButton

$preflightTitle = Wait-UiElement -Description 'Проверка перед созданием dialog' -TimeoutSeconds 30 -Probe {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    'Проверка перед созданием'
  )
  $appWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}
if ($null -eq $preflightTitle) { throw 'Generation action did not open the real preflight.' }

$smokeNumber = "WIN-SMOKE-$PID"
$numberCondition = [System.Windows.Automation.PropertyCondition]::new(
  [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
  'workflow-document-number'
)
$dateCondition = [System.Windows.Automation.PropertyCondition]::new(
  [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
  'workflow-document-date'
)
$numberInput = $appWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $numberCondition)
$dateInput = $appWindow.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $dateCondition)
if ($null -ne $numberInput) { Set-UiValue -Element $numberInput -Value $smokeNumber }
if ($null -ne $dateInput) { Set-UiValue -Element $dateInput -Value '26.08.2026' }

$generateButton = Wait-UiElement -Description 'Создать документы button' -TimeoutSeconds 30 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Создать документы')
}
Invoke-UiElement -Element $generateButton

$createdDeadline = [DateTime]::UtcNow.AddSeconds(60)
$createdDoc = $null
do {
  if ($process.HasExited) { throw 'Installed application exited during real document generation smoke.' }
  $createdDoc = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter 'Проверочная кнопка.docx' -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($null -eq $createdDoc) { Start-Sleep -Milliseconds 500 }
} while ($null -eq $createdDoc -and [DateTime]::UtcNow -lt $createdDeadline)
if ($null -eq $createdDoc) {
  throw "Installed application did not physically create Проверочная кнопка.docx under $defaultOutputRoot"
}
if ($createdDoc.Length -le 0) { throw "Created DOCX is empty: $($createdDoc.FullName)" }
$createdArchive = [System.IO.Compression.ZipFile]::OpenRead($createdDoc.FullName)
try {
  $documentEntry = $createdArchive.GetEntry('word/document.xml')
  if ($null -eq $documentEntry) { throw "Created file is not a readable Word DOCX: $($createdDoc.FullName)" }
  $reader = [System.IO.StreamReader]::new($documentEntry.Open(), [System.Text.Encoding]::UTF8)
  try { $createdXml = $reader.ReadToEnd() } finally { $reader.Dispose() }
  if ($createdXml -notmatch 'Проверочная кнопка') { throw 'Created DOCX lost the template content.' }
} finally {
  $createdArchive.Dispose()
}
Write-Host "Installed end-to-end document generation OK: $($createdDoc.FullName)"

if ($adversarial) {
  # Repeating the same deterministic output must not overwrite the first kit.
  $repeatAction = Wait-UiElement -Description 'repeat generation action' -TimeoutSeconds 30 -Probe {
    Find-ButtonByNames -Root $appWindow -Names @('Проверить и создать (1)', 'Создать документы (1)')
  }
  Invoke-UiElement -Element $repeatAction
  $repeatPreflight = Wait-UiElement -Description 'repeat preflight' -TimeoutSeconds 30 -Probe {
    $appWindow.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Проверка перед созданием')
    )
  }
  $repeatGenerate = Wait-UiElement -Description 'repeat Создать документы' -TimeoutSeconds 30 -Probe {
    Find-ButtonByNames -Root $appWindow -Names @('Создать документы')
  }
  Invoke-UiElement -Element $repeatGenerate
  $otherVariants = Wait-UiElement -Description 'existing-kit Другие варианты' -TimeoutSeconds 30 -Probe {
    Find-ButtonByNames -Root $appWindow -Names @('Другие варианты')
  }
  Invoke-UiElement -Element $otherVariants
  $newVersion = Wait-UiElement -Description 'Создать новую версию' -TimeoutSeconds 30 -Probe {
    Find-ButtonByNames -Root $appWindow -Names @('Создать новую версию')
  }
  Invoke-UiElement -Element $newVersion
  $versionDeadline = [DateTime]::UtcNow.AddSeconds(60)
  do {
    $versionDocs = @(Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter 'Проверочная кнопка.docx' -ErrorAction SilentlyContinue)
    if ($versionDocs.Count -lt 2) { Start-Sleep -Milliseconds 500 }
  } while ($versionDocs.Count -lt 2 -and [DateTime]::UtcNow -lt $versionDeadline)
  if ($versionDocs.Count -lt 2) { throw 'Repeat generation did not publish a second version without overwrite.' }
  $distinctFolders = @($versionDocs | ForEach-Object DirectoryName | Sort-Object -Unique)
  if ($distinctFolders.Count -lt 2) { throw 'Repeat generation overwrote the original output folder.' }
  Write-Host "ADVERSARIAL OK: collision created a second version in a distinct folder ($($distinctFolders.Count) folders)."
}

Stop-Process -Id $process.Id -Force
$process.WaitForExit()
Start-Sleep -Seconds 1
if ($adversarial) {
  Remove-Item -LiteralPath $defaultOutputRoot -Recurse -Force -ErrorAction SilentlyContinue
  [System.IO.File]::WriteAllText($defaultOutputRoot, 'deliberate path collision')
  $blockedProcess = Start-Process -FilePath $app.FullName -PassThru
  $blockedWindow = Wait-UiElement -Description 'window with output-root collision' -TimeoutSeconds 30 -Probe {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$blockedProcess.Id
    )
    $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
  }
  $recoveryAlert = Wait-UiElement -Description 'visible output-root recovery alert' -TimeoutSeconds 30 -Probe {
    $blockedWindow.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        'Не удалось подготовить папку готовых документов'
      )
    )
  }
  if ($blockedProcess.HasExited) { throw 'Output-root path collision crashed the application.' }
  if (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Leaf)) {
    throw 'Application silently replaced the deliberate output-root collision file.'
  }
  Write-Host 'ADVERSARIAL OK: output-root collision stayed fail-closed and visible.'
  Stop-Process -Id $blockedProcess.Id -Force
  $blockedProcess.WaitForExit()
  Remove-Item -LiteralPath $defaultOutputRoot -Force
}
$process = Start-Process -FilePath $app.FullName -PassThru
$appWindow = Wait-UiElement -Description 'restarted installed Dokkomplekt window' -TimeoutSeconds 30 -Probe {
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    [int]$process.Id
  )
  $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
}
if ($adversarial) {
  $rootRecoveryDeadline = [DateTime]::UtcNow.AddSeconds(20)
  while (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container) -and [DateTime]::UtcNow -lt $rootRecoveryDeadline) {
    Start-Sleep -Milliseconds 250
  }
  if (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container)) {
    throw 'Application did not recreate Desktop output root after collision was removed.'
  }
  Write-Host 'ADVERSARIAL OK: Desktop output root recovered on clean restart.'
}
$persistedButton = Wait-UiElement -Description 'persisted template button after restart' -TimeoutSeconds 30 -Probe {
  Find-ButtonByNames -Root $appWindow -Names @('Проверочная кнопка')
}
if ($null -eq $persistedButton) { throw 'Created template button was lost after application restart.' }
Write-Host 'Persisted template button survived application restart.'

Stop-Process -Id $process.Id -Force
$process.WaitForExit()

$uninstaller = Get-ChildItem -Path $installDir -Recurse -File -Filter "*.exe" |
  Where-Object { $_.Name -match "uninstall" } |
  Select-Object -First 1
if (!$uninstaller) { throw "NSIS uninstaller was not created" }
$uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "NSIS silent uninstall failed with exit code $($uninstall.ExitCode)" }

Write-Host "Windows installer validation OK ($webViewMode): installed, remained alive, and uninstalled $($installer.FullName)"
