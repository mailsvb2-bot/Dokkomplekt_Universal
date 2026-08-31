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
$bundleIdentifier = if (-not [string]::IsNullOrWhiteSpace([string]$config.identifier)) {
  [string]$config.identifier
} else {
  [string]$baseConfig.identifier
}
if ([string]::IsNullOrWhiteSpace($bundleIdentifier)) {
  throw 'Tauri bundle identifier is unavailable; cannot isolate installed-app state.'
}
if ($env:DOKKOMPLEKT_REQUIRE_AUTHENTICODE -eq '1') {
  $installerSignature = Get-AuthenticodeSignature -FilePath $installer.FullName
  if ($installerSignature.Status -ne 'Valid') { throw "Installer signature is invalid: $($installerSignature.Status)" }
  $appSignature = Get-AuthenticodeSignature -FilePath $app.FullName
  if ($appSignature.Status -ne 'Valid') { throw "Installed application signature is invalid: $($appSignature.Status)" }
}

# This is explicitly a first-run contract. Tauri's Windows app_data_dir is
# %APPDATA%/<bundle identifier>; clear that exact application-owned state so a
# previous compile/test process on the same packaging runner cannot turn this
# into an accidental persisted-user restart. The runner itself is ephemeral.
$roamingAppData = [Environment]::GetFolderPath('ApplicationData')
if ([string]::IsNullOrWhiteSpace($roamingAppData)) { throw 'Windows roaming AppData path is unavailable' }
$appDataRoot = Join-Path $roamingAppData $bundleIdentifier
Remove-Item -LiteralPath $appDataRoot -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $appDataRoot) {
  throw "Could not clear Dokkomplekt app data before first-run smoke: $appDataRoot"
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

$appStdout = Join-Path $env:RUNNER_TEMP "dokkomplekt-installed-app-$PID.stdout.log"
$appStderr = Join-Path $env:RUNNER_TEMP "dokkomplekt-installed-app-$PID.stderr.log"
Remove-Item -LiteralPath $appStdout, $appStderr -Force -ErrorAction SilentlyContinue
$process = Start-Process -FilePath $app.FullName -RedirectStandardOutput $appStdout -RedirectStandardError $appStderr -PassThru

function Write-AppLaunchDiagnostics {
  Write-Host "Installed app path: $($app.FullName)"
  Write-Host "Known Desktop path: $desktopPath"
  Write-Host "Tauri app data root: $appDataRoot"
  Write-Host "LOCALAPPDATA: $env:LOCALAPPDATA"
  if (Test-Path -LiteralPath $appStdout) { Write-Host "--- installed app stdout ---"; Get-Content -LiteralPath $appStdout -ErrorAction SilentlyContinue | Write-Host }
  if (Test-Path -LiteralPath $appStderr) { Write-Host "--- installed app stderr ---"; Get-Content -LiteralPath $appStderr -ErrorAction SilentlyContinue | Write-Host }
}

Start-Sleep -Seconds 5
if ($process.HasExited) {
  $earlyExitCode = $process.ExitCode
  Write-AppLaunchDiagnostics
  throw "Installed application exited early during launch smoke with code $earlyExitCode"
}

# The canonical thin installer reaches native setup before UI automation starts.
# Keep the first-run invariant tightly bounded: a live process without the real
# Desktop output root is still a product failure, not a reason to wait longer.
$coldStartDeadlineSeconds = 20
$outputDeadline = [DateTime]::UtcNow.AddSeconds($coldStartDeadlineSeconds)
while (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container) -and [DateTime]::UtcNow -lt $outputDeadline) {
  if ($process.HasExited) {
    throw "Installed application exited before creating the canonical Desktop output root"
  }
  Start-Sleep -Milliseconds 250
}
if (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Container)) {
  Write-AppLaunchDiagnostics
  throw "Installed application did not create the canonical Desktop output root within $coldStartDeadlineSeconds seconds: $defaultOutputRoot"
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

function Test-UiaTransientTimeout {
  param([Parameter(Mandatory = $true)]$ErrorRecord)
  $message = [string]$ErrorRecord.Exception.Message
  return $message -match 'Operation timed out|0x80131505'
}

function Wait-UiElement {
  param(
    [Parameter(Mandatory = $true)][scriptblock]$Probe,
    [Parameter(Mandatory = $true)][string]$Description,
    [int]$TimeoutSeconds = 25
  )
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    try {
      $element = & $Probe
    } catch {
      if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
      $element = $null
    }
    if ($null -ne $element) { return $element }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "UI smoke timeout: $Description"
}

function Invoke-UiElement {
  param([Parameter(Mandatory = $true)]$Element)
  $deadline = [DateTime]::UtcNow.AddSeconds(5)
  do {
    try {
      if (-not $Element.Current.IsEnabled) {
        Start-Sleep -Milliseconds 150
        continue
      }
      if ($Element.Current.IsOffscreen -and $Element.Current.IsScrollItemPatternAvailable) {
        $scroll = $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern)
        $scroll.ScrollIntoView()
        Start-Sleep -Milliseconds 150
      }
      if ($Element.Current.IsInvokePatternAvailable) {
        $pattern = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return
      }
      if ($Element.Current.IsLegacyIAccessiblePatternAvailable) {
        $legacy = $Element.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern)
        $legacy.DoDefaultAction()
        return
      }
      try {
        $point = $Element.GetClickablePoint()
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$point.X, [int]$point.Y)
        [DokkomplektNativeMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [DokkomplektNativeMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return
      } catch {
        # WebView2 can temporarily omit a clickable point for a keyboard-actionable
        # button. Focus + Enter still exercises the real installed UI action.
        $Element.SetFocus()
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
        return
      }
    } catch {
      if ([DateTime]::UtcNow -ge $deadline) { throw }
      Start-Sleep -Milliseconds 150
    }
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'UI element did not become enabled and actionable within 5 seconds.'
}

function Invoke-UiElementPhysically {
  param([Parameter(Mandatory = $true)]$Element)
  $deadline = [DateTime]::UtcNow.AddSeconds(5)
  do {
    try {
      if (-not $Element.Current.IsEnabled) {
        Start-Sleep -Milliseconds 100
        continue
      }
      if ($Element.Current.IsOffscreen -and $Element.Current.IsScrollItemPatternAvailable) {
        $scroll = $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern)
        $scroll.ScrollIntoView()
        Start-Sleep -Milliseconds 100
      }
      try {
        $point = $Element.GetClickablePoint()
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$point.X, [int]$point.Y)
        [DokkomplektNativeMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [DokkomplektNativeMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
      } catch {
        $Element.SetFocus()
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
      }
      return
    } catch {
      if ([DateTime]::UtcNow -ge $deadline) { throw }
      Start-Sleep -Milliseconds 100
    }
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'UI element did not become physically actionable within 5 seconds.'
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

function New-MedicalStoryDocxFixture {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][ValidateSet('template','source')][string]$Variant
  )
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::CreateNew)
  try {
    $archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create, $false)
    try {
      if ($Variant -eq 'template') {
        $patient = 'Иванов Иван Иванович'
        $caseNumber = '1111'
        $admission = '20.08.2026'
        $diagnosis = 'F20.0 шаблонная формулировка'
        $treatment = 'старое лечение'
        $workplace = 'Старый завод'
        $position = 'старый инженер'
      } else {
        $patient = 'Петров Пётр Петрович'
        $caseNumber = '2222'
        $admission = '26.08.2026'
        $diagnosis = 'F20.0 Параноидная шизофрения'
        $treatment = 'рисперидон 4 мг/сут'
        $workplace = 'Новый завод'
        $position = 'инженер'
      }
      if ($Variant -eq 'template') {
        # Reproduce the real doctor-owned template shape: labels live in Word
        # table cells while the adjacent value cells are physically empty.
        # The installed application must compile those exact empty cells into
        # semantic render targets before the template can be published.
        $structuredFields =
          '<w:tbl>' +
          '<w:tr><w:tc><w:p><w:r><w:t>История болезни №</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>' +
          '<w:tr><w:tc><w:p><w:r><w:t>Диагноз</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>' +
          '<w:tr><w:tc><w:p><w:r><w:t>План лечения</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>' +
          '<w:tr><w:tc><w:p><w:r><w:t>Место работы</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc><w:tc><w:p><w:r><w:t>Должность</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>' +
          '</w:tbl>'
      } else {
        $structuredFields =
          '<w:p><w:r><w:t>Номер истории болезни: ' + $caseNumber + '</w:t></w:r></w:p>' +
          '<w:p><w:r><w:t>Диагноз: ' + $diagnosis + '</w:t></w:r></w:p>' +
          '<w:p><w:r><w:t>Лечение: ' + $treatment + '</w:t></w:r></w:p>' +
          '<w:p><w:r><w:t>Место работы: ' + $workplace + '</w:t></w:r></w:p>' +
          '<w:p><w:r><w:t>Должность: ' + $position + '</w:t></w:r></w:p>'
      }
      $body = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>' +
        '<w:p><w:r><w:t>Первичный осмотр</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Ф.И.О.: ' + $patient + '</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Дата поступления: ' + $admission + '</w:t></w:r></w:p>' +
        $structuredFields +
        '<w:p><w:r><w:t>Лечащий врач __________</w:t></w:r></w:p>' +
        '<w:p><w:r><w:t>Заведующий отделением __________</w:t></w:r></w:p>' +
        '<w:sectPr><w:headerReference w:type="default" r:id="rIdHeader1"/></w:sectPr></w:body></w:document>'
      $parts = @{
        '[Content_Types].xml' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>'
        '_rels/.rels' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
        'word/_rels/document.xml.rels' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHeader1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>'
        'word/document.xml' = $body
        'word/header1.xml' = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>ГБУЗ НО «НКЦПЗ» диспансер №2</w:t></w:r></w:p></w:hdr>'
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
    $condition = [System.Windows.Automation.AndCondition]::new($name, $kind)
    $found = $null
    for ($attempt = 0; $attempt -lt 3 -and $null -eq $found; $attempt++) {
      try {
        $found = $Root.FindFirst(
          [System.Windows.Automation.TreeScope]::Descendants,
          $condition
        )
      } catch {
        if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
        if ($attempt -lt 2) { Start-Sleep -Milliseconds 200 }
      }
    }
    if ($null -ne $found) { return $found }
  }
  return $null
}

function Find-ReadyButtonByNames {
  param(
    [Parameter(Mandatory = $true)]$Root,
    [Parameter(Mandatory = $true)][string[]]$Names
  )
  $button = Find-ButtonByNames -Root $Root -Names $Names
  if ($null -eq $button) { return $null }
  try {
    if (-not $button.Current.IsEnabled) { return $null }
    if ($button.Current.IsInvokePatternAvailable -or $button.Current.IsLegacyIAccessiblePatternAvailable) {
      return $button
    }
    if ($button.Current.IsOffscreen -and $button.Current.IsScrollItemPatternAvailable) {
      $scroll = $button.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern)
      $scroll.ScrollIntoView()
      Start-Sleep -Milliseconds 100
    }
    try {
      $null = $button.GetClickablePoint()
      return $button
    } catch {
      $button.SetFocus()
      return $button
    }
  } catch {
    return $null
  }
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
  if ($secondProcess.ExitCode -ne 0) {
    throw "Adversarial single-instance check failed: second launch exited with code $($secondProcess.ExitCode)."
  }
  if ($process.HasExited) { throw 'Adversarial single-instance check killed the primary UI process.' }
  Write-Host 'ADVERSARIAL OK: second launch exited cleanly and primary UI stayed alive.'
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
if ($adversarial) {
  New-MedicalStoryDocxFixture -Path $plainTemplate -Variant 'template'
  $medicalSource = Join-Path $fixtureDir 'новый первичный пациент.docx'
  New-MedicalStoryDocxFixture -Path $medicalSource -Variant 'source'
  $activeSourcePath = $medicalSource
} else {
  New-PlainDocxFixture -Path $plainTemplate
  $activeSourcePath = $plainTemplate
}
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
  Find-ReadyButtonByNames -Root $appWindow -Names @('Создать кнопки (1)')
}
Invoke-UiElement -Element $createPreparedButton

