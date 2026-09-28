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
  public static extern void mouse_event(uint flags, uint dx, uint dy, int data, UIntPtr extraInfo);
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, string lParam);
  [DllImport("user32.dll", EntryPoint = "SendMessageW", SetLastError = true)]
  public static extern IntPtr SendMessagePtr(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder text, int maxCount);
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool IsWindow(IntPtr hWnd);
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

function Find-ElementByAutomationId {
  param([Parameter(Mandatory = $true)]$Root, [Parameter(Mandatory = $true)][string]$AutomationId)
  return $Root.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      $AutomationId
    )
  )
}

function Find-ReadyButtonByAutomationId {
  param([Parameter(Mandatory = $true)]$Root, [Parameter(Mandatory = $true)][string]$AutomationId)
  $button = $Root.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.AndCondition]::new(
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button),
      [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $AutomationId)
    )
  )
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

  # WebView2 on hosted Windows can acknowledge both UIA Invoke and a foreground
  # coordinate click without dispatching the React DOM activation. Do not declare
  # success from either input method: first require the observable transition.
  # If it is still absent and the exact same action remains enabled, use one
  # focused keyboard activation (the same user-equivalent path already required
  # by the installed learning proof), then still require the real transition.
  $physicalDeadline = [DateTime]::UtcNow.AddSeconds($TransitionSeconds)
  do {
    try { $transition = & $TransitionProbe } catch {
      if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
      $transition = $null
    }
    if ($null -ne $transition) { return $transition }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $physicalDeadline)

  $keyboardAction = $null
  try { $keyboardAction = & $ActionProbe } catch {
    if (-not (Test-UiaTransientTimeout -ErrorRecord $_)) { throw }
  }
  if ($null -ne $keyboardAction -and $keyboardAction.Current.IsEnabled) {
    Write-Host "E1 UI action '$Description' still has no transition after physical retry; using one focused keyboard Space fallback."
    if ($keyboardAction.Current.IsOffscreen -and $keyboardAction.Current.IsScrollItemPatternAvailable) {
      $keyboardAction.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
      Start-Sleep -Milliseconds 100
    }
    $keyboardAction.SetFocus()
    Start-Sleep -Milliseconds 100
    [System.Windows.Forms.SendKeys]::SendWait(' ')
  }

  return Wait-UiElement -Description $TransitionDescription -TimeoutSeconds 30 -Probe $TransitionProbe
}

