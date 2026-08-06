param(
    [Parameter(Mandatory = $true)] [string] $InstallerRoot
)
$ErrorActionPreference = 'Stop'
if ($env:DOKKOMPLEKT_RUN_HARDWARE_E2E -ne '1') {
    throw 'Hardware E2E is opt-in: set DOKKOMPLEKT_RUN_HARDWARE_E2E=1 on a dedicated runner.'
}
if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_TEST_PRINTER)) {
    throw 'DOKKOMPLEKT_TEST_PRINTER must name a dedicated test printer.'
}
New-Item -ItemType Directory -Force -Path '.release-gate' | Out-Null
$releaseGate = (Resolve-Path '.release-gate').Path
$rebootEvidencePath = $env:DOKKOMPLEKT_REBOOT_EVIDENCE_PATH
if ([string]::IsNullOrWhiteSpace($rebootEvidencePath) -and -not [string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_REBOOT_EVIDENCE)) {
    $rebootEvidencePath = $env:DOKKOMPLEKT_REBOOT_EVIDENCE
}

$word = Get-Command winword.exe -ErrorAction SilentlyContinue
if ($null -eq $word) {
    $word = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Winword.exe' -ErrorAction SilentlyContinue
}
if ($null -eq $word) { throw 'Microsoft Word is not installed on the hardware runner.' }
$printer = Get-Printer -Name $env:DOKKOMPLEKT_TEST_PRINTER -ErrorAction Stop
if ($null -eq $printer) { throw 'Dedicated test printer is unavailable.' }

cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
$printStartedUtc = [DateTime]::UtcNow
try {
    wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
} catch {
    throw "PrintService Operational log could not be enabled: $($_.Exception.Message)"
}
cargo test -p dokkomplekt-tauri windows_word_print_hardware_e2e --locked -- --ignored --nocapture
Start-Sleep -Seconds 5
$printEvents = @(Get-WinEvent -FilterHashtable @{
    LogName='Microsoft-Windows-PrintService/Operational';
    Id=307;
    StartTime=$printStartedUtc
} -ErrorAction Stop | Where-Object { $_.Message -match [Regex]::Escape($printer.Name) })
if ($printEvents.Count -eq 0) {
    throw "No completed PrintService event 307 was observed for printer '$($printer.Name)'. COM submission alone is not accepted."
}
$printEventEvidence = @($printEvents | ForEach-Object {
    [ordered]@{
        record_id = $_.RecordId
        event_id = $_.Id
        provider = $_.ProviderName
        machine = $_.MachineName
        created_at_utc = $_.TimeCreated.ToUniversalTime().ToString('o')
        printer = $printer.Name
    }
})
$printEventEvidence | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $releaseGate 'PRINT_EVENT_307.json') -Encoding utf8

$installers = @(Get-ChildItem -LiteralPath $InstallerRoot -Recurse -File -Filter '*.exe')
if ($installers.Count -eq 0) { throw "No NSIS installer found under $InstallerRoot" }
$installer = $installers | Sort-Object Length -Descending | Select-Object -First 1
$signature = Get-AuthenticodeSignature $installer.FullName
if ($signature.Status -ne 'Valid') { throw "Installer is not validly signed: $($signature.Status)" }

$installDir = Join-Path $env:RUNNER_TEMP ("dokkomplekt-hardware-install-" + [Guid]::NewGuid().ToString('N'))
Remove-Item -LiteralPath $installDir -Recurse -Force -ErrorAction SilentlyContinue
$installProcess = Start-Process -FilePath $installer.FullName -ArgumentList @('/S', "/D=$installDir") -Wait -PassThru
if ($installProcess.ExitCode -ne 0) { throw "Silent NSIS install failed with exit code $($installProcess.ExitCode)" }
if (-not (Test-Path -LiteralPath $installDir -PathType Container)) { throw 'Silent NSIS install did not create the requested install directory.' }
$appCandidates = @(Get-ChildItem -LiteralPath $installDir -Recurse -File -Filter '*.exe' | Where-Object {
    $_.Name -in @('Dokkomplekt Universal.exe', 'dokkomplekt-tauri.exe', 'Dokkomplekt.exe')
})
if ($appCandidates.Count -ne 1) {
    throw "Expected exactly one installed application executable, found $($appCandidates.Count)."
}
$app = $appCandidates[0].FullName
$installedSignature = Get-AuthenticodeSignature -FilePath $app
if ($installedSignature.Status -ne 'Valid') { throw "Installed application is not validly signed: $($installedSignature.Status)" }
[ordered]@{
    schema = 'dokkomplekt.authenticode-evidence.v1'
    installer = [ordered]@{
        name = $installer.Name
        sha256 = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        signer_thumbprint = $signature.SignerCertificate.Thumbprint
        signer_subject = $signature.SignerCertificate.Subject
        status = [string]$signature.Status
    }
    installed_application = [ordered]@{
        name = (Split-Path $app -Leaf)
        sha256 = (Get-FileHash $app -Algorithm SHA256).Hash.ToLowerInvariant()
        signer_thumbprint = $installedSignature.SignerCertificate.Thumbprint
        signer_subject = $installedSignature.SignerCertificate.Subject
        status = [string]$installedSignature.Status
    }
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $releaseGate 'AUTHENTICODE_SIGNATURES.json') -Encoding utf8

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public sealed class DokkomplektWindowEvidence
{
    public long Handle { get; set; }
    public uint ProcessId { get; set; }
    public string Title { get; set; } = string.Empty;
    public string ClassName { get; set; } = string.Empty;
}