$expectedTemplateButtonName = if ($adversarial) { 'исходник проверка' } else { 'button-smoke' }
# WebView2 can acknowledge UIA InvokePattern without dispatching the underlying DOM
# click on a saturated hosted runner. Require an observable setup transition and,
# if the action is still visibly idle, retry exactly once with physical input.
$templateSetupTransitionDeadlineSeconds = 5
$templateSetupTransitionDeadline = [DateTime]::UtcNow.AddSeconds($templateSetupTransitionDeadlineSeconds)
$templateSetupStarted = $false
do {
  if ($process.HasExited) { throw 'Installed application exited while starting Word template registration.' }
  $createdEarly = Find-ButtonByNames -Root $appWindow -Names @($expectedTemplateButtonName)
  if ($null -ne $createdEarly) { $templateSetupStarted = $true; break }
  $stillReady = Find-ReadyButtonByNames -Root $appWindow -Names @('Создать кнопки (1)')
  if ($null -eq $stillReady) { $templateSetupStarted = $true; break }
  Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $templateSetupTransitionDeadline)

if (-not $templateSetupStarted) {
  Write-Host 'UIA action produced no observable template-registration transition; retrying once with physical input.'
  $createPreparedButton = Wait-UiElement -Description 'Создать кнопки (1) physical retry' -TimeoutSeconds 10 -Probe {
    Find-ReadyButtonByNames -Root $appWindow -Names @('Создать кнопки (1)')
  }
  Invoke-UiElementPhysically -Element $createPreparedButton
}