function Invoke-UiElementPhysically {
  param([Parameter(Mandatory = $true)]$Element, [string]$Description = 'UI element')
  if (-not $Element.Current.IsEnabled) { throw "$Description is disabled" }

  # A native OpenFileDialog can close while another hosted-runner window still
  # owns foreground. In that state the first global click merely activates the
  # Dokkomplekt window and never reaches the WebView2 button. Mirror the proven
  # baseline installed smoke: restore the real installed app to foreground before
  # the one bounded physical retry, then focus and dispatch the user-equivalent input.
  $process.Refresh()
  $windowHandle = [IntPtr]$process.MainWindowHandle
  if ($windowHandle -ne [IntPtr]::Zero) {
    [void][DokkomplektE1NativeMouse]::ShowWindow($windowHandle, 5)
    [void][DokkomplektE1NativeMouse]::SetForegroundWindow($windowHandle)
    Start-Sleep -Milliseconds 150
  }

  function Test-ElementPhysicallyVisible {
    param([Parameter(Mandatory = $true)]$Target)
    # WebView2 IsOffscreen is advisory only: hosted runners can report true for
    # a rectangle whose center is physically inside the live app window.
    $rect = $Target.Current.BoundingRectangle
    if ($rect.IsEmpty -or $rect.Width -le 1 -or $rect.Height -le 1) { return $false }
    $appWindow = Find-LiveAppWindow
    if ($null -eq $appWindow) { return $false }
    $windowRect = $appWindow.Current.BoundingRectangle
    $screen = [System.Windows.Forms.Screen]::FromPoint(
      [System.Drawing.Point]::new(
        [int][Math]::Round($windowRect.Left + ($windowRect.Width / 2)),
        [int][Math]::Round($windowRect.Top + ($windowRect.Height / 2))
      )
    )
    $work = $screen.WorkingArea
    $virtual = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $centerX = $rect.Left + ($rect.Width / 2)
    $centerY = $rect.Top + ($rect.Height / 2)
    # A UIA rectangle is physically clickable only when its center inside the real Windows VirtualScreen
    # is valid. This fail-closed guard prevents
    # stale/virtualized WebView2 geometry from sending a global click off-screen.
    if ($centerX -lt $virtual.Left -or $centerX -ge $virtual.Right -or
        $centerY -lt $virtual.Top -or $centerY -ge $virtual.Bottom) {
      return $false
    }
    $margin = 6
    $left = [Math]::Max($windowRect.Left + $margin, $work.Left + $margin)
    $top = [Math]::Max($windowRect.Top + $margin, $work.Top + $margin)
    $right = [Math]::Min($windowRect.Right - $margin, $work.Right - $margin)
    $bottom = [Math]::Min($windowRect.Bottom - $margin, $work.Bottom - $margin)
    return $centerX -ge $left -and $centerX -lt $right -and
      $centerY -ge $top -and $centerY -lt $bottom
  }

  if (-not (Test-ElementPhysicallyVisible -Target $Element)) {
    # Recover only when geometry is not physically clickable. UIA IsOffscreen
    # is logged but cannot veto a point whose rectangle is inside the live app.
    if ($Element.Current.IsScrollItemPatternAvailable) {
      try {
        $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
        Start-Sleep -Milliseconds 150
      } catch { }
    }
    if (-not (Test-ElementPhysicallyVisible -Target $Element)) {
      try {
        $Element.SetFocus()
        Start-Sleep -Milliseconds 200
      } catch { }
    }
    if (-not (Test-ElementPhysicallyVisible -Target $Element)) {
      $appWindow = Find-LiveAppWindow
      if ($null -ne $appWindow) {
        $windowRect = $appWindow.Current.BoundingRectangle
        $wheelX = [int][Math]::Round($windowRect.Left + ($windowRect.Width / 2))
        $wheelY = [int][Math]::Round($windowRect.Top + ($windowRect.Height / 2))
        [void][DokkomplektE1NativeMouse]::SetCursorPos($wheelX, $wheelY)
        for ($scrollAttempt = 0; $scrollAttempt -lt 12 -and -not (Test-ElementPhysicallyVisible -Target $Element); $scrollAttempt++) {
          $targetRect = $Element.Current.BoundingRectangle
          $targetCenterY = $targetRect.Top + ($targetRect.Height / 2)
          $windowCenterY = $windowRect.Top + ($windowRect.Height / 2)
          $wheelDelta = if ($targetCenterY -gt $windowCenterY) { -360 } else { 360 }
          [DokkomplektE1NativeMouse]::mouse_event(0x0800, 0, 0, $wheelDelta, [UIntPtr]::Zero)
          Start-Sleep -Milliseconds 120
        }
      }
    }
    if (-not (Test-ElementPhysicallyVisible -Target $Element)) {
      $rect = $Element.Current.BoundingRectangle
      $appWindow = Find-LiveAppWindow
      $windowRect = if ($null -ne $appWindow) { $appWindow.Current.BoundingRectangle } else { [System.Windows.Rect]::Empty }
      throw "$Description remains outside the physically clickable installed-app area after bounded scroll/focus recovery. rect=($([int]$rect.Left),$([int]$rect.Top),$([int]$rect.Width),$([int]$rect.Height)); window=($([int]$windowRect.Left),$([int]$windowRect.Top),$([int]$windowRect.Width),$([int]$windowRect.Height))."
    }
  }

  $clickPoint = $null
  try {
    $clickPoint = $Element.GetClickablePoint()
  } catch {
    $rect = $Element.Current.BoundingRectangle
    if (-not $rect.IsEmpty -and $rect.Width -gt 1 -and $rect.Height -gt 1) {
      $clickPoint = [System.Windows.Point]::new(
        $rect.Left + ($rect.Width / 2),
        $rect.Top + ($rect.Height / 2)
      )
    }
  }
  if ($null -ne $clickPoint) {
    # UIA returns physical screen coordinates. WinForms Cursor.Position may be
    # DPI-virtualized on hosted runners, which can shift lower WebView2 targets
    # away from the actual HTML control. Use user32 directly so UIA and mouse
    # coordinates stay in the same native coordinate space.
    $x = [int][Math]::Round($clickPoint.X)
    $y = [int][Math]::Round($clickPoint.Y)
    $rect = $Element.Current.BoundingRectangle
    Write-Host "E1 physical target '$Description': point=($x,$y) rect=($([int]$rect.Left),$([int]$rect.Top),$([int]$rect.Width),$([int]$rect.Height)) offscreen=$($Element.Current.IsOffscreen)"
    $appWindow = Find-LiveAppWindow
    if ($null -eq $appWindow) { throw "$Description lost the installed app window before physical click." }
    $windowRect = $appWindow.Current.BoundingRectangle
    $screen = [System.Windows.Forms.Screen]::FromPoint(
      [System.Drawing.Point]::new(
        [int][Math]::Round($windowRect.Left + ($windowRect.Width / 2)),
        [int][Math]::Round($windowRect.Top + ($windowRect.Height / 2))
      )
    )
    $work = $screen.WorkingArea
    $left = [Math]::Max($windowRect.Left, $work.Left)
    $top = [Math]::Max($windowRect.Top, $work.Top)
    $right = [Math]::Min($windowRect.Right, $work.Right)
    $bottom = [Math]::Min($windowRect.Bottom, $work.Bottom)
    if ($x -lt $left -or $x -ge $right -or $y -lt $top -or $y -ge $bottom) {
      throw "$Description produced a click point outside the installed app working area: point=($x,$y); app=($([int]$windowRect.Left),$([int]$windowRect.Top),$([int]$windowRect.Width),$([int]$windowRect.Height)); work=($($work.Left),$($work.Top),$($work.Width),$($work.Height))."
    }
    if (-not [DokkomplektE1NativeMouse]::SetCursorPos($x, $y)) {
      throw "$Description failed to position the native cursor at ($x,$y)."
    }
    Start-Sleep -Milliseconds 75
    [DokkomplektE1NativeMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [DokkomplektE1NativeMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return
  }

  try { $Element.SetFocus() } catch { }
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
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

function Set-ReactControlledText {
  param(
    [Parameter(Mandatory = $true)]$Element,
    [Parameter(Mandatory = $true)][string]$Value,
    [Parameter(Mandatory = $true)][string]$Description
  )

  # WebView2 exposes HTML inputs through UIA, but SetFocus can succeed without
  # transferring real DOM keyboard focus. Commit through user-equivalent input
  # and require observable value persistence before accepting the interaction.
  $expected = Normalize-UiValue -Value $Value
  $lastActual = ''
  for ($attempt = 0; $attempt -lt 3; $attempt++) {
    try {
      if ($Element.Current.IsOffscreen -and $Element.Current.IsScrollItemPatternAvailable) {
        $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
        Start-Sleep -Milliseconds 100
      }

      if ($attempt -gt 0) {
        Invoke-UiElementPhysically -Element $Element -Description "$Description focus retry $attempt"
        Start-Sleep -Milliseconds 120
      } else {
        $Element.SetFocus()
        Start-Sleep -Milliseconds 100
      }

      Set-Clipboard -Value $Value -ErrorAction Stop
      [System.Windows.Forms.SendKeys]::SendWait('^a')
      [System.Windows.Forms.SendKeys]::SendWait('^v')

      $commitDeadline = [DateTime]::UtcNow.AddSeconds(2)
      do {
        Start-Sleep -Milliseconds 100
        $lastActual = Normalize-UiValue -Value (Get-UiValue -Element $Element)
        if ($lastActual -eq $expected) { break }
      } while ([DateTime]::UtcNow -lt $commitDeadline)

      if ($lastActual -eq $expected) {
        [System.Windows.Forms.SendKeys]::SendWait('{TAB}')
        Start-Sleep -Milliseconds 200
        $lastActual = Normalize-UiValue -Value (Get-UiValue -Element $Element)
        if ($lastActual -eq $expected) { return }
      }
    } catch {
      if ($attempt -ge 2) {
        throw "React input commit failed for '$Description': $($_.Exception.Message)"
      }
    }
  }

  throw "React input did not persist for '$Description'. Expected '$expected', actual '$lastActual'."
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

function Set-OpenFileDialogPath {
  param(
    [Parameter(Mandatory = $true)]$Dialog,
    [Parameter(Mandatory = $true)][string]$Path
  )

  $dialogHandle = [IntPtr]$Dialog.Current.NativeWindowHandle
  if ($dialogHandle -ne [IntPtr]::Zero) {
    [void][DokkomplektE1NativeMouse]::ShowWindow($dialogHandle, 5)
    [void][DokkomplektE1NativeMouse]::SetForegroundWindow($dialogHandle)
    Start-Sleep -Milliseconds 100
  }

  $edit = $Dialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      '1148'
    )
  )
  if ($null -eq $edit) {
    throw "OpenFileDialog filename edit AutomationId=1148 is missing for '$Path'."
  }

  # Drive the filename field as a real user would. On hosted Windows the
  # accessibility ValuePattern can echo SetValue while the common dialog's real
  # edit remains empty. Keyboard paste updates the actual focused control.
  $edit.SetFocus()
  Start-Sleep -Milliseconds 100
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  try {
    Set-Clipboard -Value $Path -ErrorAction Stop
    [System.Windows.Forms.SendKeys]::SendWait('^v')
  } catch {
    # Do not fall back to ValuePattern.SetValue here. Hosted common dialogs can
    # echo that UIA value while leaving the native filename edit empty. Leave the
    # field untouched and continue to the independent native/user-equivalent
    # fallbacks below, which are verified separately before submission.
    Write-Host "OpenFileDialog clipboard paste unavailable; continuing with native/user fallback for '$Path'."
  }
  Start-Sleep -Milliseconds 250

  $expected = Normalize-UiValue -Value $Path
  $actual = Normalize-UiValue -Value (Get-UiValue -Element $edit)

  # Never "verify" a failed paste by setting and immediately rereading the
  # same UIA ValuePattern: hosted Explorer-style dialogs can echo that value while
  # their native filename edit remains empty. If the real user-equivalent paste
  # did not read back, continue to an independent native/user path instead.
  #
  # In multi-select common dialogs AutomationId=1148 may itself be a zero-HWND
  # wrapper that exposes ValuePattern while a descendant owns the real native
  # edit. Enumerate all value candidates and select a nonzero HWND; never stop at
  # the first accessibility wrapper.
  if ($actual -ne $expected) {
    $supportsValue = [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty,
      $true
    )
    $editHandle = [IntPtr]::Zero
    foreach ($nativeTarget in $edit.FindAll(
      [System.Windows.Automation.TreeScope]::Subtree,
      $supportsValue
    )) {
      $candidateHandle = [IntPtr]$nativeTarget.Current.NativeWindowHandle
      if ($candidateHandle -ne [IntPtr]::Zero) {
        $editHandle = $candidateHandle
        break
      }
    }

    # Some common-dialog implementations expose the native edit as an Edit
    # control without ValuePattern. Keep the same nonzero-HWND requirement.
    if ($editHandle -eq [IntPtr]::Zero) {
      $editControlCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit
      )
      foreach ($nativeTarget in $edit.FindAll(
        [System.Windows.Automation.TreeScope]::Subtree,
        $editControlCondition
      )) {
        $candidateHandle = [IntPtr]$nativeTarget.Current.NativeWindowHandle
        if ($candidateHandle -ne [IntPtr]::Zero) {
          $editHandle = $candidateHandle
          break
        }
      }
    }

    if ($editHandle -ne [IntPtr]::Zero) {
      $null = [DokkomplektE1NativeMouse]::SendMessage(
        $editHandle,
        0x000C,
        [IntPtr]::Zero,
        $Path
      )
      Start-Sleep -Milliseconds 200

      # Verify the exact native control that received WM_SETTEXT. Reading the
      # wrapper's ValuePattern here would recreate the UIA-echo false positive.
      $nativeLength = [DokkomplektE1NativeMouse]::GetWindowTextLength($editHandle)
      $nativeBuilder = [System.Text.StringBuilder]::new([Math]::Max(1, $nativeLength + 1))
      $null = [DokkomplektE1NativeMouse]::GetWindowText(
        $editHandle,
        $nativeBuilder,
        $nativeBuilder.Capacity
      )
      $actual = Normalize-UiValue -Value $nativeBuilder.ToString()
      if ($actual -eq $expected) {
        return $edit
      }
    }
  }

  # Last bounded user-equivalent fallback for Explorer-style multi-select
  # dialogs: navigate to the parent directory through the address bar, then type
  # only the leaf file name into the real file-name field. This avoids the
  # full-path paste quirk while still proving the visible filename before submit.
  if ($actual -ne $expected) {
    $parent = [System.IO.Path]::GetDirectoryName($Path)
    $leaf = [System.IO.Path]::GetFileName($Path)
    if (-not [string]::IsNullOrWhiteSpace($parent) -and -not [string]::IsNullOrWhiteSpace($leaf)) {
      try {
        [void][DokkomplektE1NativeMouse]::SetForegroundWindow($dialogHandle)
        [System.Windows.Forms.SendKeys]::SendWait('^l')
        Start-Sleep -Milliseconds 100
        Set-Clipboard -Value $parent -ErrorAction Stop
        [System.Windows.Forms.SendKeys]::SendWait('^v')
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
        Start-Sleep -Milliseconds 500

        $edit = $Dialog.FindFirst(
          [System.Windows.Automation.TreeScope]::Descendants,
          [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
            '1148'
          )
        )
        if ($null -eq $edit) {
          throw "OpenFileDialog filename edit disappeared while navigating to '$parent'."
        }
        $edit.SetFocus()
        Start-Sleep -Milliseconds 100
        [System.Windows.Forms.SendKeys]::SendWait('^a')
        Set-Clipboard -Value $leaf -ErrorAction Stop
        [System.Windows.Forms.SendKeys]::SendWait('^v')
        Start-Sleep -Milliseconds 250
        $leafActual = Normalize-UiValue -Value (Get-UiValue -Element $edit)
        if ($leafActual -eq (Normalize-UiValue -Value $leaf)) {
          Write-Host "OpenFileDialog multi-select fallback committed leaf '$leaf' in '$parent'."
          return $edit
        }
        $actual = $leafActual
      } catch {
        Write-Host "OpenFileDialog address-bar clipboard fallback unavailable for '$Path': $($_.Exception.Message)"
      }
    }
  }

  if ($actual -ne $expected) {
    throw "OpenFileDialog path did not commit. Expected='$Path' Actual='$actual'."
  }
  return $edit
}

function Submit-OpenFileDialog {
  param([Parameter(Mandatory = $true)]$Dialog)

  # A successful UIA InvokePattern call is not evidence that the hosted Windows
  # common dialog actually accepted the selection. Drive the real native dialog,
  # then prove that the exact HWND disappeared before the installed test proceeds.
  $dialogHandle = [IntPtr]$Dialog.Current.NativeWindowHandle
  if ($dialogHandle -eq [IntPtr]::Zero) {
    throw 'OpenFileDialog does not expose a native HWND.'
  }
  [void][DokkomplektE1NativeMouse]::ShowWindow($dialogHandle, 5)
  [void][DokkomplektE1NativeMouse]::SetForegroundWindow($dialogHandle)
  Start-Sleep -Milliseconds 100

  # Set-OpenFileDialogPath has already proved the exact filename field value.
  # Submit through the dialog's real Open button first. Pressing Enter while the
  # filename edit itself owns focus can make the common dialog treat a full path
  # as navigation and clear the field instead of accepting the file.
  $filenameEdit = $Dialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      '1148'
    )
  )

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
    try {
      $openButton.SetFocus()
      Start-Sleep -Milliseconds 50
      if ($openButton.Current.IsInvokePatternAvailable) {
        $openButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
      } elseif ($openButton.Current.IsLegacyIAccessiblePatternAvailable) {
        $openButton.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern).DoDefaultAction()
      } else {
        $point = $openButton.GetClickablePoint()
        $x = [int][Math]::Round($point.X)
        $y = [int][Math]::Round($point.Y)
        if (-not [DokkomplektE1NativeMouse]::SetCursorPos($x, $y)) {
          throw "Native Open button failed to position the cursor at ($x,$y)."
        }
        Start-Sleep -Milliseconds 75
        [DokkomplektE1NativeMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [DokkomplektE1NativeMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
      }
    } catch {
      Write-Host "Native Open button UIA submit did not complete cleanly; falling back to IDOK: $($_.Exception.Message)"
    }
  }

  $uiaDeadline = [DateTime]::UtcNow.AddSeconds(2)
  while ([DokkomplektE1NativeMouse]::IsWindow($dialogHandle) -and [DateTime]::UtcNow -lt $uiaDeadline) {
    Start-Sleep -Milliseconds 100
  }
  if (-not [DokkomplektE1NativeMouse]::IsWindow($dialogHandle)) { return }

  # UIA can acknowledge InvokePattern while the hosted common dialog remains
  # open. WM_COMMAND/IDOK reaches the same real dialog command handler and is
  # bounded by an exact HWND liveness check.
  [void][DokkomplektE1NativeMouse]::SetForegroundWindow($dialogHandle)
  $null = [DokkomplektE1NativeMouse]::SendMessagePtr(
    $dialogHandle,
    0x0111,
    [IntPtr]1,
    [IntPtr]::Zero
  )
  $nativeDeadline = [DateTime]::UtcNow.AddSeconds(3)
  while ([DokkomplektE1NativeMouse]::IsWindow($dialogHandle) -and [DateTime]::UtcNow -lt $nativeDeadline) {
    Start-Sleep -Milliseconds 100
  }
  if (-not [DokkomplektE1NativeMouse]::IsWindow($dialogHandle)) { return }

  # Final user-equivalent fallback: foreground the same dialog and press Enter.
  # If it still survives, fail closed with the filename visible to the dialog.
  [void][DokkomplektE1NativeMouse]::SetForegroundWindow($dialogHandle)
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  $keyDeadline = [DateTime]::UtcNow.AddSeconds(2)
  while ([DokkomplektE1NativeMouse]::IsWindow($dialogHandle) -and [DateTime]::UtcNow -lt $keyDeadline) {
    Start-Sleep -Milliseconds 100
  }
  if (-not [DokkomplektE1NativeMouse]::IsWindow($dialogHandle)) { return }

  $filename = ''
  try {
    $filenameEdit = $Dialog.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
        '1148'
      )
    )
    if ($null -ne $filenameEdit) { $filename = [string](Get-UiValue -Element $filenameEdit) }
  } catch { }
  throw "Native OpenFileDialog remained open after UIA, WM_COMMAND(IDOK), and Enter. Filename='$filename'."
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


function New-E1TextDocx {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string[]]$Lines
  )
  Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::CreateNew)
  try {
    $archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create, $false)
    try {
      $paragraphs = foreach ($line in $Lines) {
        $escaped = [System.Security.SecurityElement]::Escape([string]$line)
        '<w:p><w:r><w:t xml:space="preserve">' + $escaped + '</w:t></w:r></w:p>'
      }
      $documentXml = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>' +
        ($paragraphs -join '') + '<w:sectPr/></w:body></w:document>'
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

function Find-E1NamedElement {
  param([Parameter(Mandatory = $true)][string]$Name)
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  return $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      $Name
    )
  )
}

function Find-E1NamedElementContaining {
  param([Parameter(Mandatory = $true)][string]$Text)
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  foreach ($element in $window.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.Condition]::TrueCondition
  )) {
    try { $name = [string]$element.Current.Name } catch { continue }
    if (-not [string]::IsNullOrWhiteSpace($name) -and $name.Contains($Text)) {
      return $element
    }
  }
  return $null
}

function Find-E1ElementByAutomationId {
  param([Parameter(Mandatory = $true)][string]$AutomationId)
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  return $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      $AutomationId
    )
  )
}