public static class DokkomplektWindowProbe
{
    private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr hWnd, StringBuilder className, int maxCount);

    public static DokkomplektWindowEvidence[] GetVisibleWindows(uint processId)
    {
        var windows = new List<DokkomplektWindowEvidence>();
        EnumWindows((hWnd, _) =>
        {
            if (!IsWindowVisible(hWnd)) return true;
            GetWindowThreadProcessId(hWnd, out uint ownerProcessId);
            if (processId != 0 && ownerProcessId != processId) return true;
            var title = new StringBuilder(1024);
            var className = new StringBuilder(256);
            GetWindowText(hWnd, title, title.Capacity);
            GetClassName(hWnd, className, className.Capacity);
            windows.Add(new DokkomplektWindowEvidence
            {
                Handle = hWnd.ToInt64(),
                ProcessId = ownerProcessId,
                Title = title.ToString(),
                ClassName = className.ToString()
            });
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }
}
'@

function Get-NewVisibleConsoleWindows {
    param(
        [Parameter(Mandatory = $true)] [System.Collections.Generic.HashSet[long]] $BaselineHandles
    )
    $forbiddenProcessNames = @('cmd', 'powershell', 'pwsh', 'conhost', 'wscript', 'cscript')
    $unexpected = [System.Collections.Generic.List[object]]::new()
    foreach ($window in @([DokkomplektWindowProbe]::GetVisibleWindows(0))) {
        if ($BaselineHandles.Contains([long] $window.Handle)) { continue }
        $owner = Get-Process -Id ([int] $window.ProcessId) -ErrorAction SilentlyContinue
        $ownerName = if ($null -eq $owner) { '' } else { $owner.ProcessName.ToLowerInvariant() }
        if ($window.ClassName -eq 'ConsoleWindowClass' -or $forbiddenProcessNames -contains $ownerName) {
            $unexpected.Add([ordered]@{
                handle = [long] $window.Handle
                process_id = [int] $window.ProcessId
                process_name = $ownerName
                class_name = [string] $window.ClassName
                title = [string] $window.Title
            })
        }
    }
    return @($unexpected)
}

function Assert-NoNewVisibleConsoleWindows {
    param(
        [Parameter(Mandatory = $true)] [System.Collections.Generic.HashSet[long]] $BaselineHandles
    )
    $unexpected = @(Get-NewVisibleConsoleWindows -BaselineHandles $BaselineHandles)
    if ($unexpected.Count -gt 0) {
        $summary = ($unexpected | ForEach-Object {
            "$($_.process_name):$($_.process_id):$($_.class_name):$($_.title)"
        }) -join ', '
        throw "Unexpected visible console or script-host window appeared during installed GUI launch: $summary"
    }
}

function Wait-VisibleApplicationWindow {
    param(
        [Parameter(Mandatory = $true)] [Diagnostics.Process] $Process,
        [Parameter(Mandatory = $true)] [System.Collections.Generic.HashSet[long]] $BaselineHandles,
        [int] $TimeoutSeconds = 20
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "Installed application exited before a visible GUI window appeared: $($Process.ExitCode)"
        }
        Assert-NoNewVisibleConsoleWindows -BaselineHandles $BaselineHandles
        $windows = @([DokkomplektWindowProbe]::GetVisibleWindows([uint32] $Process.Id))
        $window = $windows | Where-Object { -not [string]::IsNullOrWhiteSpace($_.Title) } | Select-Object -First 1
        if ($null -ne $window) { return $window }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "No visible titled GUI window appeared for process $($Process.Id) within $TimeoutSeconds seconds."
}

function Start-And-ProbeInstalledApplication {
    param(
        [Parameter(Mandatory = $true)] [string] $ApplicationPath,
        [Parameter(Mandatory = $true)] [string] $LaunchLabel
    )
    $baselineHandles = [System.Collections.Generic.HashSet[long]]::new()
    foreach ($existingWindow in @([DokkomplektWindowProbe]::GetVisibleWindows(0))) {
        $baselineHandles.Add([long] $existingWindow.Handle) | Out-Null
    }
    $startedAt = [DateTime]::UtcNow
    $process = Start-Process -FilePath $ApplicationPath -PassThru
    try {
        $window = Wait-VisibleApplicationWindow -Process $process -BaselineHandles $baselineHandles
        $observationMilliseconds = 3000
        $observationDeadline = [DateTime]::UtcNow.AddMilliseconds($observationMilliseconds)
        do {
            $process.Refresh()
            if ($process.HasExited) {
                throw "Installed application exited during GUI/console observation: $($process.ExitCode)"
            }
            Assert-NoNewVisibleConsoleWindows -BaselineHandles $baselineHandles
            Start-Sleep -Milliseconds 200
        } while ([DateTime]::UtcNow -lt $observationDeadline)
        return [ordered]@{
            launch = $LaunchLabel
            process_id = $process.Id
            started_at_utc = $startedAt.ToString('o')
            observed_at_utc = [DateTime]::UtcNow.ToString('o')
            visible_window_handle = [long] $window.Handle
            visible_window_title = [string] $window.Title
            visible_window_class = [string] $window.ClassName
            console_observation_milliseconds = $observationMilliseconds
            unexpected_visible_console_windows = @()
        }
    } finally {
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            $process.WaitForExit(10000) | Out-Null
        }
    }
}