# Registration still has a bounded completion deadline and must expose the real
# filename-derived document button; a process exit remains an immediate failure.
$templateRegistrationDeadlineSeconds = 90
$createdDocumentButton = Wait-UiElement -Description "created static template button '$expectedTemplateButtonName'" -TimeoutSeconds $templateRegistrationDeadlineSeconds -Probe {
  if ($process.HasExited) { throw 'Installed application exited while registering the selected Word template.' }
  Find-ButtonByNames -Root $appWindow -Names @($expectedTemplateButtonName)
}
if ($null -eq $createdDocumentButton) { throw 'The real plain DOCX did not become a document button.' }
$contentDerivedButton = Find-ButtonByNames -Root $appWindow -Names @('Проверочная кнопка')
if ($null -ne $contentDerivedButton) {
  throw 'Template body text leaked into the reusable document button label.'
}
Write-Host "Create button from a real unmarked DOCX OK: '$expectedTemplateButtonName'; body text was not used as the label."

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
Set-UiValue -Element $sourceFileNameEdit -Value $activeSourcePath
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
  $goodSourceName = [System.IO.Path]::GetFileName($activeSourcePath)
  $goodSourceAfterBroken = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $goodSourceName)
  )
  $brokenSourceActive = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, [System.IO.Path]::GetFileName($brokenSource))
  )
  if ($null -eq $goodSourceAfterBroken -or $null -ne $brokenSourceActive) {
    throw 'Corrupt replacement became active or displaced the previously accepted source.'
  }
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
  $goodSourceAfterOversized = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $goodSourceName)
  )
  $oversizedSourceActive = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, [System.IO.Path]::GetFileName($oversizedSource))
  )
  if ($null -eq $goodSourceAfterOversized -or $null -ne $oversizedSourceActive) {
    throw 'Oversized replacement became active or displaced the previously accepted source.'
  }
  Write-Host 'ADVERSARIAL OK: >100MB source rejected without losing previous source.'
}