function Get-E1DomainSelection {
  param([Parameter(Mandatory = $true)][string]$FileName)

  $combo = Find-E1NamedElement -Name "Профиль для $FileName"
  if ($null -eq $combo) { return '' }

  try {
    if ($combo.Current.IsSelectionPatternAvailable) {
      $selection = $combo.GetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern).Current.GetSelection()
      if ($null -ne $selection -and $selection.Count -gt 0) {
        $name = [string]$selection[0].Current.Name
        if (-not [string]::IsNullOrWhiteSpace($name)) { return $name }
      }
    }
  } catch { }

  # Chromium may expose an HTML <select> without SelectionPattern/ValuePattern on
  # the collapsed combobox while still exposing the selected <option> through
  # SelectionItemPattern. Inspect the live option items and require IsSelected.
  $domainOptionNames = @(
    'Профиль: автоматически',
    'Универсальный документооборот',
    'Медицина',
    'Юридическая работа',
    'Кадровая работа',
    'Бухгалтерия',
    'Образование',
    'Своя профессия / профиль'
  )
  $expandedForRead = $false
  try {
    if ($combo.Current.IsExpandCollapsePatternAvailable) {
      $expandCollapse = $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
      if ($expandCollapse.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded) {
        $expandCollapse.Expand()
        $expandedForRead = $true
        Start-Sleep -Milliseconds 150
      }
    }

    $window = Find-LiveAppWindow
    if ($null -ne $window) {
      foreach ($candidate in $window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
      )) {
        try {
          $name = [string]$candidate.Current.Name
          if ($domainOptionNames -notcontains $name) { continue }
          if (-not $candidate.Current.IsSelectionItemPatternAvailable) { continue }
          $selectionItem = $candidate.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
          if ($selectionItem.Current.IsSelected) { return $name }
        } catch { }
      }
    }
  } finally {
    if ($expandedForRead) {
      try {
        $combo = Find-E1NamedElement -Name "Профиль для $FileName"
        if ($null -ne $combo -and $combo.Current.IsExpandCollapsePatternAvailable) {
          $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
        }
      } catch { }
    }
  }

  try {
    return [string](Get-UiValue -Element $combo)
  } catch {
    return ''
  }
}

function Set-E1TemplateDomainOverride {
  param(
    [Parameter(Mandatory = $true)][string]$FileName,
    [Parameter(Mandatory = $true)][string]$OptionName,
    [string]$CustomProfile = ''
  )

  $groupName = "Профиль для $FileName"
  $group = Find-E1NamedElement -Name $groupName
  if ($null -eq $group) {
    try {
      $group = Invoke-UiActionWithObservedTransition `
        -Description "open advanced template settings for $FileName" `
        -TransitionDescription "domain choices for $FileName" `
        -TransitionSeconds 5 `
        -ActionProbe {
          Find-E1NamedElement -Name 'Необязательно: настроить автоматическое заполнение'
        } `
        -TransitionProbe {
          Find-E1NamedElement -Name $groupName
        }
    } catch {
      Write-Host ("E1 UI snapshot after failed advanced settings transition for '$FileName': " + (Get-E1UiSnapshot))
      throw
    }
  }

  $radioName = "$OptionName для $FileName"
  $radio = Wait-UiElement -Description "domain radio '$OptionName' for $FileName" -Probe {
    Find-E1NamedElement -Name $radioName
  }
  if ($radio.Current.IsOffscreen -and $radio.Current.IsScrollItemPatternAvailable) {
    $radio.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
    Start-Sleep -Milliseconds 100
  }

  Invoke-UiElementPhysically -Element $radio -Description "select domain '$OptionName' for $FileName"
  Start-Sleep -Milliseconds 250

  $selectionVerified = $false
  $radio = Find-E1NamedElement -Name $radioName
  if ($null -ne $radio) {
    try {
      if ($radio.Current.IsSelectionItemPatternAvailable) {
        $selectionVerified = $radio.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected
      } elseif ($radio.Current.IsTogglePatternAvailable) {
        $selectionVerified = (
          $radio.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -eq
          [System.Windows.Automation.ToggleState]::On
        )
      }
    } catch { }
  }
  if ($selectionVerified) {
    Write-Host "E1 domain radio PASS: '$OptionName' selected for $FileName."
  } else {
    # Hosted WebView2 can expose the radio but omit its selected/toggle state.
    # Give Chromium one explicit keyboard activation on the same radio so React
    # receives a real user change event even when UIA state read-back is absent.
    try {
      $radio.SetFocus()
      Start-Sleep -Milliseconds 75
      [System.Windows.Forms.SendKeys]::SendWait(' ')
      Start-Sleep -Milliseconds 150
      [System.Windows.Forms.SendKeys]::SendWait('{TAB}')
      Start-Sleep -Milliseconds 150
      Write-Host "E1 domain radio keyboard activation sent for '$OptionName' / $FileName; downstream domain-specific installed behavior remains authoritative."
    } catch {
      throw "E1 domain radio for $FileName could not be activated by physical click or keyboard Space: $($_.Exception.Message)"
    }
  }

  if (-not [string]::IsNullOrWhiteSpace($CustomProfile)) {
    $customName = "Своя профессия / профиль для $FileName"
    $custom = Wait-UiElement -Description "custom domain value for $FileName" -Probe {
      Find-E1NamedElement -Name $customName
    }
    Set-ReactControlledText -Element $custom -Value $CustomProfile -Description "custom domain value for $FileName"

    $custom = Wait-UiElement -Description "persisted custom domain value for $FileName" -Probe {
      Find-E1NamedElement -Name $customName
    }
    $actualCustom = Normalize-UiValue -Value (Get-UiValue -Element $custom)
    Write-Host "E1 custom domain value commit: expected='$CustomProfile' actual='$actualCustom'."
    if ($actualCustom -ne (Normalize-UiValue -Value $CustomProfile)) {
      throw "E1 custom domain override did not persist for $FileName. Expected '$CustomProfile', actual '$actualCustom'."
    }
  }
}

function Get-E1UiSnapshot {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return 'window=<missing>' }
  $parts = New-Object System.Collections.Generic.List[string]
  try {
    foreach ($element in $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
      $name = [string]$element.Current.Name
      if ([string]::IsNullOrWhiteSpace($name)) { continue }
      $type = [string]$element.Current.ControlType.ProgrammaticName
      $enabled = [bool]$element.Current.IsEnabled
      $parts.Add(("$type|enabled=$enabled|$name"))
      if ($parts.Count -ge 120) { break }
    }
  } catch {
    $parts.Add(("snapshot-error=" + $_.Exception.Message))
  }
  return ($parts -join ' || ')
}

function Add-E1DomainTemplate {
  param(
    [Parameter(Mandatory = $true)][string]$TemplatePath,
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][string]$DomainOption,
    [string]$CustomProfile = ''
  )
  $fileName = [System.IO.Path]::GetFileName($TemplatePath)
  $dialog = Invoke-UiActionWithObservedTransition `
    -Description "Добавить шаблоны for $Label" `
    -TransitionDescription "native template picker for $Label" `
    -ActionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Добавить шаблоны')
    } `
    -TransitionProbe { Find-FileDialog }

  $edit = $dialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      '1148'
    )
  )
  Set-UiValue -Element $edit -Value $TemplatePath
  Submit-OpenFileDialog -Dialog $dialog

  $labelInput = Wait-UiElement -Description "template label for $Label" -TimeoutSeconds 40 -Probe {
    Find-E1NamedElement -Name "Название документа для $fileName"
  }
  Set-ReactControlledText -Element $labelInput -Value $Label -Description "template label for $fileName"
  $labelInput = Wait-UiElement -Description "persisted template label for $Label" -Probe {
    Find-E1NamedElement -Name "Название документа для $fileName"
  }
  $actualLabel = Normalize-UiValue -Value (Get-UiValue -Element $labelInput)
  if ($actualLabel -ne (Normalize-UiValue -Value $Label)) {
    throw "E1 template label did not persist for $fileName. Expected '$Label', actual '$actualLabel'."
  }
  Set-E1TemplateDomainOverride -FileName $fileName -OptionName $DomainOption -CustomProfile $CustomProfile

  try {
    $null = Invoke-UiActionWithObservedTransition `
      -Description "create $Label button" `
      -TransitionDescription "$Label document button" `
      -TransitionSeconds 8 `
      -ActionProbe {
        $window = Find-LiveAppWindow
        if ($null -eq $window) { return $null }
        Find-ReadyButtonByNames -Root $window -Names @('Создать кнопки (1)')
      } `
      -TransitionProbe {
        $window = Find-LiveAppWindow
        if ($null -eq $window) { return $null }
        Find-ButtonByNames -Root $window -Names @($Label)
      }
  } catch {
    Write-Host ("E1 UI snapshot after failed template confirmation for '$Label': " + (Get-E1UiSnapshot))
    # Hosted WebView2 can expose an enabled HTML button whose UIA InvokePattern
    # and one coordinate click are both acknowledged without delivering a DOM
    # click. The same runner already needs a focused keyboard fallback for other
    # React controls. Use one bounded Space activation only after both previous
    # user-equivalent paths failed and the button is still enabled.
    $createButton = Wait-UiElement -Description "focused create $Label button fallback" -Probe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Создать кнопки (1)')
    }
    try {
      if ($createButton.Current.IsOffscreen -and $createButton.Current.IsScrollItemPatternAvailable) {
        $createButton.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
        Start-Sleep -Milliseconds 100
      }
      $createButton.SetFocus()
      Start-Sleep -Milliseconds 100
      [System.Windows.Forms.SendKeys]::SendWait(' ')
      $null = Wait-UiElement -Description "$Label document button after focused Space fallback" -TimeoutSeconds 30 -Probe {
        $window = Find-LiveAppWindow
        if ($null -eq $window) { return $null }
        Find-ButtonByNames -Root $window -Names @($Label)
      }
      Write-Host "E1 template confirmation keyboard fallback PASS for '$Label'."
    } catch {
      Write-Host ("E1 UI snapshot after keyboard fallback failure for '$Label': " + (Get-E1UiSnapshot))
      throw
    }
  }
}

function Set-E1DomainSource {
  param([Parameter(Mandatory = $true)][string]$SourcePath)
  $dialog = Invoke-UiActionWithObservedTransition `
    -Description 'replace source for E1 cross-domain scenario' `
    -TransitionDescription 'native source picker for E1 cross-domain scenario' `
    -ActionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Заменить исходный файл', 'Выбрать исходный файл')
    } `
    -TransitionProbe { Find-FileDialog }

  $edit = $dialog.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      '1148'
    )
  )
  Set-UiValue -Element $edit -Value $SourcePath
  Submit-OpenFileDialog -Dialog $dialog
  $sourceName = [System.IO.Path]::GetFileName($SourcePath)
  $null = Wait-UiElement -Description "cross-domain source accepted: $sourceName" -TimeoutSeconds 40 -Probe {
    Find-E1NamedElement -Name $sourceName
  }
}

