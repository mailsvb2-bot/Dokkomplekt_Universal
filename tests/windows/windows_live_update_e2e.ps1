[CmdletBinding()]
param(
    [string] $PreviousInstaller = $env:DOKKOMPLEKT_PREVIOUS_SIGNED_INSTALLER,
    [ValidateSet('10', '11')] [string] $ExpectedWindowsVersion = '11',
    [string] $OutputPath = 'verification/release/FPR19_LIVE_UPDATE.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($PreviousInstaller)) {
    $hardwareConfig = Join-Path $env:LOCALAPPDATA 'DokkomplektHardwareRunner\hardware-config.cmd'
    if (Test-Path -LiteralPath $hardwareConfig -PathType Leaf) {
        foreach ($line in Get-Content -LiteralPath $hardwareConfig) {
            if ($line -match '^\s*set\s+"?DOKKOMPLEKT_PREVIOUS_SIGNED_INSTALLER=(.*?)"?\s*$') {
                $PreviousInstaller = $matches[1].Replace('%%', '%')
                break
            }
        }
    }
}

if ($env:DOKKOMPLEKT_RUN_HARDWARE_E2E -ne '1') {
    throw 'FPR-19 live update may run only on the dedicated hardware runner.'
}
if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'FPR-19 live update requires Windows x64.'
}

$os = Get-CimInstance Win32_OperatingSystem
if ([string]$os.Caption -notmatch 'Windows 11') {
    throw "FPR-19 live update requires Windows 11; detected $($os.Caption)."
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sessionId = (Get-Process -Id $PID).SessionId
if ($identity.IsSystem -or $sessionId -eq 0) {
    throw 'FPR-19 live update requires an interactive user session.'
}

if ([string]::IsNullOrWhiteSpace($PreviousInstaller) -or -not (Test-Path -LiteralPath $PreviousInstaller -PathType Leaf)) {
    throw 'DOKKOMPLEKT_PREVIOUS_SIGNED_INSTALLER must point to a previous production-signed NSIS installer.'
}
$PreviousInstaller = (Resolve-Path -LiteralPath $PreviousInstaller).Path
$previousSignature = Get-AuthenticodeSignature -FilePath $PreviousInstaller
if ($previousSignature.Status -ne 'Valid') {
    throw "Previous installer Authenticode is not valid: $($previousSignature.Status)"
}

$config = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$targetVersion = [version]([string]$config.version)
$identifier = [string]$config.identifier
$installDir = Join-Path $env:RUNNER_TEMP ("dokkomplekt-fpr19-" + [Guid]::NewGuid().ToString('N'))
$process = $null

function Stop-LiveApp {
    if ($null -eq $script:process) { return }
    try {
        $script:process.Refresh()
        if (-not $script:process.HasExited) {
            Stop-Process -Id $script:process.Id -Force -ErrorAction SilentlyContinue
            try { $script:process.WaitForExit(10000) | Out-Null } catch { }
        }
    } catch { }
}

trap {
    $failure = $_
    Stop-LiveApp
    Write-Error -ErrorRecord $failure
    exit 1
}

$install = Start-Process -FilePath $PreviousInstaller -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
if ($install.ExitCode -ne 0) {
    throw "Previous signed installer failed with exit code $($install.ExitCode)."
}

function Get-InstalledApp {
    $apps = @(Get-ChildItem -LiteralPath $installDir -Recurse -File -Filter '*.exe' | Where-Object {
        $_.Name -notmatch 'uninstall' -and (
            $_.Name -in @('Dokkomplekt Universal.exe', 'dokkomplekt-tauri.exe', 'Dokkomplekt.exe') -or
            $_.VersionInfo.OriginalFilename -eq 'dokkomplekt-tauri.exe'
        )
    })
    if ($apps.Count -ne 1) {
        throw "Expected exactly one installed application, found $($apps.Count)."
    }
    return $apps[0]
}

$app = Get-InstalledApp
if ((Get-AuthenticodeSignature -FilePath $app.FullName).Status -ne 'Valid') {
    throw 'Previous installed application is not validly signed.'
}
$previousVersionRaw = [string]$app.VersionInfo.ProductVersion
$previousVersionMatch = [regex]::Match($previousVersionRaw, '\d+\.\d+\.\d+')
if (-not $previousVersionMatch.Success) {
    throw "Cannot determine previous application version from '$previousVersionRaw'."
}
$previousVersion = [version]$previousVersionMatch.Value
if ($previousVersion -ge $targetVersion) {
    throw "Previous version $previousVersion must be lower than target $targetVersion."
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
$desktop = [System.Windows.Automation.AutomationElement]::RootElement
$process = Start-Process -FilePath $app.FullName -PassThru

function Find-AppWindow {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
        [int]$script:process.Id
    )
    return $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
}

function Wait-LiveElement {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Probe,
        [Parameter(Mandatory = $true)][string]$Description,
        [int]$TimeoutSeconds = 45
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $value = & $Probe
        if ($null -ne $value) { return $value }
        if ($script:process.HasExited) {
            throw "Application exited while waiting for $Description."
        }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "FPR-19 UI timeout: $Description"
}

function Find-Button {
    param(
        [Parameter(Mandatory = $true)]$Root,
        [Parameter(Mandatory = $true)][string]$Name
    )
    $condition = [System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Button
        ),
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::NameProperty,
            $Name
        )
    )
    $button = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -ne $button -and $button.Current.IsEnabled) { return $button }
    return $null
}

