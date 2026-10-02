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
$buildNumber = [int]$os.BuildNumber
$workstation = [int]$os.ProductType -eq 1
$expectedOs = if ($ExpectedWindowsVersion -eq '10') {
    $workstation -and $buildNumber -ge 10240 -and $buildNumber -lt 22000
} else {
    $workstation -and $buildNumber -ge 22000
}
if (-not $expectedOs) {
    throw "FPR-19 live update expected Windows $ExpectedWindowsVersion but detected caption=$($os.Caption) build=$buildNumber."
}
if ([string]$os.OSArchitecture -notmatch '64') {
    throw "FPR-19 live update requires a 64-bit OS; detected: $($os.OSArchitecture)"
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
$appData = Join-Path ([Environment]::GetFolderPath('ApplicationData')) $identifier
$stateDatabase = Join-Path $appData 'dokkomplekt-user-state.sqlite'
$stateKey = "$stateDatabase.key"
$recoveryPath = Join-Path $appData 'update-recovery.json'
$outputPreferenceStateKey = 'output_preferences_v2'
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


function Start-LiveApp {
    $script:process = Start-Process -FilePath $app.FullName -PassThru
    $null = Wait-LiveElement -Description 'application window after update recovery' -Probe { Find-AppWindow }
}

function Open-LiveSettings {
    $settings = Wait-LiveElement -Description 'Settings button' -Probe {
        $window = Find-AppWindow
        if ($null -ne $window) { Find-Button -Root $window -Name 'Настройки' }
    }
    Invoke-Button -Button $settings
}

function Get-OutputPreferencesFingerprint {
    $raw = & python scripts/read_app_state_fingerprint.py --database $stateDatabase --state-key $outputPreferenceStateKey --wait-seconds 30
    if ($LASTEXITCODE -ne 0) {
        throw 'FPR-12 output_preferences_v2 row did not become readable.'
    }
    $fingerprint = ([string]$raw).Trim().ToLowerInvariant()
    if ($fingerprint -notmatch '^[0-9a-f]{64}$') {
        throw "FPR-12 state fingerprint is invalid: $fingerprint"
    }
    return $fingerprint
}

function Wait-RecoveryStatus {
    param(
        [Parameter(Mandatory = $true)][string]$ExpectedStatus,
        [int]$TimeoutSeconds = 45
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if (Test-Path -LiteralPath $recoveryPath -PathType Leaf) {
            try {
                $candidate = Get-Content -LiteralPath $recoveryPath -Raw | ConvertFrom-Json
                if ([string]$candidate.status -eq $ExpectedStatus) {
                    return $candidate
                }
            } catch { }
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    $actual = if (Test-Path -LiteralPath $recoveryPath -PathType Leaf) {
        try { [string](Get-Content -LiteralPath $recoveryPath -Raw | ConvertFrom-Json).status } catch { '<unreadable>' }
    } else {
        '<missing>'
    }
    throw "Timed out waiting for update recovery status '$ExpectedStatus'; actual=$actual"
}

function Wait-FailedInstallerSettled {
    param(
        [Parameter(Mandatory = $true)][string]$PackagePath,
        [int]$TimeoutSeconds = 60
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $quietPasses = 0
    do {
        $activeInstallers = @(Get-CimInstance Win32_Process | Where-Object {
            $exePath = [string]$_.ExecutablePath
            $commandLine = [string]$_.CommandLine
            $samePackage = -not [string]::IsNullOrWhiteSpace($exePath) -and
                $exePath.Equals($PackagePath, [StringComparison]::OrdinalIgnoreCase)
            $sameInstall = -not [string]::IsNullOrWhiteSpace($commandLine) -and
                $commandLine.IndexOf($installDir, [StringComparison]::OrdinalIgnoreCase) -ge 0 -and
                [int]$_.ProcessId -ne $PID
            $samePackage -or $sameInstall
        })
        if ($activeInstallers.Count -eq 0) {
            $quietPasses += 1
            if ($quietPasses -ge 4) { return }
        } else {
            $quietPasses = 0
        }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Forced installer failure did not settle while the installed executable was locked.'
}

function Invoke-UpdateAndExit {
    param([switch]$HoldInstalledExecutable)

    Open-LiveSettings
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

    $executableLock = $null
    try {
        if ($HoldInstalledExecutable) {
            $executableLock = [System.IO.File]::Open(
                $app.FullName,
                [System.IO.FileMode]::Open,
                [System.IO.FileAccess]::Read,
                [System.IO.FileShare]::Read
            )
        }
        Invoke-Button -Button $apply
        if (-not $process.WaitForExit(60000)) {
            throw 'Application did not exit after apply_verified_update.'
        }
        if ($HoldInstalledExecutable) {
            $started = Wait-RecoveryStatus -ExpectedStatus 'installer_started' -TimeoutSeconds 30
            Start-Sleep -Seconds 2
            Wait-FailedInstallerSettled -PackagePath ([string]$started.package_path)
            $stillInstalled = Get-InstalledApp
            $raw = [string]$stillInstalled.VersionInfo.ProductVersion
            $match = [regex]::Match($raw, '\d+\.\d+\.\d+')
            if (-not $match.Success -or [version]$match.Value -ne $previousVersion) {
                throw "Forced installer failure unexpectedly changed installed version to '$raw'."
            }
            return $started
        }
        return $null
    } finally {
        if ($null -ne $executableLock) {
            $executableLock.Dispose()
        }
    }
}

$null = Wait-LiveElement -Description 'previous application window' -Probe { Find-AppWindow }
Open-LiveSettings
$previousOutputPreferencesFingerprint = Get-OutputPreferencesFingerprint

# Failure proof 1: the real signed installer is started while the installed
# executable is held open without write/delete sharing. The previous version
# must remain installed and the next startup must preserve valid live state
# without restoring the pre-update snapshot.
$null = Invoke-UpdateAndExit -HoldInstalledExecutable
Start-LiveApp
$recoverable = Wait-RecoveryStatus -ExpectedStatus 'recoverable_failure'
$recoverableFingerprint = Get-OutputPreferencesFingerprint
if ($recoverableFingerprint -ne $previousOutputPreferencesFingerprint) {
    throw "FPR-19 recoverable installer failure mutated durable state: before=$previousOutputPreferencesFingerprint after=$recoverableFingerprint"
}
if ([string]$recoverable.last_error -notmatch 'сохранены без отката') {
    throw "FPR-19 recoverable failure did not prove no-rollback preservation: $($recoverable.last_error)"
}
Write-Host 'FPR-19 LIVE RECOVERABLE FAILURE PASS: real signed installer failed while executable was locked; valid live DB/key were preserved without rollback.'
Stop-LiveApp

# Failure proof 2: create a fresh consistent pre-update snapshot with another
# real installer failure, then damage the live encrypted DB/key while the app is
# stopped. Startup reconciliation must restore the snapshot before workspace load.
Start-LiveApp
$rollbackBeforeFingerprint = Get-OutputPreferencesFingerprint
$startedForRollback = Invoke-UpdateAndExit -HoldInstalledExecutable
$backupDir = [string]$startedForRollback.backup_dir
if (-not (Test-Path -LiteralPath $backupDir -PathType Container)) {
    throw "FPR-19 rollback proof has no backup directory: $backupDir"
}
$backupDatabase = Join-Path $backupDir ([IO.Path]::GetFileName($stateDatabase))
$backupKey = Join-Path $backupDir ([IO.Path]::GetFileName($stateKey))
if (-not (Test-Path -LiteralPath $backupDatabase -PathType Leaf) -or
    -not (Test-Path -LiteralPath $backupKey -PathType Leaf)) {
    throw 'FPR-19 rollback proof is missing the required SQLite snapshot or local-key backup.'
}
[IO.File]::WriteAllBytes(
    $stateDatabase,
    [Text.Encoding]::UTF8.GetBytes('FPR19-CONTROLLED-CORRUPT-DATABASE')
)
[IO.File]::WriteAllBytes(
    $stateKey,
    [Text.Encoding]::UTF8.GetBytes('FPR19-CONTROLLED-CORRUPT-KEY')
)
Remove-Item -LiteralPath "$stateDatabase-wal","$stateDatabase-shm" -Force -ErrorAction SilentlyContinue

Start-LiveApp
$rolledBack = Wait-RecoveryStatus -ExpectedStatus 'rolled_back'
$rolledBackFingerprint = Get-OutputPreferencesFingerprint
if ($rolledBackFingerprint -ne $rollbackBeforeFingerprint) {
    throw "FPR-19 rollback did not restore exact durable state: before=$rollbackBeforeFingerprint after=$rolledBackFingerprint"
}
if ([string]$rolledBack.last_error -notmatch 'автоматически восстановлено') {
    throw "FPR-19 rollback marker does not describe automatic restoration: $($rolledBack.last_error)"
}
Write-Host 'FPR-19 LIVE ROLLBACK PASS: controlled damaged live DB/key were restored from the consistent pre-update snapshot before workspace load.'
Stop-LiveApp

# Positive path: only after both recovery branches pass, allow the same previous
# signed build to update normally through the real application UI.
Start-LiveApp
$preSuccessFingerprint = Get-OutputPreferencesFingerprint
if ($preSuccessFingerprint -ne $previousOutputPreferencesFingerprint) {
    throw "FPR-19 recovery exercises changed durable output preferences before success: baseline=$previousOutputPreferencesFingerprint current=$preSuccessFingerprint"
}
$null = Invoke-UpdateAndExit

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

$updatedOutputPreferencesRaw = & python scripts/read_app_state_fingerprint.py --database $stateDatabase --state-key $outputPreferenceStateKey --wait-seconds 30
if ($LASTEXITCODE -ne 0) {
    throw 'FPR-12 updated output_preferences_v2 row did not remain readable after installer apply.'
}
$updatedOutputPreferencesFingerprint = ([string]$updatedOutputPreferencesRaw).Trim().ToLowerInvariant()
if ($updatedOutputPreferencesFingerprint -notmatch '^[0-9a-f]{64}$') {
    throw "FPR-12 updated state fingerprint is invalid: $updatedOutputPreferencesFingerprint"
}
if ($updatedOutputPreferencesFingerprint -ne $previousOutputPreferencesFingerprint) {
    throw "FPR-12 cross-version installer mutated durable output preferences: before=$previousOutputPreferencesFingerprint after=$updatedOutputPreferencesFingerprint"
}
Write-Host 'FPR-12 LIVE UPGRADE STORAGE PASS: exact output_preferences_v2 row survived signed previous-version -> current-version installer apply.'

$process = Start-Process -FilePath $app.FullName -PassThru
$null = Wait-LiveElement -Description 'updated application window' -Probe { Find-AppWindow }
$null = Wait-LiveElement -Description 'verified update recovery status' -TimeoutSeconds 45 -Probe {
    Find-StatusContaining -Text "Обновление до версии $targetVersion установлено"
}

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
    recoverable_failure_status = [string]$recoverable.status
    recoverable_failure_output_preferences_sha256 = $recoverableFingerprint
    rollback_status = [string]$rolledBack.status
    rollback_output_preferences_sha256 = $rolledBackFingerprint
    output_preferences_state_key = $outputPreferenceStateKey
    previous_output_preferences_sha256 = $previousOutputPreferencesFingerprint
    post_installer_output_preferences_sha256 = $updatedOutputPreferencesFingerprint
    expected_windows_version = $ExpectedWindowsVersion
    windows = [string]$os.Caption
    windows_build_number = $buildNumber
    os_architecture = [string]$os.OSArchitecture
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