function Invoke-E1DomainScenario {
  param(
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][System.Collections.IDictionary]$PromptValues,
    [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$PluginRequiredFields,
    [Parameter(Mandatory = $true)][string[]]$ExpectedOutputValues,
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [Parameter(Mandatory = $true)][string]$ReceiptRoot,
    [bool]$ExpectPreflight = $true
  )

  $window = Find-LiveAppWindow
  $clearSelection = Find-ReadyButtonByNames -Root $window -Names @('Снять выбор')
  if ($null -ne $clearSelection) {
    Invoke-UiElementPhysically -Element $clearSelection -Description "clear selection before $Label"
  }
  Start-Sleep -Milliseconds 200

  Invoke-UiActionPhysicallyFromProbe -Description "select $Label" -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    $window.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        "Добавить $Label в комплект"
      )
    )
  }

  $receiptCountBefore = if (Test-Path -LiteralPath $ReceiptRoot -PathType Container) {
    @(Get-ChildItem -LiteralPath $ReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
  } else { 0 }
  $outputName = "$Label.docx"
  $existing = @(Get-ChildItem -LiteralPath $OutputRoot -Recurse -File -Filter $outputName -ErrorAction SilentlyContinue)
  foreach ($item in $existing) { Remove-Item -LiteralPath $item.FullName -Force -ErrorAction SilentlyContinue }

  $generationAction = Wait-UiElement -Description "generation action for $Label" -Probe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByAutomationId -Root $window -AutomationId 'create-selected-documents'
  }
  Invoke-UiElementPhysically -Element $generationAction -Description "start canonical generation for $Label"

  if ($ExpectPreflight) {
    $null = Wait-UiElement -Description "preflight for $Label" -Probe {
      Find-E1NamedElement -Name 'Проверка перед созданием'
    }

    foreach ($fieldId in $PluginRequiredFields) {
      $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
      $control = (Find-LiveAppWindow).FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
          [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
          $automationId
        )
      )
      if ($null -eq $control) {
        throw "E1 $Label did not expose canonical domain-required field: $fieldId"
      }
    }

    foreach ($fieldId in $PromptValues.Keys) {
      $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
      $control = (Find-LiveAppWindow).FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
          [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
          $automationId
        )
      )
      if ($null -eq $control) {
        if ($PluginRequiredFields -contains $fieldId) {
          throw "E1 $Label lost required prompt: $fieldId"
        }
        continue
      }
      $expectedPromptValue = [string]$PromptValues[$fieldId]
      Set-ReactControlledText -Element $control -Value $expectedPromptValue -Description "$Label preflight field $fieldId"
      $control = (Find-LiveAppWindow).FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
          [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
          $automationId
        )
      )
      if ($null -eq $control) {
        throw "E1 $Label lost preflight control after commit: $fieldId"
      }
      $actualPromptValue = Normalize-UiValue -Value (Get-UiValue -Element $control)
      if ($actualPromptValue -ne (Normalize-UiValue -Value $expectedPromptValue)) {
        throw "E1 $Label preflight field did not commit through React: $fieldId expected='$expectedPromptValue' actual='$actualPromptValue'"
      }
    }

    $createAction = Wait-UiElement -Description "create action for $Label" -TimeoutSeconds 30 -Probe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Создать документы')
    }
    if (Test-ElementPhysicallyVisible -Target $createAction) {
      Invoke-UiElementPhysically -Element $createAction -Description "create $Label"
    } else {
      # A fixed preflight footer can sit below Windows WorkingArea when the
      # hosted runner taskbar reduces the physically clickable desktop. Do not
      # synthesize an off-screen mouse click: activate the same real HTML button
      # through browser-native keyboard input, then let physical output/receipt
      # evidence below prove that React actually accepted the action.
      $createAction.SetFocus()
      Start-Sleep -Milliseconds 150
      [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
      Write-Host "E1 keyboard activation used for '$Label' create because the live preflight button was outside Windows WorkingArea."
    }
  } else {
    Write-Host "FPR-07 zero-question path: $Label started directly from the canonical generation action."
  }

  $deadline = [DateTime]::UtcNow.AddSeconds(60)
  $created = $null
  while ($null -eq $created -and [DateTime]::UtcNow -lt $deadline) {
    if (-not $ExpectPreflight) {
      $unexpectedPreflight = Find-E1NamedElement -Name 'Проверка перед созданием'
      if ($null -ne $unexpectedPreflight) {
        throw "FPR-07 zero-question regression: $Label opened a preflight form despite having no user questions."
      }
    }
    $created = Get-ChildItem -LiteralPath $OutputRoot -Recurse -File -Filter $outputName -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $created) {
      $failure = Find-E1NamedElement -Name 'Документы не созданы'
      $failureDetail = Find-E1NamedElementContaining -Text 'Документы не созданы:'
      if ($null -ne $failure -or $null -ne $failureDetail) {
        $failureText = ''
        if ($null -ne $failureDetail) {
          try { $failureText = [string]$failureDetail.Current.Name } catch { $failureText = '' }
        }
        if ([string]::IsNullOrWhiteSpace($failureText)) {
          $statusElement = Find-ElementByAutomationId -Root (Find-LiveAppWindow) -AutomationId 'app-status'
          if ($null -ne $statusElement) {
            try { $failureText = [string]$statusElement.Current.Name } catch { $failureText = '' }
          }
        }
        if ([string]::IsNullOrWhiteSpace($failureText)) { $failureText = Get-E1UiSnapshot }
        throw "E1 $Label backend rejected the completed preflight: $failureText"
      }
      Start-Sleep -Milliseconds 250
    }
  }
  if ($null -eq $created) {
    $statusElement = Find-ElementByAutomationId -Root (Find-LiveAppWindow) -AutomationId 'app-status'
    $statusText = '<missing>'
    if ($null -ne $statusElement) {
      try { $statusText = [string]$statusElement.Current.Name } catch { $statusText = '<unreadable>' }
    }
    $visibleDocx = @(Get-ChildItem -LiteralPath $OutputRoot -Recurse -File -Filter '*.docx' -ErrorAction SilentlyContinue |
      Select-Object -ExpandProperty FullName)
    throw "E1 $Label did not publish a physical DOCX. app-status='$statusText'; visible-docx=$($visibleDocx -join ' | ')"
  }

  $archive = [System.IO.Compression.ZipFile]::OpenRead($created.FullName)
  try {
    $entry = $archive.GetEntry('word/document.xml')
    if ($null -eq $entry) { throw "E1 $Label output is not a readable DOCX package." }
    $reader = [System.IO.StreamReader]::new($entry.Open(), [System.Text.Encoding]::UTF8)
    try { $xml = $reader.ReadToEnd() } finally { $reader.Dispose() }
    foreach ($value in $ExpectedOutputValues) {
      if ($xml -notmatch [regex]::Escape($value)) {
        throw "E1 $Label physical read-back is missing: $value"
      }
    }
    if ($xml -match '\{\{') { throw "E1 $Label physical read-back contains unresolved placeholders." }
  } finally { $archive.Dispose() }

  $outputHash = (Get-FileHash -LiteralPath $created.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  $receiptDeadline = [DateTime]::UtcNow.AddSeconds(30)
  $matched = $false
  while (-not $matched -and [DateTime]::UtcNow -lt $receiptDeadline) {
    if (Test-Path -LiteralPath $ReceiptRoot -PathType Container) {
      foreach ($file in @(Get-ChildItem -LiteralPath $ReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue)) {
        try {
          $data = (Get-Content -LiteralPath $file.FullName -Raw) | ConvertFrom-Json
          if ($data.output_sha256 -eq $outputHash -and $data.status -eq 'committed') {
            $matched = $true
            break
          }
        } catch { }
      }
    }
    if (-not $matched) { Start-Sleep -Milliseconds 250 }
  }
  if (-not $matched) { throw "E1 $Label has no matching committed GenerationReceipt." }

  $receiptCountAfter = @(Get-ChildItem -LiteralPath $ReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
  if ($receiptCountAfter -ne ($receiptCountBefore + 1)) {
    throw "E1 $Label did not add exactly one committed receipt."
  }
  $null = Wait-UiElement -Description "workspace ready after $Label" -TimeoutSeconds 30 -Probe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Добавить шаблоны')
  }
  Write-Host "E1 CROSS-DOMAIN PASS: $Label -> physical DOCX -> committed receipt"
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

Add-E1DomainTemplate `
  -TemplatePath $accountingTemplate `
  -Label $accountingLabel `
  -DomainOption 'Бухгалтерия'

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

$fpr04SourceFields = @(
  'document.number',
  'document.date',
  'org.name',
  'counterparty.name',
  'contract.number',
  'contract.date',
  'contract.subject',
  'amount.total'
)
$fpr04Proof = Wait-UiElement -Description 'FPR-04 recognition provenance proof' -TimeoutSeconds 40 -Probe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-E1NamedElementContaining -Text 'Происхождение данных подтверждено:'
}
$fpr04ProofName = [string]$fpr04Proof.Current.Name
foreach ($fieldId in $fpr04SourceFields) {
  $expectedTrace = "${fieldId}:Scanner/document_text/deterministic_source_parser"
  if (-not $fpr04ProofName.Contains($expectedTrace)) {
    throw "FPR-04 installed recognition provenance missing exact parser trace: $expectedTrace. Actual=$fpr04ProofName"
  }
}
Write-Host "FPR-04 INSTALLED PASS: source-owned Accounting fields preserve Scanner/document_text/deterministic_source_parser provenance through real UI intake."

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
try {
  $null = Invoke-UiActionWithObservedTransition `
    -Description 'open E1 Accounting preflight' `
    -TransitionDescription 'E1 Accounting preflight' `
    -TransitionSeconds 6 `
    -ActionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByAutomationId -Root $window -AutomationId 'create-selected-documents'
    } `
    -TransitionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      $window.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Проверка перед созданием')
      )
    }
} catch {
  $window = Find-LiveAppWindow
  $statusElement = if ($null -ne $window) { Find-ElementByAutomationId -Root $window -AutomationId 'app-status' } else { $null }
  $statusText = if ($null -ne $statusElement) { [string]$statusElement.Current.Name } else { '<missing>' }
  Write-Host "E1 Accounting preflight diagnostic status before canonical shortcut: $statusText"
  Write-Host ("E1 Accounting preflight UI snapshot before canonical shortcut: " + (Get-E1UiSnapshot))

  # Hosted WebView2 can expose an enabled HTML button while swallowing UIA,
  # coordinate click and focused Space. Ctrl+Enter is an application-owned,
  # documented user path wired to the exact same openGenerationPreflight action.
  # It is therefore the stable installed-shell fallback, not a test bypass.
  $process.Refresh()
  $windowHandle = [IntPtr]$process.MainWindowHandle
  if ($windowHandle -ne [IntPtr]::Zero) {
    [void][DokkomplektE1NativeMouse]::ShowWindow($windowHandle, 5)
    [void][DokkomplektE1NativeMouse]::SetForegroundWindow($windowHandle)
    Start-Sleep -Milliseconds 150
  }
  [System.Windows.Forms.SendKeys]::SendWait('^{ENTER}')
  $null = Wait-UiElement -Description 'E1 Accounting preflight through canonical Ctrl+Enter' -TimeoutSeconds 30 -Probe {
    Find-E1NamedElement -Name 'Проверка перед созданием'
  }
  Write-Host "E1 canonical generation shortcut PASS: Ctrl+Enter opened the same Accounting preflight."
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

# FPR-03: the source selected through the real native picker must survive the
# installed publication boundary byte-for-byte. The production path already
# copies the retained SourceSnapshot as "Исходный - <original name>"; prove the
# physical user-visible copy is exactly the source bytes selected for this run.
$publishedAccountingSource = Join-Path $accountingDoc.Directory.FullName ("Исходный - " + $sourceFileName)
if (-not (Test-Path -LiteralPath $publishedAccountingSource -PathType Leaf)) {
  throw "FPR-03 published source copy is missing: $publishedAccountingSource"
}
$publishedAccountingSourceInfo = Get-Item -LiteralPath $publishedAccountingSource -Force
if ($publishedAccountingSourceInfo.Length -le 0) {
  throw "FPR-03 published source copy is empty: $publishedAccountingSource"
}
$accountingSourceHash = (Get-FileHash -LiteralPath $accountingSource -Algorithm SHA256).Hash.ToLowerInvariant()
$publishedAccountingSourceHash = (Get-FileHash -LiteralPath $publishedAccountingSource -Algorithm SHA256).Hash.ToLowerInvariant()
if ($publishedAccountingSourceHash -ne $accountingSourceHash) {
  throw "FPR-03 published source SHA-256 mismatch. Expected=$accountingSourceHash Actual=$publishedAccountingSourceHash"
}
Write-Host "FPR-03 INSTALLED PASS: source picker -> retained snapshot -> published source copy SHA-256 exact."

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

# FPR-21: prove the remaining non-medical built-in/custom domains through the
# exact installed UI/core/generation/receipt path. Medical is proven by the
# immediately preceding Windows installed discharge smoke in the same Quality job.
$crossDomainSource = Join-Path $fixtureDir 'e1-cross-domain-source.docx'
New-E1TextDocx -Path $crossDomainSource -Lines @(
  'Исходный документ для установленной междоменной проверки.',
  'Реквизиты намеренно отсутствуют: доменные обязательные поля должны появиться в preflight.'
)

$domainScenarios = @(
  [pscustomobject]@{
    FileName = 'e1-legal-contract.docx'
    Label = 'E1 Юридический договор'
    DomainOption = 'Юридическая работа'
    CustomProfile = ''
    TemplateLines = @('ДОГОВОР', 'Документ № {{document.number}} от {{document.date}}', 'Договор № {{contract.number}}')
    PromptValues = [ordered]@{
      'document.number' = 'E1-LEGAL-1'
      'document.date' = '14.09.2026'
      'contract.number' = 'K-42'
      'contract.date' = '14.09.2026'
      'contract.party_a' = 'ООО Право-А'
      'contract.party_b' = 'ООО Право-Б'
    }
    PluginRequired = @('contract.date', 'contract.party_a', 'contract.party_b')
    Expected = @('E1-LEGAL-1', '14.09.2026', 'K-42')
  },
  [pscustomobject]@{
    FileName = 'e1-hr-employment-contract.docx'
    Label = 'E1 Трудовой договор'
    DomainOption = 'Кадровая работа'
    CustomProfile = ''
    TemplateLines = @('ТРУДОВОЙ ДОГОВОР', 'Документ № {{document.number}} от {{document.date}}', 'Сотрудник: {{employee.name}}')
    PromptValues = [ordered]@{
      'document.number' = 'E1-HR-1'
      'document.date' = '15.09.2026'
      'org.name' = 'ООО Кадры'
      'employee.name' = 'Сидоров Сергей Сергеевич'
      'employee.position' = 'аналитик'
      'employee.hire_date' = '16.09.2026'
      'employee.contract_number' = 'TD-51'
    }
    PluginRequired = @('org.name', 'employee.position', 'employee.hire_date', 'employee.contract_number')
    Expected = @('E1-HR-1', '15.09.2026', 'Сидоров Сергей Сергеевич')
  },
  [pscustomobject]@{
    FileName = 'e1-education-certificate.docx'
    Label = 'E1 Справка об обучении'
    DomainOption = 'Образование'
    CustomProfile = ''
    TemplateLines = @('СПРАВКА ОБ ОБУЧЕНИИ', 'Справка № {{document.number}}', 'Обучающийся: {{education.student_name}}')
    PromptValues = [ordered]@{
      'document.number' = 'E1-EDU-1'
      'document.date' = '16.09.2026'
      'education.student_name' = 'Орлова Анна Игоревна'
      'education.institution' = 'Учебный центр Эталон'
    }
    PluginRequired = @('document.date', 'education.institution')
    Expected = @('E1-EDU-1', 'Орлова Анна Игоревна')
  },
  [pscustomobject]@{
    FileName = 'e1-custom-architect.docx'
    Label = 'E1 Архитектурное заключение'
    DomainOption = 'Своя профессия / профиль'
    CustomProfile = 'Архитектор'
    TemplateLines = @('АРХИТЕКТУРНОЕ ЗАКЛЮЧЕНИЕ', 'Документ № {{document.number}} от {{document.date}}', 'Проект: {{custom.project}}')
    PromptValues = [ordered]@{
      'document.number' = 'E1-CUSTOM-1'
      'document.date' = '17.09.2026'
      'custom.project' = 'Проект Север'
    }
    PluginRequired = @()
    Expected = @('E1-CUSTOM-1', '17.09.2026', 'Проект Север')
  }
)

foreach ($scenario in $domainScenarios) {
  # Every profession scenario is a fresh case. Reusing the previous Accounting
  # semantic case would make source-owned values such as contract.date appear
  # already satisfied and would weaken the installed required-field proof.
  $null = Invoke-UiActionWithObservedTransition `
    -Description "reset case before $($scenario.Label)" `
    -TransitionDescription "empty case before $($scenario.Label)" `
    -ActionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Новый комплект', 'Новый пациент / дело')
    } `
    -TransitionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Выбрать исходный файл')
    }

  $templatePath = Join-Path $fixtureDir $scenario.FileName
  New-E1TextDocx -Path $templatePath -Lines $scenario.TemplateLines
  Add-E1DomainTemplate `
    -TemplatePath $templatePath `
    -Label $scenario.Label `
    -DomainOption $scenario.DomainOption `
    -CustomProfile $scenario.CustomProfile
  Set-E1DomainSource -SourcePath $crossDomainSource
  Invoke-E1DomainScenario `
    -Label $scenario.Label `
    -PromptValues $scenario.PromptValues `
    -PluginRequiredFields $scenario.PluginRequired `
    -ExpectedOutputValues $scenario.Expected `
    -OutputRoot $defaultOutputRoot `
    -ReceiptRoot $completionReceiptRoot
}
# FPR-01: prove the installed main-document batch boundary, not merely a
# sequence of single-document generations. Two ordinary Universal templates are
# selected together, share one preflight, and must each produce a readable
# physical DOCX plus its own committed GenerationReceipt.
$null = Invoke-UiActionWithObservedTransition `
  -Description 'reset case before FPR-01 main-document batch' `
  -TransitionDescription 'empty case before FPR-01 main-document batch' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Новый комплект', 'Новый пациент / дело')
  } `
  -TransitionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Выбрать исходный файл')
  }

$fpr01Documents = @(
  [pscustomobject]@{
    FileName = 'fpr01-main-a.docx'
    Label = 'FPR01 Основной документ А'
    Marker = 'FPR01-MAIN-A'
  },
  [pscustomobject]@{
    FileName = 'fpr01-main-b.docx'
    Label = 'FPR01 Основной документ Б'
    Marker = 'FPR01-MAIN-B'
  }
)
foreach ($document in $fpr01Documents) {
  $templatePath = Join-Path $fixtureDir $document.FileName
  New-E1TextDocx -Path $templatePath -Lines @(
    $document.Marker,
    'Документ № {{document.number}}',
    'Дата документа: {{document.date}}'
  )
  Add-E1DomainTemplate `
    -TemplatePath $templatePath `
    -Label $document.Label `
    -DomainOption 'Универсальный документооборот'
}