# End-to-end installed generation proof: select the real created button, open the
# real preflight, fill deterministic folder fields when the backend asks for them,
# click Create, then require a physical readable DOCX in the Desktop output subfolder.
$selectAllButton = Wait-UiElement -Description 'Выбрать всё button' -TimeoutSeconds 30 -Probe {
  Find-ReadyButtonByNames -Root $appWindow -Names @('Выбрать всё')
}
Invoke-UiElement -Element $selectAllButton

$preflightButton = Wait-UiElement -Description 'generation action for one selected document' -TimeoutSeconds 40 -Probe {
  Find-ReadyButtonByNames -Root $appWindow -Names @('Проверить и создать (1)', 'Создать документы (1)')
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

$expectedGeneratedFileName = "$expectedTemplateButtonName.docx"
$generateButton = Wait-UiElement -Description 'Создать документы button' -TimeoutSeconds 30 -Probe {
  Find-ReadyButtonByNames -Root $appWindow -Names @('Создать документы')
}
Invoke-UiElement -Element $generateButton

# WebView2 can report a successful UIA InvokePattern call without dispatching a DOM click
# on a saturated hosted runner. Never trust the automation method itself: require an
# observable product transition. If nothing at all changes, retry once through real
# mouse/keyboard input. The app's confirmation-in-flight guard makes this idempotent,
# and the smoke still fails unless a physical readable DOCX is ultimately published.
$generationTransitionDeadlineSeconds = 5
$generationTransitionDeadline = [DateTime]::UtcNow.AddSeconds($generationTransitionDeadlineSeconds)
$generationActionStarted = $false
do {
  if ($process.HasExited) { throw 'Installed application exited while starting real document generation.' }
  $createdDuringTransition = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $expectedGeneratedFileName -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($null -ne $createdDuringTransition) { $generationActionStarted = $true; break }

  $failureDuringTransition = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Документы не созданы')
  )
  if ($null -ne $failureDuringTransition) { $generationActionStarted = $true; break }

  $busyGenerationButton = Find-ButtonByNames -Root $appWindow -Names @('Создаём документы…', 'Проверяем сценарий…')
  if ($null -ne $busyGenerationButton) { $generationActionStarted = $true; break }

  $preflightStillOpen = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Проверка перед созданием')
  )
  if ($null -eq $preflightStillOpen) { $generationActionStarted = $true; break }
  Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $generationTransitionDeadline)