$guiConsoleEvidencePath = Join-Path $releaseGate 'GUI_AND_CONSOLE_EVIDENCE.json'
$launchEvidence = @(
    Start-And-ProbeInstalledApplication -ApplicationPath $app -LaunchLabel 'first-launch'
    Start-And-ProbeInstalledApplication -ApplicationPath $app -LaunchLabel 'restart'
)
if ($launchEvidence.Count -ne 2) { throw 'Both initial launch and restart GUI evidence are required.' }
[ordered]@{
    schema = 'dokkomplekt.gui-console-evidence.v1'
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    application_name = (Split-Path $app -Leaf)
    application_sha256 = (Get-FileHash $app -Algorithm SHA256).Hash.ToLowerInvariant()
    launches = $launchEvidence
} | ConvertTo-Json -Depth 7 | Set-Content $guiConsoleEvidencePath -Encoding utf8

$sourceSha256 = (python scripts/source_fingerprint.py).Trim()
$watchFolder = Join-Path $env:TEMP ("DokkomplektHardwareE2E-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $watchFolder | Out-Null
$cleanupEvidence = Join-Path $releaseGate 'WATCHER_UNINSTALL.json'
$installEvidence = Join-Path $releaseGate 'WATCHER_INSTALL.json'
$env:DOKKOMPLEKT_RUN_HARDWARE_E2E = '1'
$cleanup = Start-Process -FilePath $app -ArgumentList @(
    '--e2e-uninstall-watcher',
    "--e2e-evidence=$cleanupEvidence"
) -Wait -PassThru
if ($cleanup.ExitCode -ne 0) { throw "Watcher cleanup command failed: $($cleanup.ExitCode)" }
$watcherInstall = Start-Process -FilePath $app -ArgumentList @(
    "--e2e-install-watcher=$watchFolder",
    "--e2e-evidence=$installEvidence"
) -Wait -PassThru
if ($watcherInstall.ExitCode -ne 0) { throw "Watcher installation command failed: $($watcherInstall.ExitCode)" }
if (-not (Test-Path -LiteralPath $installEvidence -PathType Leaf)) { throw 'Watcher install evidence was not written by the application.' }
$watcherInstallRecord = Get-Content -LiteralPath $installEvidence -Raw | ConvertFrom-Json
if ($watcherInstallRecord.action -ne 'install' -or $watcherInstallRecord.watch_folder -ne $watchFolder) {
    throw 'Watcher install evidence does not describe this test folder.'
}
$startupEvidence = @(
    Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -like '*Dokkomplekt*' },
    Get-CimInstance Win32_StartupCommand -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -like '*Dokkomplekt*' -and $_.Command -match '--background-watch'
    }
) | Where-Object { $null -ne $_ }
if ($startupEvidence.Count -eq 0) {
    throw 'Watcher/autostart registration created by this scenario was not found.'
}