Set-E1DomainSource -SourcePath $crossDomainSource
$window = Find-LiveAppWindow
$clearSelection = Find-ReadyButtonByNames -Root $window -Names @('Снять выбор')
if ($null -ne $clearSelection) {
  Invoke-UiElementPhysically -Element $clearSelection -Description 'clear selection before FPR-01 batch'
}
Start-Sleep -Milliseconds 200

$fpr01SelectedCount = 0
foreach ($document in $fpr01Documents) {
  $fpr01SelectedCount += 1
  $expectedActionNames = @(
    "Проверить и создать ($fpr01SelectedCount)",
    "Создать документы ($fpr01SelectedCount)"
  )
  Invoke-UiActionWithObservedTransition `
    -Description "select $($document.Label) for FPR-01 batch" `
    -TransitionDescription "FPR-01 selection count $fpr01SelectedCount" `
    -ActionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      $window.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
          [System.Windows.Automation.AutomationElement]::NameProperty,
          "Добавить $($document.Label) в комплект"
        )
      )
    } `
    -TransitionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names $expectedActionNames
    } | Out-Null
}

$null = Invoke-UiActionWithObservedTransition `
  -Description 'FPR-01 two-document generation action' `
  -TransitionDescription 'FPR-01 shared preflight' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Проверить и создать (2)', 'Создать документы (2)')
  } `
  -TransitionProbe {
    Find-E1NamedElement -Name 'Проверка перед созданием'
  }

$fpr01PromptValues = [ordered]@{
  'document.number' = 'FPR01-2DOCS'
  'document.date' = '18.09.2026'
}
foreach ($fieldId in $fpr01PromptValues.Keys) {
  $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
  $control = (Find-LiveAppWindow).FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      $automationId
    )
  )
  if ($null -eq $control) {
    throw "FPR-01 shared preflight did not expose required field: $fieldId"
  }
  Set-UiValue -Element $control -Value ([string]$fpr01PromptValues[$fieldId])
}

$fpr01ReceiptCountBefore = if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
  @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
} else { 0 }
foreach ($document in $fpr01Documents) {
  $outputName = "$($document.Label).docx"
  foreach ($existing in @(Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $outputName -ErrorAction SilentlyContinue)) {
    Remove-Item -LiteralPath $existing.FullName -Force -ErrorAction SilentlyContinue
  }
}

Invoke-UiActionPhysicallyFromProbe -Description 'create FPR-01 two-document batch' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Создать документы')
}

$fpr01Deadline = [DateTime]::UtcNow.AddSeconds(75)
$fpr01Created = @{}
do {
  if ($process.HasExited) { throw 'Installed application exited during FPR-01 two-document generation.' }
  foreach ($document in $fpr01Documents) {
    if ($fpr01Created.ContainsKey($document.Label)) { continue }
    $outputName = "$($document.Label).docx"
    $created = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $outputName -ErrorAction SilentlyContinue |
      Select-Object -First 1
    if ($null -ne $created) { $fpr01Created[$document.Label] = $created }
  }
  if ($fpr01Created.Count -eq $fpr01Documents.Count) { break }
  $failure = Find-E1NamedElement -Name 'Документы не созданы'
  if ($null -ne $failure) { throw 'FPR-01 installed backend rejected the two-document batch.' }
  Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $fpr01Deadline)

if ($fpr01Created.Count -ne $fpr01Documents.Count) {
  $missing = @($fpr01Documents | Where-Object { -not $fpr01Created.ContainsKey($_.Label) } | ForEach-Object Label) -join ', '
  throw "FPR-01 did not publish every selected document. Missing: $missing"
}

$fpr13BundleFolders = @($fpr01Created.Values | ForEach-Object { $_.Directory.FullName } | Sort-Object -Unique)
if ($fpr13BundleFolders.Count -ne 1) {
  throw "FPR-13 selected documents were not published into one physical bundle folder: $($fpr13BundleFolders -join ' | ')"
}

$fpr01OutputHashes = New-Object System.Collections.Generic.HashSet[string]
foreach ($document in $fpr01Documents) {
  $created = $fpr01Created[$document.Label]
  if ($created.Length -le 0) { throw "FPR-01 created empty DOCX: $($created.FullName)" }
  $archive = [System.IO.Compression.ZipFile]::OpenRead($created.FullName)
  try {
    $entry = $archive.GetEntry('word/document.xml')
    if ($null -eq $entry) { throw "FPR-01 output is not a readable DOCX: $($created.FullName)" }
    $reader = [System.IO.StreamReader]::new($entry.Open(), [System.Text.Encoding]::UTF8)
    try { $xml = $reader.ReadToEnd() } finally { $reader.Dispose() }
    foreach ($expected in @($document.Marker, 'FPR01-2DOCS', '18.09.2026')) {
      if ($xml -notmatch [regex]::Escape($expected)) {
        throw "FPR-01 physical read-back for '$($document.Label)' is missing: $expected"
      }
    }
    if ($xml -match '\{\{') {
      throw "FPR-01 physical read-back for '$($document.Label)' contains unresolved placeholders."
    }
  } finally {
    $archive.Dispose()
  }
  $hash = (Get-FileHash -LiteralPath $created.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  if (-not $fpr01OutputHashes.Add($hash)) {
    throw 'FPR-01 selected documents unexpectedly produced identical physical output hashes.'
  }
}

$fpr01ReceiptDeadline = [DateTime]::UtcNow.AddSeconds(30)
$fpr01MatchedHashes = New-Object System.Collections.Generic.HashSet[string]
$fpr13PlanBindings = New-Object System.Collections.Generic.HashSet[string]
do {
  if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
    foreach ($file in @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue)) {
      try {
        $data = (Get-Content -LiteralPath $file.FullName -Raw) | ConvertFrom-Json
        $receiptHash = [string]$data.output_sha256
        if ($data.status -eq 'committed' -and $fpr01OutputHashes.Contains($receiptHash)) {
          $null = $fpr01MatchedHashes.Add($receiptHash)
          $planBinding = [string]$data.plan_binding_sha256
          if ([string]::IsNullOrWhiteSpace($planBinding) -or $planBinding.Length -ne 64) {
            throw "FPR-13 committed receipt for output $receiptHash has no complete plan binding."
          }
          $null = $fpr13PlanBindings.Add($planBinding)
        }
      } catch { }
    }
  }
  if ($fpr01MatchedHashes.Count -eq $fpr01Documents.Count) { break }
  Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $fpr01ReceiptDeadline)

if ($fpr01MatchedHashes.Count -ne $fpr01Documents.Count) {
  throw "FPR-01 expected $($fpr01Documents.Count) committed receipts bound to the selected physical outputs; matched $($fpr01MatchedHashes.Count)."
}
$fpr01ReceiptCountAfter = @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
if ($fpr01ReceiptCountAfter -ne ($fpr01ReceiptCountBefore + $fpr01Documents.Count)) {
  throw "FPR-01 one batch did not add exactly $($fpr01Documents.Count) committed GenerationReceipts."
}
if ($fpr13PlanBindings.Count -ne 1) {
  throw "FPR-13 two outputs do not share one immutable plan binding; distinct bindings=$($fpr13PlanBindings.Count)."
}
$fpr13BundleStatus = Wait-UiElement -Description 'FPR-13 honest bundle completion status' -TimeoutSeconds 15 -Probe {
  Find-E1NamedElementContaining -Text 'Комплект создан: 2 документ(ов)'
}
if ($null -eq $fpr13BundleStatus) {
  throw 'FPR-13 UI did not report the actual two-document bundle size.'
}
Write-Host "FPR-13 INSTALLED PASS: one selected bundle -> one physical folder '$($fpr13BundleFolders[0])' -> 2 distinct readable DOCX -> one shared plan binding -> honest UI count -> 2 committed receipts."
Write-Host 'FPR-01 INSTALLED PASS: one shared preflight -> 2 selected main documents -> 2 readable DOCX -> 2 committed receipts.'

# FPR-02: prove the real installed medical diary path. The registered diary
# button must route through the single program-calendar template, doctor-owned
# Texts library and canonical preflight; the physical DOCX must remain paragraph
# text (not the retired legacy table engine), start at D0+1, stop on discharge,
# preserve the dedicated final row, both signature blocks and centered date paragraphs.
$null = Invoke-UiActionWithObservedTransition `
  -Description 'reset case before FPR-02 diary proof' `
  -TransitionDescription 'empty case before FPR-02 diary proof' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Новый комплект', 'Новый пациент / дело')
  } `
  -TransitionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Выбрать исходный файл')
  }