if (-not $generationActionStarted) {
  Write-Host 'UIA action produced no observable generation transition; retrying once with physical input.'
  $generateButton = Wait-UiElement -Description 'Создать документы physical retry' -TimeoutSeconds 10 -Probe {
    Find-ReadyButtonByNames -Root $appWindow -Names @('Создать документы')
  }
  Invoke-UiElementPhysically -Element $generateButton
}

$createdDeadline = [DateTime]::UtcNow.AddSeconds(60)
$createdDoc = $null
$generationFailure = $null
do {
  if ($process.HasExited) { throw 'Installed application exited during real document generation smoke.' }
  $createdDoc = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $expectedGeneratedFileName -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($null -ne $createdDoc) { break }

  # The preflight deliberately stays open when Rust rejects generation. Surface the
  # real backend reason immediately instead of hiding it behind a 60-second file timeout.
  $failureMarker = $appWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      'Документы не созданы'
    )
  )
  if ($null -ne $failureMarker) {
    $textCondition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Text
    )
    $visibleText = @($appWindow.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textCondition) |
      ForEach-Object { $_.Current.Name } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $markerIndex = [Array]::IndexOf($visibleText, 'Документы не созданы')
    if ($markerIndex -ge 0 -and ($markerIndex + 1) -lt $visibleText.Count) {
      $generationFailure = $visibleText[$markerIndex + 1]
    } else {
      $generationFailure = ($visibleText -join ' | ')
    }
    break
  }
  Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $createdDeadline)