$verifiedRebootPath = Join-Path $releaseGate 'WINDOWS_REBOOT_E2E_PASSED.json'
if ($env:DOKKOMPLEKT_PREPARE_REBOOT_E2E -eq '1') {
    if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT)) {
        throw 'DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT is required in prepare-reboot mode.'
    }
    $rawEvidence = if ([string]::IsNullOrWhiteSpace($rebootEvidencePath)) {
        Join-Path $releaseGate 'WINDOWS_REBOOT_E2E_RAW.json'
    } else { $rebootEvidencePath }
    & "$PSScriptRoot/prepare_reboot_evidence.ps1" `
        -AppPath $app `
        -WatchFolder $watchFolder `
        -SourceDocument $env:DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT `
        -ExpectedSourceSha256 $sourceSha256 `
        -EvidencePath $rawEvidence
    throw 'Reboot evidence preparation completed. Reboot the runner, log in, then rerun this gate without DOKKOMPLEKT_PREPARE_REBOOT_E2E.'
}
if ([string]::IsNullOrWhiteSpace($rebootEvidencePath)) {
    throw 'DOKKOMPLEKT_REBOOT_EVIDENCE_PATH must point to evidence produced after a real Windows reboot.'
}
& "$PSScriptRoot/verify_reboot_evidence.ps1" `
    -EvidencePath $rebootEvidencePath `
    -ExpectedSourceSha256 $sourceSha256 `
    -OutputPath $verifiedRebootPath
$verifiedReboot = Get-Content -LiteralPath $verifiedRebootPath -Raw | ConvertFrom-Json

$finalCleanup = Start-Process -FilePath $app -ArgumentList @(
    '--e2e-uninstall-watcher',
    "--e2e-evidence=$cleanupEvidence"
) -Wait -PassThru
if ($finalCleanup.ExitCode -ne 0) { throw "Final watcher cleanup failed: $($finalCleanup.ExitCode)" }
$uninstallers = @(Get-ChildItem -LiteralPath $installDir -Recurse -File -Filter '*.exe' | Where-Object { $_.Name -match 'uninstall' })
if ($uninstallers.Count -ne 1) { throw "Expected exactly one NSIS uninstaller, found $($uninstallers.Count)." }
$uninstall = Start-Process -FilePath $uninstallers[0].FullName -ArgumentList '/S' -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "NSIS silent uninstall failed with exit code $($uninstall.ExitCode)" }
Start-Sleep -Seconds 2
if (Test-Path -LiteralPath $app -PathType Leaf) { throw 'Installed application still exists after silent uninstall.' }

$printEvidencePath = Join-Path $releaseGate 'PRINT_EVENT_307.json'
$signatureEvidencePath = Join-Path $releaseGate 'AUTHENTICODE_SIGNATURES.json'
if (-not (Test-Path -LiteralPath $guiConsoleEvidencePath -PathType Leaf)) {
    throw 'GUI and console evidence is missing.'
}
[ordered]@{
    schema = 'dokkomplekt.windows-hardware-e2e.v3'
    completed_at_utc = [DateTime]::UtcNow.ToString('o')
    source_sha256 = $sourceSha256
    printer = $printer.Name
    installer = $installer.Name
    installer_sha256 = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    word_available = $true
    watcher_autostart_found = $true
    application_restart_passed = $true
    gui_window_observed = $true
    unexpected_console_windows_observed = $false
    gui_console_evidence_sha256 = (Get-FileHash $guiConsoleEvidencePath -Algorithm SHA256).Hash.ToLowerInvariant()
    operating_system_reboot_tested = $true
    watcher_started_after_reboot = $verifiedReboot.watcher_started_after_reboot
    post_reboot_case_completed = $verifiedReboot.post_reboot_case_completed
    post_reboot_output_sha256 = $verifiedReboot.post_reboot_output_sha256
    print_spooler_completion_observed = $true
    print_event_count = $printEvents.Count
    print_event_evidence_sha256 = (Get-FileHash $printEvidencePath -Algorithm SHA256).Hash.ToLowerInvariant()
    authenticode_evidence_sha256 = (Get-FileHash $signatureEvidencePath -Algorithm SHA256).Hash.ToLowerInvariant()
    installed_application_signature_valid = $true
    silent_uninstall_passed = $true
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $releaseGate 'WINDOWS_HARDWARE_E2E_PASSED.json') -Encoding utf8
Remove-Item -LiteralPath $watchFolder -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "WINDOWS HARDWARE E2E PASSED: printer=$($printer.Name); installer=$($installer.Name); reboot=true; uninstall=true"