$fpr02DiaryLabel = 'Дневники FPR02'
$fpr02DiaryTemplate = Join-Path $fixtureDir 'fpr02-diaries.docx'
New-E1TextDocx -Path $fpr02DiaryTemplate -Lines @(
  'Дневники',
  'Календарные дневниковые записи пациента'
)
Add-E1DomainTemplate `
  -TemplatePath $fpr02DiaryTemplate `
  -Label $fpr02DiaryLabel `
  -DomainOption 'Медицина'

$fpr02Source = Join-Path $fixtureDir 'fpr02-медицинский-источник.docx'
New-E1TextDocx -Path $fpr02Source -Lines @(
  'Первичный осмотр',
  'Ф.И.О.: Петров Пётр Петрович',
  'Дата рождения: 02.02.1982',
  'Дата поступления: 10.05.2026',
  'Дата выписки: 13.05.2026',
  'Диагноз: F20.0 Параноидная шизофрения',
  'Лечение: рисперидон 4 мг/сут'
)
Set-E1DomainSource -SourcePath $fpr02Source

$window = Find-LiveAppWindow
$clearSelection = Find-ReadyButtonByNames -Root $window -Names @('Снять выбор')
if ($null -ne $clearSelection) {
  Invoke-UiElementPhysically -Element $clearSelection -Description 'clear selection before FPR-02 diary'
}
Start-Sleep -Milliseconds 200
Invoke-UiActionPhysicallyFromProbe -Description 'select FPR-02 diary document' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  $window.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      "Добавить $fpr02DiaryLabel в комплект"
    )
  )
}
$null = Wait-UiElement -Description 'medical diary additional sources panel' -TimeoutSeconds 30 -Probe {
  Find-E1NamedElement -Name 'Медицинские дневники'
}

$fpr02DiaryText = Join-Path $fixtureDir 'fpr02-regular-diary.docx'
$fpr02DoctorText = 'FPR02 профессиональный текст дневника, подтверждённый врачом.'
New-E1TextDocx -Path $fpr02DiaryText -Lines @($fpr02DoctorText)
$diaryTextDialog = Invoke-UiActionWithObservedTransition `
  -Description 'FPR-02 Тексты' `
  -TransitionDescription 'native diary Texts picker' `
  -ActionProbe {
    Find-E1NamedElement -Name 'Тексты'
  } `
  -TransitionProbe { Find-FileDialog }
$null = Set-OpenFileDialogPath -Dialog $diaryTextDialog -Path $fpr02DiaryText
Submit-OpenFileDialog -Dialog $diaryTextDialog

$fpr02ImportDeadline = [DateTime]::UtcNow.AddSeconds(40)
$fpr02ImportStatusText = ''
$fpr02ImportPassed = $false
do {
  $fpr02ImportStatus = Find-E1ElementByAutomationId -AutomationId 'additional-materials-status'
  if ($null -eq $fpr02ImportStatus) {
    $fpr02ImportStatus = Find-E1NamedElementContaining -Text 'Тексты'
  }
  if ($null -ne $fpr02ImportStatus) {
    try { $fpr02ImportStatusText = [string]$fpr02ImportStatus.Current.Name } catch { $fpr02ImportStatusText = '' }
    if ($fpr02ImportStatusText -match 'сохранено\s+1\s+из\s+1' -and $fpr02ImportStatusText -match 'ошибок\s+0') {
      $fpr02ImportPassed = $true
      break
    }
    if ($fpr02ImportStatusText -match 'Не удалось|Ошибка|ошибок\s+[1-9]') {
      throw "FPR-02 diary text import failed: $fpr02ImportStatusText"
    }
  }
  Start-Sleep -Milliseconds 150
} while ([DateTime]::UtcNow -lt $fpr02ImportDeadline)

if (-not $fpr02ImportPassed) {
  $snapshot = Get-E1UiSnapshot
  throw "FPR-02 diary text import did not reach terminal success. Last status='$fpr02ImportStatusText'. UI=$snapshot"
}
if ($fpr02ImportStatusText -notmatch 'F20\.0') {
  throw "FPR-02 diary Texts were not bound to the source-owned diagnosis F20.0: $fpr02ImportStatusText"
}
Write-Host "FPR-02 Texts import PASS: $fpr02ImportStatusText"

$null = Invoke-UiActionWithObservedTransition `
  -Description 'FPR-02 diary generation action' `
  -TransitionDescription 'FPR-02 diary preflight' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Проверить и создать (1)', 'Создать документы (1)')
  } `
  -TransitionProbe { Find-E1NamedElement -Name 'Проверка перед созданием' }

# Source-owned values are intentionally absent from the preflight prompt list.
# Workflow PromptAskMode::IfMissing suppresses a question when SemanticCase already
# contains the source value. Re-prompting these fields would regress the donor UX
# and would let the test overwrite the very recognition result it is meant to prove.
foreach ($fieldId in @('medical.admission_date', 'medical.discharge_date', 'medical.diagnosis', 'medical.case_number')) {
  $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
  $control = (Find-LiveAppWindow).FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      $automationId
    )
  )
  if ($null -ne $control) {
    throw "FPR-02 diary role unexpectedly re-prompted a source/non-diary field: $fieldId"
  }
}

# A fresh universal install uses the canonical default output identity
# DocumentNumber + DocumentDate. Only still-missing values may be prompted:
# this source has no document number, but its admitted primary-inspection date is
# already recognized as document.date=10.05.2026 and must not be re-asked.
$fpr02NumberControl = (Find-LiveAppWindow).FindFirst(
  [System.Windows.Automation.TreeScope]::Descendants,
  [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
    'workflow-document-number'
  )
)
if ($null -eq $fpr02NumberControl) {
  throw 'FPR-02 missing the still-required default output-folder document.number prompt.'
}
Set-UiValue -Element $fpr02NumberControl -Value 'FPR02-42'

$fpr02DateControl = (Find-LiveAppWindow).FindFirst(
  [System.Windows.Automation.TreeScope]::Descendants,
  [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
    'workflow-document-date'
  )
)
if ($null -ne $fpr02DateControl) {
  throw 'FPR-02 re-prompted source-owned document.date instead of reusing 10.05.2026.'
}
Write-Host 'FPR-02 folder identity preflight PASS: missing number requested; source-owned document date not re-prompted.'
Write-Host 'FPR-07 PROMPT INSTALLED PASS: missing document.number produced a visible canonical preflight question.'

$fpr02PromptValues = [ordered]@{
  'medical.diary_schedule_style' = 'Каждый день'
  'medical.diary_intraday_rhythm' = 'Один раз в день'
}
foreach ($fieldId in $fpr02PromptValues.Keys) {
  $automationId = 'workflow-' + ($fieldId -replace '[^a-zA-Z0-9_-]', '-')
  $control = (Find-LiveAppWindow).FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
      $automationId
    )
  )
  if ($null -eq $control) { throw "FPR-02 missing required diary decision: $fieldId" }
  Set-UiValue -Element $control -Value $fpr02PromptValues[$fieldId]
}

$fpr02SickLeaveId = 'workflow-medical-diary_sick_leave_epicrisis'
$fpr02SickLeave = (Find-LiveAppWindow).FindFirst(
  [System.Windows.Automation.TreeScope]::Descendants,
  [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
    $fpr02SickLeaveId
  )
)
if ($null -eq $fpr02SickLeave) { throw 'FPR-02 preflight did not expose the donor sick-leave decision.' }
$fpr02SickLeave.SetFocus()
Start-Sleep -Milliseconds 100
[System.Windows.Forms.SendKeys]::SendWait('{HOME}')
[System.Windows.Forms.SendKeys]::SendWait('{DOWN}')
[System.Windows.Forms.SendKeys]::SendWait('{TAB}')
Start-Sleep -Milliseconds 250
$fpr02SickLeaveValue = Normalize-UiValue -Value (Get-UiValue -Element $fpr02SickLeave)
if ($fpr02SickLeaveValue -ne 'Нет') {
  throw "FPR-02 sick-leave decision did not commit as 'Нет': '$fpr02SickLeaveValue'"
}

$fpr02OutputName = "$fpr02DiaryLabel.docx"
foreach ($existing in @(Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $fpr02OutputName -ErrorAction SilentlyContinue)) {
  Remove-Item -LiteralPath $existing.FullName -Force -ErrorAction SilentlyContinue
}
$fpr02ReceiptCountBefore = if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
  @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
} else { 0 }

Invoke-UiActionPhysicallyFromProbe -Description 'create FPR-02 diary DOCX' -ActionProbe {
  $window = Find-LiveAppWindow
  if ($null -eq $window) { return $null }
  Find-ReadyButtonByNames -Root $window -Names @('Создать документы')
}
$fpr02Deadline = [DateTime]::UtcNow.AddSeconds(75)
$fpr02Doc = $null
do {
  if ($process.HasExited) { throw 'Installed application exited during FPR-02 diary generation.' }
  $fpr02Doc = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter $fpr02OutputName -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($null -ne $fpr02Doc) { break }
  $failure = Find-E1NamedElement -Name 'Документы не созданы'
  if ($null -ne $failure) {
    $failureDetail = Find-E1NamedElementContaining -Text 'Документы не созданы:'
    $failureText = ''
    if ($null -ne $failureDetail) {
      try { $failureText = [string]$failureDetail.Current.Name } catch { $failureText = '' }
    }
    if ([string]::IsNullOrWhiteSpace($failureText)) {
      $failureText = Get-E1UiSnapshot
    }
    throw "FPR-02 generation failed after accepted preflight: $failureText"
  }
  Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $fpr02Deadline)