if ($null -ne $generationFailure) {
  Write-AppLaunchDiagnostics
  throw "Installed application rejected real document generation: $generationFailure"
}
if ($null -eq $createdDoc) {
  Write-AppLaunchDiagnostics
  Write-Host '--- installed UI snapshot after generation timeout ---'
  foreach ($controlType in @([System.Windows.Automation.ControlType]::Button, [System.Windows.Automation.ControlType]::Text)) {
    $controlCondition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      $controlType
    )
    @($appWindow.FindAll([System.Windows.Automation.TreeScope]::Descendants, $controlCondition)) |
      ForEach-Object { if (-not [string]::IsNullOrWhiteSpace($_.Current.Name)) { Write-Host "UI: $($_.Current.Name)" } }
  }
  Write-Host '--- Desktop output tree after generation timeout ---'
  Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -Force -ErrorAction SilentlyContinue | ForEach-Object { Write-Host $_.FullName }
  throw "Installed application did not physically create $expectedGeneratedFileName under $defaultOutputRoot"
}
if ($createdDoc.Length -le 0) { throw "Created DOCX is empty: $($createdDoc.FullName)" }
$createdArchive = [System.IO.Compression.ZipFile]::OpenRead($createdDoc.FullName)
try {
  $documentEntry = $createdArchive.GetEntry('word/document.xml')
  if ($null -eq $documentEntry) { throw "Created file is not a readable Word DOCX: $($createdDoc.FullName)" }
  $reader = [System.IO.StreamReader]::new($documentEntry.Open(), [System.Text.Encoding]::UTF8)
  try { $createdXml = $reader.ReadToEnd() } finally { $reader.Dispose() }
  if ($adversarial) {
    if ($createdXml -notmatch 'Первичный осмотр') { throw 'Created medical DOCX lost the template heading.' }
    if ($createdXml -notmatch 'Петров Пётр Петрович') { throw 'Installed medical generation did not render the current patient name.' }
    if ($createdXml -match 'Иванов Иван Иванович') { throw 'Installed medical generation leaked the old template patient name.' }
    if ($createdXml -notmatch '2222') { throw 'Installed medical generation did not render the current case number.' }
    if ($createdXml -match '>1111<') { throw 'Installed medical generation leaked the old template case number.' }
    if ($createdXml -notmatch 'F20.0 Параноидная шизофрения') { throw 'Installed medical generation did not render the current diagnosis from the tabular template.' }
    if ($createdXml -match 'шаблонная формулировка') { throw 'Installed medical generation leaked the old tabular diagnosis.' }
    if ($createdXml -notmatch 'рисперидон 4 мг/сут') { throw 'Installed medical generation did not render current treatment into the tabular template.' }
    if ($createdXml -match 'старое лечение') { throw 'Installed medical generation leaked old tabular treatment.' }
    if ($createdXml -notmatch 'Новый завод') { throw 'Installed medical generation did not render current workplace.' }
    if ($createdXml -match 'Старый завод') { throw 'Installed medical generation leaked old workplace.' }
    if ($createdXml -notmatch '>инженер<') { throw 'Installed medical generation did not render current position.' }
    if ($createdXml -match 'старый инженер') { throw 'Installed medical generation leaked old position.' }
    if ($createdXml -notmatch 'Экспертный анамнез') { throw 'Primary medical generation did not restore the role-owned expert anamnesis before signatures.' }
    $headerEntry = $createdArchive.GetEntry('word/header1.xml')
    if ($null -eq $headerEntry) { throw 'Created medical DOCX lost its Word header story.' }
    $headerReader = [System.IO.StreamReader]::new($headerEntry.Open(), [System.Text.Encoding]::UTF8)
    try { $createdHeaderXml = $headerReader.ReadToEnd() } finally { $headerReader.Dispose() }
    if ($createdHeaderXml -notmatch 'НКЦПЗ') { throw 'Medical compiler consumed or corrupted the fixed Word header.' }
    if ($createdHeaderXml -match '\{\{') { throw 'Medical compiler incorrectly converted fixed header text into a semantic placeholder.' }
  } elseif ($createdXml -notmatch 'Проверочная кнопка') {
    throw 'Created DOCX lost the template content.'
  }
} finally {
  $createdArchive.Dispose()
}
Write-Host "Installed end-to-end document generation OK: $($createdDoc.FullName)"

