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

  if ($Element.Current.IsOffscreen -and $Element.Current.IsScrollItemPatternAvailable) {
    $Element.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
    Start-Sleep -Milliseconds 100
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
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$clickPoint.X, [int]$clickPoint.Y)
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
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$point.X, [int]$point.Y)
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

  $comboName = "Профиль для $FileName"
  $combo = Find-E1NamedElement -Name $comboName
  if ($null -eq $combo) {
    try {
      $combo = Invoke-UiActionWithObservedTransition `
        -Description "open advanced template settings for $FileName" `
        -TransitionDescription "domain selector for $FileName" `
        -TransitionSeconds 5 `
        -ActionProbe {
          Find-E1NamedElement -Name 'Необязательно: настроить автоматическое заполнение'
        } `
        -TransitionProbe {
          Find-E1NamedElement -Name $comboName
        }
    } catch {
      Write-Host ("E1 UI snapshot after failed advanced settings transition for '$FileName': " + (Get-E1UiSnapshot))
      throw
    }
  }

  if ($combo.Current.IsOffscreen -and $combo.Current.IsScrollItemPatternAvailable) {
    $combo.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView()
    Start-Sleep -Milliseconds 100
  }

  # Chromium/WebView2 does not consistently publish <option> descendants for an
  # expanded HTML <select> on hosted Windows runners. Prefer semantic UIA when
  # available, but never treat input delivery itself as proof: the live selected
  # option is read back before this helper returns.
  $option = $null
  try {
    if ($combo.Current.IsExpandCollapsePatternAvailable) {
      $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    } else {
      Invoke-UiElement -Element $combo -Description "open domain selector for $FileName"
    }
    Start-Sleep -Milliseconds 150
    $optionDeadline = [DateTime]::UtcNow.AddSeconds(2)
    do {
      $window = Find-LiveAppWindow
      if ($null -ne $window) {
        $option = $window.FindFirst(
          [System.Windows.Automation.TreeScope]::Descendants,
          [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::NameProperty,
            $OptionName
          )
        )
      }
      if ($null -ne $option) { break }
      Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $optionDeadline)
  } catch {
    $option = $null
  }

  if ($null -ne $option) {
    if ($option.Current.IsSelectionItemPatternAvailable) {
      $option.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    } else {
      Invoke-UiElement -Element $option -Description "select domain '$OptionName' for $FileName"
    }
    Start-Sleep -Milliseconds 200
  }

  $actualDomain = Normalize-UiValue -Value (Get-E1DomainSelection -FileName $FileName)
  $expectedDomain = Normalize-UiValue -Value $OptionName
  if ($actualDomain -ne $expectedDomain) {
    $domainOffsets = @{
      'Универсальный документооборот' = 1
      'Медицина' = 2
      'Юридическая работа' = 3
      'Кадровая работа' = 4
      'Бухгалтерия' = 5
      'Образование' = 6
      'Своя профессия / профиль' = 7
    }
    if (-not $domainOffsets.ContainsKey($OptionName)) {
      throw "Unsupported E1 domain option for keyboard fallback: $OptionName"
    }

    $combo = Wait-UiElement -Description "domain selector before keyboard fallback for $FileName" -Probe {
      Find-E1NamedElement -Name $comboName
    }
    if ($combo.Current.IsExpandCollapsePatternAvailable) {
      try {
        $expandCollapse = $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        if ($expandCollapse.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) {
          $expandCollapse.Collapse()
          Start-Sleep -Milliseconds 100
        }
      } catch { }
    }

    $combo = Wait-UiElement -Description "live domain selector for keyboard fallback for $FileName" -Probe {
      Find-E1NamedElement -Name $comboName
    }

    # Exercise the real controlled <select> instead of relying on UIA focus
    # routing, which is flaky for WebView2 on hosted runners. A physical click
    # gives Chromium foreground ownership; HOME/DOWN/ENTER then changes the live
    # selection and dispatches React's change event.
    Invoke-UiElementPhysically -Element $combo -Description "focus domain selector for $FileName"
    Start-Sleep -Milliseconds 100
    [System.Windows.Forms.SendKeys]::SendWait('{HOME}')
    Start-Sleep -Milliseconds 100
    for ($index = 0; $index -lt [int]$domainOffsets[$OptionName]; $index += 1) {
      [System.Windows.Forms.SendKeys]::SendWait('{DOWN}')
      Start-Sleep -Milliseconds 50
    }
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Start-Sleep -Milliseconds 300

    $actualDomain = Normalize-UiValue -Value (Get-E1DomainSelection -FileName $FileName)
    Write-Host "E1 domain selector keyboard fallback: expected='$OptionName' actual='$actualDomain'."
  }

  if (-not [string]::IsNullOrWhiteSpace($actualDomain) -and $actualDomain -ne $expectedDomain) {
    throw "E1 domain override did not persist for $FileName. Expected '$OptionName', actual '$actualDomain'."
  }
  if ([string]::IsNullOrWhiteSpace($actualDomain)) {
    # Hosted WebView2 can make the controlled <select> completely unreadable to
    # UIA even after a real user-equivalent change. Do not treat that as success:
    # defer proof to the mandatory domain-specific preflight/output checks that
    # immediately follow template publication. A wrong domain will fail there
    # because the expected plugin-required fields and physical result are absent.
    Write-Host "E1 domain UIA read-back unavailable for $FileName; deferring '$OptionName' verification to domain-specific installed behavior."
  }

  if (-not [string]::IsNullOrWhiteSpace($CustomProfile)) {
    $customName = "Своя профессия / профиль для $FileName"
    $custom = Wait-UiElement -Description "custom domain value for $FileName" -Probe {
      Find-E1NamedElement -Name $customName
    }
    Set-UiValue -Element $custom -Value $CustomProfile

    # React controls this input. UIA ValuePattern.SetValue can update the DOM
    # without dispatching React's input/onChange event, so force one
    # user-equivalent no-op edit and then blur. This commits exactly the
    # already supplied Unicode value without depending on the runner keyboard
    # layout for Cyrillic text.
    $custom.SetFocus()
    Start-Sleep -Milliseconds 50
    [System.Windows.Forms.SendKeys]::SendWait(' ')
    [System.Windows.Forms.SendKeys]::SendWait('{BACKSPACE}')
    [System.Windows.Forms.SendKeys]::SendWait('{TAB}')
    Start-Sleep -Milliseconds 300

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
  Set-UiValue -Element $labelInput -Value $Label
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
    throw
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
    Find-ReadyButtonByNames -Root $window -Names @('Проверить и создать (1)', 'Создать документы (1)')
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
      Set-UiValue -Element $control -Value ([string]$PromptValues[$fieldId])
    }

    Invoke-UiActionPhysicallyFromProbe -Description "create $Label" -ActionProbe {
      $window = Find-LiveAppWindow
      if ($null -eq $window) { return $null }
      Find-ReadyButtonByNames -Root $window -Names @('Создать документы')
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
      if ($null -ne $failure) { throw "E1 $Label backend rejected the completed preflight." }
      Start-Sleep -Milliseconds 250
    }
  }
  if ($null -eq $created) { throw "E1 $Label did not publish a physical DOCX." }

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
Set-E1TemplateDomainOverride -FileName 'service_act.docx' -OptionName 'Бухгалтерия'
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
do {
  if (Test-Path -LiteralPath $completionReceiptRoot -PathType Container) {
    foreach ($file in @(Get-ChildItem -LiteralPath $completionReceiptRoot -File -Filter '*.json' -ErrorAction SilentlyContinue)) {
      try {
        $data = (Get-Content -LiteralPath $file.FullName -Raw) | ConvertFrom-Json
        $receiptHash = [string]$data.output_sha256
        if ($data.status -eq 'committed' -and $fpr01OutputHashes.Contains($receiptHash)) {
          $null = $fpr01MatchedHashes.Add($receiptHash)
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

Stop-Process -Id $process.Id -Force
$process.WaitForExit()
$uninstaller = Get-ChildItem -Path $installDir -Recurse -File -Filter '*.exe' | Where-Object { $_.Name -match 'uninstall' } | Select-Object -First 1
if (!$uninstaller) { throw 'E1 NSIS uninstaller missing.' }
$uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "E1 NSIS uninstall failed with exit code $($uninstall.ExitCode)" }