function Invoke-Button {
    param([Parameter(Mandatory = $true)]$Button)
    if ($Button.Current.IsInvokePatternAvailable) {
        $Button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        return
    }
    if ($Button.Current.IsLegacyIAccessiblePatternAvailable) {
        $Button.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern).DoDefaultAction()
        return
    }
    $Button.SetFocus()
    Start-Sleep -Milliseconds 100
    [System.Windows.Forms.SendKeys]::SendWait(' ')
}

function Find-StatusContaining {
    param([Parameter(Mandatory = $true)][string]$Text)
    $window = Find-AppWindow
    if ($null -eq $window) { return $null }
    foreach ($element in $window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
    )) {
        try {
            if (([string]$element.Current.Name).Contains($Text)) { return $element }
        } catch { }
    }
    return $null
}

$null = Wait-LiveElement -Description 'previous application window' -Probe { Find-AppWindow }
$settings = Wait-LiveElement -Description 'Settings button' -Probe {
    $window = Find-AppWindow
    if ($null -ne $window) { Find-Button -Root $window -Name 'Настройки' }
}
Invoke-Button -Button $settings

$check = Wait-LiveElement -Description 'Check updates button' -Probe {
    $window = Find-AppWindow
    if ($null -ne $window) { Find-Button -Root $window -Name 'Проверить обновления' }
}
Invoke-Button -Button $check

$null = Wait-LiveElement -Description "available target version $targetVersion" -TimeoutSeconds 90 -Probe {
    Find-StatusContaining -Text "Доступна версия $targetVersion"
}

$apply = Wait-LiveElement -Description 'Install and restart update confirmation' -TimeoutSeconds 30 -Probe {
    $window = Find-AppWindow
    if ($null -ne $window) { Find-Button -Root $window -Name 'Установить и перезапустить' }
}
Invoke-Button -Button $apply

if (-not $process.WaitForExit(60000)) {
    throw 'Previous application did not exit after apply_verified_update.'
}

$deadline = [DateTime]::UtcNow.AddMinutes(3)
$updated = $false
do {
    Start-Sleep -Seconds 2
    $app = Get-InstalledApp
    $raw = [string]$app.VersionInfo.ProductVersion
    $match = [regex]::Match($raw, '\d+\.\d+\.\d+')
    if ($match.Success -and [version]$match.Value -ge $targetVersion) {
        $updated = $true
        break
    }
} while ([DateTime]::UtcNow -lt $deadline)
if (-not $updated) {
    throw "Installed application did not advance to target $targetVersion."
}
if ((Get-AuthenticodeSignature -FilePath $app.FullName).Status -ne 'Valid') {
    throw 'Updated installed application is not validly signed.'
}

$process = Start-Process -FilePath $app.FullName -PassThru
$null = Wait-LiveElement -Description 'updated application window' -Probe { Find-AppWindow }
$null = Wait-LiveElement -Description 'verified update recovery status' -TimeoutSeconds 45 -Probe {
    Find-StatusContaining -Text "Обновление до версии $targetVersion установлено"
}

$appData = Join-Path ([Environment]::GetFolderPath('ApplicationData')) $identifier
$recoveryPath = Join-Path $appData 'update-recovery.json'
if (-not (Test-Path -LiteralPath $recoveryPath -PathType Leaf)) {
    throw 'FPR-19 update recovery marker is missing after updated restart.'
}
$recovery = Get-Content -LiteralPath $recoveryPath -Raw | ConvertFrom-Json
if ($recovery.status -ne 'verified') {
    throw "FPR-19 recovery marker is not verified: $($recovery.status)"
}
if ([version]$recovery.from_version -ne $previousVersion) {
    throw "FPR-19 recovery from_version mismatch: $($recovery.from_version) vs $previousVersion"
}
if ([version]$recovery.target_version -ne $targetVersion) {
    throw "FPR-19 recovery target_version mismatch: $($recovery.target_version) vs $targetVersion"
}
if (-not (Test-Path -LiteralPath $recovery.backup_dir -PathType Container)) {
    throw 'FPR-19 verified recovery marker references a missing backup directory.'
}

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
[ordered]@{
    schema = 'dokkomplekt.fpr19-live-update.v1'
    completed_at_utc = [DateTime]::UtcNow.ToString('o')
    previous_version = $previousVersion.ToString()
    target_version = $targetVersion.ToString()
    previous_installer_sha256 = (Get-FileHash -LiteralPath $PreviousInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
    updated_application_sha256 = (Get-FileHash -LiteralPath $app.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    recovery_status = [string]$recovery.status
    backup_dir = [string]$recovery.backup_dir
    windows = [string]$os.Caption
    session_id = $sessionId
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutputPath -Encoding utf8

Write-Host 'FPR-19 LIVE UPDATE PASS: signed previous version -> real Update UI -> verified NSIS apply -> target version -> verified recovery marker.'

Stop-LiveApp
$uninstaller = @(Get-ChildItem -LiteralPath $installDir -Recurse -File -Filter '*.exe' |
    Where-Object { $_.Name -match 'uninstall' } |
    Select-Object -First 1)
if ($uninstaller.Count -eq 1) {
    $uninstall = Start-Process -FilePath $uninstaller[0].FullName -ArgumentList '/S' -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) {
        throw "FPR-19 cleanup uninstall failed: $($uninstall.ExitCode)"
    }
}