if ($adversarial) {
  # The file reaches disk before the React confirmation cycle necessarily finishes.
  # A real user cannot click the generation action through the still-open modal, but
  # UI Automation can invoke covered controls. Wait for the first preflight to close
  # before testing a second user-visible generation cycle.
  $firstPreflightClosedDeadline = [DateTime]::UtcNow.AddSeconds(30)
  $firstPreflightStillOpen = $true
  do {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$process.Id
    )
    $currentAppWindow = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $currentAppWindow) {
      $firstPreflightStillOpen = $true
    } else {
      $firstPreflight = $currentAppWindow.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
          [System.Windows.Automation.AutomationElement]::NameProperty,
          'Проверка перед созданием'
        )
      )
      $firstPreflightStillOpen = $null -ne $firstPreflight
    }
    if ($firstPreflightStillOpen) { Start-Sleep -Milliseconds 250 }
  } while ($firstPreflightStillOpen -and [DateTime]::UtcNow -lt $firstPreflightClosedDeadline)
  if ($firstPreflightStillOpen) {
    throw 'First generation published a DOCX but its preflight did not finish closing.'
  }
  Start-Sleep -Milliseconds 250

  # Repeating the same deterministic output must not overwrite the first kit.
  # Generation/modals can rebuild WebView2's accessibility provider on hosted
  # Windows runners, so every poll must resolve the live top-level window.
  $repeatAction = Wait-UiElement -Description 'repeat generation action' -TimeoutSeconds 30 -Probe {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$process.Id
    )
    $currentAppWindow = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $currentAppWindow) { return $null }
    Find-ReadyButtonByNames -Root $currentAppWindow -Names @('Проверить и создать (1)', 'Создать документы (1)')
  }
  Invoke-UiElement -Element $repeatAction
  $repeatPreflight = Wait-UiElement -Description 'repeat preflight' -TimeoutSeconds 30 -Probe {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$process.Id
    )
    $currentAppWindow = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $currentAppWindow) { return $null }
    $currentAppWindow.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Проверка перед созданием')
    )
  }
  $repeatGenerate = Wait-UiElement -Description 'repeat Создать документы' -TimeoutSeconds 30 -Probe {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$process.Id
    )
    $currentAppWindow = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $currentAppWindow) { return $null }
    Find-ReadyButtonByNames -Root $currentAppWindow -Names @('Создать документы')
  }
  Invoke-UiElement -Element $repeatGenerate
  $otherVariants = Wait-UiElement -Description 'existing-kit Другие варианты' -TimeoutSeconds 30 -Probe {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$process.Id
    )
    $currentAppWindow = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $currentAppWindow) { return $null }
    Find-ButtonByNames -Root $currentAppWindow -Names @('Другие варианты')
  }
  Invoke-UiElement -Element $otherVariants
  $newVersion = Wait-UiElement -Description 'Создать новую версию' -TimeoutSeconds 30 -Probe {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$process.Id
    )
    $currentAppWindow = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $currentAppWindow) { return $null }
    Find-ButtonByNames -Root $currentAppWindow -Names @('Создать новую версию')
  }
  Invoke-UiElement -Element $newVersion
  $versionDeadline = [DateTime]::UtcNow.AddSeconds(60)
  do {
    $versionDocs = @(Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $expectedGeneratedFileName -ErrorAction SilentlyContinue)
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
  if ($blockedProcess.HasExited) { throw 'Output-root path collision crashed the application.' }
  if (-not (Test-Path -LiteralPath $defaultOutputRoot -PathType Leaf)) {
    throw 'Application silently replaced the deliberate output-root collision file.'
  }
  try {
    $recoveryAlert = Wait-UiElement -Description 'visible output-root recovery alert' -TimeoutSeconds 30 -Probe {
      # WebView2 may rebuild its accessibility provider after startup on hosted
      # Windows runners. Re-resolve the top-level window on every probe instead
      # of keeping an AutomationElement whose descendant tree can go stale.
      $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
        [int]$blockedProcess.Id
      )
      $currentBlockedWindow = $desktop.FindFirst(
        [System.Windows.Automation.TreeScope]::Children,
        $condition
      )
      if ($null -eq $currentBlockedWindow) { return $null }
      $descendants = $currentBlockedWindow.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
      )
      foreach ($element in $descendants) {
        $name = [string]$element.Current.Name
        if ($name.StartsWith('Не удалось восстановить проверенную папку результата:')) {
          return $element
        }
      }
      return $null
    }
  } catch {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
      [int]$blockedProcess.Id
    )
    $diagnosticWindow = $desktop.FindFirst(
      [System.Windows.Automation.TreeScope]::Children,
      $condition
    )
    $visibleNames = if ($null -eq $diagnosticWindow) {
      @()
    } else {
      @($diagnosticWindow.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
      ) | ForEach-Object { $_.Current.Name } | Where-Object { $_ } | Select-Object -Unique)
    }
    Write-Host ('Output-root collision UIA names: ' + ($visibleNames -join ' | '))
    throw
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
$restartState = Wait-UiElement -Description 'definitive workspace state after restart' -TimeoutSeconds 30 -Probe {
  # WebView2 may rebuild its accessibility provider after a clean restart on
  # hosted Windows runners. Re-resolve the top-level window on every poll so
  # persisted/recovery/empty is read from the live accessibility tree.
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    [int]$process.Id
  )
  $currentAppWindow = $desktop.FindFirst(
    [System.Windows.Automation.TreeScope]::Children,
    $condition
  )
  if ($null -eq $currentAppWindow) { return $null }

  $persisted = Find-ButtonByNames -Root $currentAppWindow -Names @($expectedTemplateButtonName)
  if ($null -ne $persisted) { return @{ Kind = 'persisted'; Element = $persisted } }
  $recovery = $currentAppWindow.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      'Не удалось загрузить рабочий набор'
    )
  )
  if ($null -ne $recovery) { return @{ Kind = 'recovery'; Element = $recovery } }
  $firstRun = Find-ButtonByNames -Root $currentAppWindow -Names @('Создать свои кнопки')
  if ($null -ne $firstRun) { return @{ Kind = 'empty'; Element = $firstRun } }
  return $null
}
if ($restartState.Kind -eq 'recovery') {
  throw 'Persisted workspace restart entered explicit recovery mode instead of restoring the saved button.'
}
if ($restartState.Kind -eq 'empty') {
  throw 'Persisted workspace restart returned an empty first-run pack after the button had been durably created.'
}
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