if ($null -eq $fpr02Doc) { throw 'FPR-02 did not publish a physical diary DOCX.' }
$fpr02ExpectedFolder = 'FPR02-42 10.05.2026'
if ($fpr02Doc.Directory.Name -ne $fpr02ExpectedFolder) {
  throw "FPR-02 output folder did not preserve missing-number + source-date identity. Expected='$fpr02ExpectedFolder' Actual='$($fpr02Doc.Directory.Name)'."
}
Write-Host "FPR-02 folder identity PASS: $fpr02ExpectedFolder"

$archive = [System.IO.Compression.ZipFile]::OpenRead($fpr02Doc.FullName)
try {
  $entry = $archive.GetEntry('word/document.xml')
  if ($null -eq $entry) { throw 'FPR-02 output is not a readable DOCX package.' }
  $reader = [System.IO.StreamReader]::new($entry.Open(), [System.Text.Encoding]::UTF8)
  try { $fpr02Xml = $reader.ReadToEnd() } finally { $reader.Dispose() }
} finally { $archive.Dispose() }

if ($fpr02Xml -match '<w:tbl') { throw 'FPR-02 canonical diary output regressed to a Word table.' }
foreach ($date in @('11.05.2026', '12.05.2026', '13.05.2026')) {
  if ($fpr02Xml -notmatch [regex]::Escape($date)) { throw "FPR-02 diary schedule is missing $date" }
  $datePattern = '<w:p(?:\s[^>]*)?>(?:(?!</w:p>).)*?' + [regex]::Escape($date) + '(?:(?!</w:p>).)*?</w:p>'
  $dateParagraph = [regex]::Match($fpr02Xml, $datePattern, [System.Text.RegularExpressions.RegexOptions]::Singleline)
  if (-not $dateParagraph.Success -or $dateParagraph.Value -notmatch '<w:jc\s+w:val="center"\s*/>') {
    throw "FPR-02 diary date is not centered in paragraph text: $date"
  }
}
if ($fpr02Xml -match [regex]::Escape('10.05.2026')) { throw 'FPR-02 incorrectly emitted an ordinary diary on admission day D0.' }
if ($fpr02Xml -match [regex]::Escape('14.05.2026')) { throw 'FPR-02 emitted a diary after the discharge boundary.' }
$doctorTextCount = [regex]::Matches($fpr02Xml, [regex]::Escape($fpr02DoctorText)).Count
if ($doctorTextCount -ne 2) { throw "FPR-02 expected doctor-owned regular text on exactly two ordinary rows, got $doctorTextCount." }
if ($fpr02Xml -notmatch [regex]::Escape('На текущую дату оформлена выписка из стационара.')) {
  throw 'FPR-02 final discharge diary text is missing.'
}
if ([regex]::Matches($fpr02Xml, 'Лечащий врач').Count -ne 3) {
  throw 'FPR-02 did not retain the treating-physician signature on every diary row.'
}
if ([regex]::Matches($fpr02Xml, 'Заведующий отделением').Count -ne 3) {
  throw 'FPR-02 did not retain the department-head signature on every diary row.'
}
if ($fpr02Xml -match '\{\{') { throw 'FPR-02 physical diary DOCX contains unresolved placeholders.' }

$fpr02Hash = (Get-FileHash -LiteralPath $fpr02Doc.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$fpr02ReceiptDeadline = [DateTime]::UtcNow.AddSeconds(30)
$fpr02ReceiptMatched = $false
do {
  if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
    foreach ($file in @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue)) {
      try {
        $data = (Get-Content -LiteralPath $file.FullName -Raw) | ConvertFrom-Json
        if ($data.status -eq 'committed' -and $data.output_sha256 -eq $fpr02Hash) {
          $fpr02ReceiptMatched = $true
          break
        }
      } catch { }
    }
  }
  if (-not $fpr02ReceiptMatched) { Start-Sleep -Milliseconds 250 }
} while (-not $fpr02ReceiptMatched -and [DateTime]::UtcNow -lt $fpr02ReceiptDeadline)
if (-not $fpr02ReceiptMatched) { throw 'FPR-02 diary output has no matching committed GenerationReceipt.' }
$fpr02ReceiptCountAfter = @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue).Count
if ($fpr02ReceiptCountAfter -ne ($fpr02ReceiptCountBefore + 1)) {
  throw 'FPR-02 diary generation did not add exactly one committed GenerationReceipt.'
}
Write-Host 'FPR-02 INSTALLED PASS: Texts -> D0+1..discharge paragraph diary -> centered dates -> doctor text/final row -> 2 signatures per row -> committed receipt.'

# FPR-05: prove the installed ICD-10 path end to end. The source deliberately
# starts with a different diagnosis. The specialist must search the bundled
# offline catalog, choose F20.0 through the live UI, and the selected code/title
# must reach a physical generated DOCX through the same SemanticCase.
$null = Invoke-UiActionWithObservedTransition `
  -Description 'reset case before FPR-05 ICD-10 proof' `
  -TransitionDescription 'empty case before FPR-05 ICD-10 proof' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Новый комплект', 'Новый пациент / дело')
  } `
  -TransitionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Выбрать исходный файл')
  }

$fpr05Label = 'МКБ-10 FPR05'
$fpr05Template = Join-Path $fixtureDir 'fpr05-icd10.docx'
New-E1TextDocx -Path $fpr05Template -Lines @(
  'Проверка МКБ-10',
  'Пациент: {{subject.name}}',
  'Код МКБ-10: {{medical.icd10}}',
  'Диагноз: {{medical.diagnosis}}'
)
Add-E1DomainTemplate -TemplatePath $fpr05Template -Label $fpr05Label -DomainOption 'Медицина'

$fpr05Source = Join-Path $fixtureDir 'fpr05-медицинский-источник.docx'
New-E1TextDocx -Path $fpr05Source -Lines @(
  'Первичный осмотр',
  'Номер документа: FPR05-77',
  'Дата документа: 15.05.2026',
  'Ф.И.О.: Смирнов Сергей Сергеевич',
  'Дата рождения: 03.03.1983',
  'Дата поступления: 15.05.2026',
  'Дата выписки: 18.05.2026',
  'Диагноз: F41.1 Генерализованное тревожное расстройство',
  'Лечение: терапия по назначению врача'
)
Set-E1DomainSource -SourcePath $fpr05Source

$settingsButton = Wait-UiElement -Description 'FPR-05 settings button' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Настройки'
}
Invoke-UiElementPhysically -Element $settingsButton -Description 'open settings for FPR-05'
$expertButton = Wait-UiElement -Description 'FPR-05 expert tools toggle' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Экспертные и административные инструменты'
}
Invoke-UiElementPhysically -Element $expertButton -Description 'open expert tools for FPR-05'

$fpr05Search = Wait-UiElement -Description 'FPR-05 ICD search input' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Поиск МКБ-10'
}
Set-UiValue -Element $fpr05Search -Value 'F20.0'
Invoke-UiActionPhysicallyFromProbe -Description 'search FPR-05 ICD-10' -ActionProbe {
  Find-E1NamedElement -Name 'Найти по МКБ-10'
}
$fpr05Hit = Wait-UiElement -Description 'FPR-05 ICD F20.0 result' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Выбрать МКБ-10 F20.0'
}
Invoke-UiElementPhysically -Element $fpr05Hit -Description 'choose FPR-05 ICD F20.0'

$fpr05Selected = ''
$fpr05CommitDeadline = [DateTime]::UtcNow.AddSeconds(20)
do {
  $fpr05Search = Find-E1NamedElement -Name 'Поиск МКБ-10'
  if ($null -ne $fpr05Search) {
    $fpr05Selected = Normalize-UiValue -Value (Get-UiValue -Element $fpr05Search)
    if ($fpr05Selected.StartsWith('F20.0 ') -and $fpr05Selected.Length -gt 6) { break }
  }
  Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $fpr05CommitDeadline)
if (-not $fpr05Selected.StartsWith('F20.0 ') -or $fpr05Selected.Length -le 6) {
  throw "FPR-05 ICD selection did not commit code + title to the live input: '$fpr05Selected'"
}
$fpr05SelectedTitle = $fpr05Selected.Substring(6).Trim()
if ([string]::IsNullOrWhiteSpace($fpr05SelectedTitle)) {
  throw "FPR-05 ICD selection returned an empty title: '$fpr05Selected'"
}
if ($fpr05Selected -match 'F41\.1|тревож') {
  throw "FPR-05 ICD selection did not replace the source diagnosis: '$fpr05Selected'"
}
Write-Host "FPR-05 ICD selection PASS: $fpr05Selected"

$settingsButton = Wait-UiElement -Description 'FPR-05 close settings button' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Настройки'
}
Invoke-UiElementPhysically -Element $settingsButton -Description 'close settings after FPR-05 ICD selection'

Invoke-E1DomainScenario `
  -Label $fpr05Label `
  -PromptValues @{} `
  -PluginRequiredFields @() `
  -ExpectedOutputValues @('F20.0', $fpr05SelectedTitle) `
  -OutputRoot $defaultOutputRoot `
  -ReceiptRoot $completionReceiptRoot `
  -ExpectPreflight $false

$fpr05Doc = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter "$fpr05Label.docx" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $fpr05Doc) { throw 'FPR-05 installed path did not publish its physical DOCX.' }
$fpr05Archive = [System.IO.Compression.ZipFile]::OpenRead($fpr05Doc.FullName)
try {
  $fpr05Entry = $fpr05Archive.GetEntry('word/document.xml')
  if ($null -eq $fpr05Entry) { throw 'FPR-05 output is not a readable DOCX package.' }
  $fpr05Reader = [System.IO.StreamReader]::new($fpr05Entry.Open(), [System.Text.Encoding]::UTF8)
  try { $fpr05Xml = $fpr05Reader.ReadToEnd() } finally { $fpr05Reader.Dispose() }
} finally { $fpr05Archive.Dispose() }
if ($fpr05Xml -match 'F41\.1|Генерализованное тревожное расстройство') {
  throw 'FPR-05 physical output retained the source diagnosis instead of the selected ICD-10 value.'
}
Write-Host "FPR-05 INSTALLED PASS: bundled ICD-10 search -> F20.0 selection -> canonical SemanticCase -> physical DOCX code/title -> committed receipt."

# FPR-06: prove the installed Scanner path through the same canonical SemanticCase.
# The hosted Windows packaging runner is not allowed to depend on Microsoft Word,
# so this exercises the product's installed manual Scanner entry. Guided Word
# Scanner confirmation converges on the same apply_scanner command in App.tsx.
$null = Invoke-UiActionWithObservedTransition `
  -Description 'reset case before FPR-06 Scanner proof' `
  -TransitionDescription 'empty case before FPR-06 Scanner proof' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Новый комплект', 'Новый пациент / дело')
  } `
  -TransitionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Выбрать исходный файл')
  }

$fpr06Label = 'Scanner FPR06'
$fpr06FieldId = 'custom.scanner_value'
$fpr06ScannerValue = 'SCANNER-FPR06-VALUE'
$fpr06Template = Join-Path $fixtureDir 'fpr06-scanner.docx'
New-E1TextDocx -Path $fpr06Template -Lines @(
  'Проверка Scanner',
  'Значение сканера: {{custom.scanner_value}}'
)
Add-E1DomainTemplate -TemplatePath $fpr06Template -Label $fpr06Label -DomainOption 'Универсальный документооборот'

$fpr06Source = Join-Path $fixtureDir 'fpr06-scanner-source.docx'
New-E1TextDocx -Path $fpr06Source -Lines @(
  'Источник для проверки Scanner',
  'Номер документа: FPR06-1',
  'Дата документа: 16.05.2026',
  "Фрагмент для разметки: $fpr06ScannerValue"
)
Set-E1DomainSource -SourcePath $fpr06Source

$fpr06FieldInput = Find-E1NamedElement -Name 'Идентификатор поля'
if ($null -eq $fpr06FieldInput) {
  $fpr06Advanced = Wait-UiElement -Description 'FPR-06 advanced tools toggle' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Расширенные инструменты'
  }
  Invoke-UiElementPhysically -Element $fpr06Advanced -Description 'open FPR-06 manual Scanner tools'
  $fpr06FieldInput = Wait-UiElement -Description 'FPR-06 Scanner field input' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Идентификатор поля'
  }
}
Set-UiValue -Element $fpr06FieldInput -Value $fpr06FieldId
$fpr06TextInput = Wait-UiElement -Description 'FPR-06 Scanner text input' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Выделенный текст'
}
Set-UiValue -Element $fpr06TextInput -Value $fpr06ScannerValue
Invoke-UiActionPhysicallyFromProbe -Description 'apply FPR-06 Scanner value' -ActionProbe {
  Find-E1NamedElement -Name 'Назначить выделение полю'
}
$fpr06Applied = Wait-UiElement -Description 'FPR-06 Scanner accepted status' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElementContaining -Text 'Разметка сохранена: принято 1'
}
if ($null -eq $fpr06Applied) { throw 'FPR-06 Scanner UI did not confirm one applied value.' }
Write-Host "FPR-06 Scanner UI PASS: $fpr06FieldId=$fpr06ScannerValue"

Invoke-E1DomainScenario `
  -Label $fpr06Label `
  -PromptValues @{} `
  -PluginRequiredFields @() `
  -ExpectedOutputValues @($fpr06ScannerValue) `
  -OutputRoot $defaultOutputRoot `
  -ReceiptRoot $completionReceiptRoot `
  -ExpectPreflight $false
