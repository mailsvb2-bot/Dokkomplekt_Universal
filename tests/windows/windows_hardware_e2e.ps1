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

$installers = @(Get-ChildItem -LiteralPath $InstallerRoot -Recurse -File -Filter '*.exe')
if ($installers.Count -eq 0) { throw "No NSIS installer found under $InstallerRoot" }
$installer = $installers | Sort-Object Length -Descending | Select-Object -First 1
$signature = Get-AuthenticodeSignature $installer.FullName
if ($signature.Status -ne 'Valid') { throw "Installer is not validly signed: $($signature.Status)" }

$process = Start-Process -FilePath $installer.FullName -ArgumentList '/S' -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Silent NSIS install failed with exit code $($process.ExitCode)" }
$installCandidates = @(
    "$env:ProgramFiles\Dokkomplekt Universal\Dokkomplekt Universal.exe",
    "$env:ProgramFiles\Dokkomplekt\Dokkomplekt.exe"
)
$app = $installCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($app)) { throw 'Installed Dokkomplekt executable was not found.' }
$installedSignature = Get-AuthenticodeSignature -FilePath $app
if ($installedSignature.Status -ne 'Valid') { throw "Installed application is not validly signed: $($installedSignature.Status)" }
$first = Start-Process -FilePath $app -PassThru
Start-Sleep -Seconds 5
if ($first.HasExited) { throw "Installed app exited early: $($first.ExitCode)" }
Stop-Process -Id $first.Id -Force
$second = Start-Process -FilePath $app -PassThru
Start-Sleep -Seconds 5
if ($second.HasExited) { throw "Installed app failed to restart: $($second.ExitCode)" }
Stop-Process -Id $second.Id -Force

New-Item -ItemType Directory -Force -Path '.release-gate' | Out-Null
$sourceSha256 = (python scripts/source_fingerprint.py).Trim()
$watchFolder = Join-Path $env:TEMP ("DokkomplektHardwareE2E-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $watchFolder | Out-Null
$cleanupEvidence = (Resolve-Path '.release-gate').Path + '\\WATCHER_UNINSTALL.json'
$installEvidence = (Resolve-Path '.release-gate').Path + '\\WATCHER_INSTALL.json'
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

$verifiedRebootPath = (Resolve-Path '.release-gate').Path + '\\WINDOWS_REBOOT_E2E_PASSED.json'
if ($env:DOKKOMPLEKT_PREPARE_REBOOT_E2E -eq '1') {
    if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT)) {
        throw 'DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT is required in prepare-reboot mode.'
    }
    $rawEvidence = if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_REBOOT_EVIDENCE)) {
        (Resolve-Path '.release-gate').Path + '\\WINDOWS_REBOOT_E2E_RAW.json'
    } else { $env:DOKKOMPLEKT_REBOOT_EVIDENCE }
    & "$PSScriptRoot/prepare_reboot_evidence.ps1" `
        -AppPath $app `
        -WatchFolder $watchFolder `
        -SourceDocument $env:DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT `
        -ExpectedSourceSha256 $sourceSha256 `
        -EvidencePath $rawEvidence
    throw 'Reboot evidence preparation completed. Reboot the runner, log in, then rerun this gate without DOKKOMPLEKT_PREPARE_REBOOT_E2E.'
}
if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_REBOOT_EVIDENCE)) {
    throw 'DOKKOMPLEKT_REBOOT_EVIDENCE must point to evidence produced after a real Windows reboot.'
}
& "$PSScriptRoot/verify_reboot_evidence.ps1" `
    -EvidencePath $env:DOKKOMPLEKT_REBOOT_EVIDENCE `
    -ExpectedSourceSha256 $sourceSha256 `
    -OutputPath $verifiedRebootPath
$verifiedReboot = Get-Content -LiteralPath $verifiedRebootPath -Raw | ConvertFrom-Json

[ordered]@{
    schema = 'dokkomplekt.windows-hardware-e2e.v1'
    completed_at_utc = [DateTime]::UtcNow.ToString('o')
    source_sha256 = $sourceSha256
    printer = $printer.Name
    installer = $installer.Name
    installer_sha256 = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    word_available = $true
    watcher_autostart_found = $true
    application_restart_passed = $true
    operating_system_reboot_tested = $true
    watcher_started_after_reboot = $verifiedReboot.watcher_started_after_reboot
    post_reboot_case_completed = $verifiedReboot.post_reboot_case_completed
    post_reboot_output_sha256 = $verifiedReboot.post_reboot_output_sha256
    print_spooler_completion_observed = $true
    print_event_count = $printEvents.Count
    installed_application_signature_valid = $true
} | ConvertTo-Json -Depth 6 | Set-Content '.release-gate/WINDOWS_HARDWARE_E2E_PASSED.json' -Encoding utf8
$finalCleanup = Start-Process -FilePath $app -ArgumentList @(
    '--e2e-uninstall-watcher',
    "--e2e-evidence=$cleanupEvidence"
) -Wait -PassThru
if ($finalCleanup.ExitCode -ne 0) { throw "Final watcher cleanup failed: $($finalCleanup.ExitCode)" }
Write-Host "WINDOWS HARDWARE E2E PASSED: printer=$($printer.Name); installer=$($installer.Name); reboot=true"