Write-Host 'FPR-06 INSTALLED PASS: manual Scanner UI -> canonical apply_scanner -> SemanticCase -> physical DOCX -> committed receipt.'
Write-Host 'FPR-07 ZERO-QUESTION INSTALLED PASS: fully resolved Scanner case skipped the form and published directly after commit-boundary recheck.'
Write-Host 'FPR-07 INSTALLED PASS: missing-value case prompts; zero-question case has no form; both publish through the same canonical workflow.'
Write-Host 'E1 FPR-21 PASS: installed Medical/Legal/HR/Accounting/Education/Custom compatibility is covered on one core.'

# FPR-15 / ACC-61: real installed clean-profile transfer proof.
# Export the current confirmed template set while a case-only sentinel exists,
# prove that the package contains no case state or local paths, destroy the
# temporary application profile, import through the installed UI, and generate
# a physical DOCX from a new case with the imported button.
$fpr15OldCaseSentinel = $fpr06ScannerValue
$fpr15ExistingPackages = @{}
foreach ($item in @(Get-ChildItem -LiteralPath $desktopPath -File -Filter '*.dktpack' -ErrorAction SilentlyContinue)) {
  $fpr15ExistingPackages[$item.FullName] = $true
}
$fpr15Export = Find-E1NamedElement -Name 'Экспорт шаблонов'
if ($null -eq $fpr15Export) {
  $management = Wait-UiElement -Description 'FPR-15 template management' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Управление кнопками'
  }
  Invoke-UiElementPhysically -Element $management -Description 'open FPR-15 template management'
  $fpr15Export = Wait-UiElement -Description 'FPR-15 export templates button' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Экспорт шаблонов'
  }
}
Invoke-UiElementPhysically -Element $fpr15Export -Description 'FPR-15 export templates'

$fpr15Package = $null
$fpr15ExportDeadline = [DateTime]::UtcNow.AddSeconds(30)
do {
  $fpr15Package = @(Get-ChildItem -LiteralPath $desktopPath -File -Filter '*.dktpack' -ErrorAction SilentlyContinue |
    Where-Object { -not $fpr15ExistingPackages.ContainsKey($_.FullName) } |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1)
  if ($fpr15Package.Count -gt 0) { $fpr15Package = $fpr15Package[0]; break }
  Start-Sleep -Milliseconds 200
} while ([DateTime]::UtcNow -lt $fpr15ExportDeadline)
if ($null -eq $fpr15Package) { throw 'FPR-15 installed export did not publish a .dktpack file.' }

$fpr15Zip = [System.IO.Compression.ZipFile]::OpenRead($fpr15Package.FullName)
try {
  $manifestEntry = $fpr15Zip.GetEntry('manifest.json')
  if ($null -eq $manifestEntry) { throw 'FPR-15 package has no manifest.json.' }
  $manifestReader = [System.IO.StreamReader]::new($manifestEntry.Open(), [System.Text.Encoding]::UTF8)
  try { $fpr15ManifestText = $manifestReader.ReadToEnd() } finally { $manifestReader.Dispose() }
  foreach ($forbidden in @('semantic_case', 'source_path', 'template_path', $appDataRoot, $fixtureDir, $fpr15OldCaseSentinel, 'fpr06-scanner-source.docx')) {
    if (-not [string]::IsNullOrWhiteSpace($forbidden) -and $fpr15ManifestText.Contains($forbidden)) {
      throw "FPR-15 manifest leaked forbidden case/local value: $forbidden"
    }
  }
  foreach ($entry in $fpr15Zip.Entries) {
    if ($entry.Length -gt 64MB) { throw "FPR-15 package entry is unexpectedly large: $($entry.FullName)" }
    $stream = $entry.Open()
    try {
      $memory = New-Object System.IO.MemoryStream
      try {
        $stream.CopyTo($memory)
        $entryBytes = $memory.ToArray()
      } finally { $memory.Dispose() }
    } finally { $stream.Dispose() }
    $entryText = [System.Text.Encoding]::UTF8.GetString($entryBytes)
    if ($entryText.Contains($fpr15OldCaseSentinel)) {
      throw "FPR-15 package leaked the prior case sentinel through $($entry.FullName)."
    }
  }
} finally {
  $fpr15Zip.Dispose()
}
Write-Host "FPR-15 EXPORT PRIVACY PASS: $($fpr15Package.FullName)"

Stop-Process -Id $process.Id -Force
$process.WaitForExit()
Remove-Item -LiteralPath $appDataRoot -Recurse -Force -ErrorAction Stop
if (Test-Path -LiteralPath $appDataRoot) { throw 'FPR-15 clean-profile reset left application data behind.' }

$process = Start-Process -FilePath $app.FullName -PassThru
$window = Wait-UiElement -Description 'FPR-15 clean-profile application window' -TimeoutSeconds 40 -Probe {
  Find-LiveAppWindow
}
if ($null -ne (Find-E1NamedElement -Name $fpr06Label)) {
  throw 'FPR-15 clean profile already contains the transferred button before import.'
}
Write-Host 'FPR-15 CLEAN PROFILE PASS: application data removed and transferred button is absent before import.'

$fpr15Import = Find-E1NamedElement -Name 'Импорт шаблонов'
if ($null -eq $fpr15Import) {
  $management = Wait-UiElement -Description 'FPR-15 clean-profile template management' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Управление кнопками'
  }
  Invoke-UiElementPhysically -Element $management -Description 'open FPR-15 clean-profile template management'
  $fpr15Import = Wait-UiElement -Description 'FPR-15 import templates button' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Импорт шаблонов'
  }
}
$importDialog = Invoke-UiActionWithObservedTransition `
  -Description 'FPR-15 import templates' `
  -TransitionDescription 'FPR-15 native transfer package picker' `
  -ActionProbe {
    $window = Find-LiveAppWindow
    if ($null -eq $window) { return $null }
    Find-ReadyButtonByNames -Root $window -Names @('Импорт шаблонов')
  } `
  -TransitionProbe { Find-FileDialog }
Set-OpenFileDialogPath -Dialog $importDialog -Path $fpr15Package.FullName | Out-Null
Submit-OpenFileDialog -Dialog $importDialog
$null = Wait-UiElement -Description 'FPR-15 imported Scanner button' -TimeoutSeconds 40 -Probe {
  Find-E1NamedElement -Name $fpr06Label
}
if (-not (Test-Path -LiteralPath $appDataRoot -PathType Container)) {
  throw 'FPR-15 import did not recreate clean profile application data.'
}
Write-Host 'FPR-15 IMPORT PASS: .dktpack imported through installed UI into a genuinely clean profile.'

$fpr15NewSentinel = 'FPR15-NEW-CLEAN-PROFILE'
$fpr15Source = Join-Path $fixtureDir 'fpr15-new-case.docx'
New-E1TextDocx -Path $fpr15Source -Lines @(
  'FPR-15 clean-profile new case',
  'Номер документа: FPR15-NEW-1',
  'Дата документа: 27.09.2026',
  "Новое значение: $fpr15NewSentinel"
)
Set-E1DomainSource -SourcePath $fpr15Source

$fpr15FieldInput = Find-E1NamedElement -Name 'Идентификатор поля'
if ($null -eq $fpr15FieldInput) {
  $fpr15Advanced = Wait-UiElement -Description 'FPR-15 advanced tools toggle' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Расширенные инструменты'
  }
  Invoke-UiElementPhysically -Element $fpr15Advanced -Description 'open FPR-15 manual Scanner tools'
  $fpr15FieldInput = Wait-UiElement -Description 'FPR-15 Scanner field input' -TimeoutSeconds 20 -Probe {
    Find-E1NamedElement -Name 'Идентификатор поля'
  }
}
Set-UiValue -Element $fpr15FieldInput -Value $fpr06FieldId
$fpr15TextInput = Wait-UiElement -Description 'FPR-15 Scanner text input' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElement -Name 'Выделенный текст'
}
Set-UiValue -Element $fpr15TextInput -Value $fpr15NewSentinel
Invoke-UiActionPhysicallyFromProbe -Description 'apply FPR-15 new Scanner value' -ActionProbe {
  Find-E1NamedElement -Name 'Назначить выделение полю'
}
$null = Wait-UiElement -Description 'FPR-15 Scanner accepted status' -TimeoutSeconds 20 -Probe {
  Find-E1NamedElementContaining -Text 'Разметка сохранена: принято 1'
}

Invoke-E1DomainScenario `
  -Label $fpr06Label `
  -PromptValues @{} `
  -PluginRequiredFields @() `
  -ExpectedOutputValues @($fpr15NewSentinel) `
  -OutputRoot $defaultOutputRoot `
  -ReceiptRoot $completionReceiptRoot `
  -ExpectPreflight $false

$fpr15Doc = Get-ChildItem -LiteralPath $defaultOutputRoot -Recurse -File -Filter "$fpr06Label.docx" -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTimeUtc -Descending |
  Select-Object -First 1
if ($null -eq $fpr15Doc) { throw 'FPR-15 clean-profile import did not publish a physical DOCX.' }
$fpr15Archive = [System.IO.Compression.ZipFile]::OpenRead($fpr15Doc.FullName)
try {
  $fpr15Entry = $fpr15Archive.GetEntry('word/document.xml')
  if ($null -eq $fpr15Entry) { throw 'FPR-15 output is not a readable DOCX package.' }
  $fpr15Reader = [System.IO.StreamReader]::new($fpr15Entry.Open(), [System.Text.Encoding]::UTF8)
  try { $fpr15Xml = $fpr15Reader.ReadToEnd() } finally { $fpr15Reader.Dispose() }
} finally { $fpr15Archive.Dispose() }
if ($fpr15Xml -notmatch [regex]::Escape($fpr15NewSentinel)) {
  throw 'FPR-15 imported button did not render the new clean-profile case value.'
}
if ($fpr15Xml -match [regex]::Escape($fpr15OldCaseSentinel)) {
  throw 'FPR-15 imported button leaked the pre-export case value into the clean-profile output.'
}
Write-Host "FPR-15 INSTALLED PASS: export -> privacy read-back -> clean profile -> import -> new case -> physical DOCX -> committed receipt: $($fpr15Doc.FullName)"

Stop-Process -Id $process.Id -Force
$process.WaitForExit()
$uninstaller = Get-ChildItem -Path $installDir -Recurse -File -Filter '*.exe' | Where-Object { $_.Name -match 'uninstall' } | Select-Object -First 1
if (!$uninstaller) { throw 'E1 NSIS uninstaller missing.' }
$uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "E1 NSIS uninstall failed with exit code $($uninstall.ExitCode)" }
