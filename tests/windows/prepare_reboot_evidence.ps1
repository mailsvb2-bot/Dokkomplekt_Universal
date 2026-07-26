param(
    [Parameter(Mandatory = $true)] [string] $AppPath,
    [Parameter(Mandatory = $true)] [string] $WatchFolder,
    [Parameter(Mandatory = $true)] [string] $SourceDocument,
    [Parameter(Mandatory = $true)] [string] $ExpectedSourceSha256,
    [Parameter(Mandatory = $true)] [string] $EvidencePath
)
$ErrorActionPreference = 'Stop'
if ($env:DOKKOMPLEKT_RUN_HARDWARE_E2E -ne '1') {
    throw 'Reboot E2E is opt-in: set DOKKOMPLEKT_RUN_HARDWARE_E2E=1 on a disposable Windows runner.'
}
foreach ($required in @($AppPath, $SourceDocument)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Required file is missing: $required" }
}
New-Item -ItemType Directory -Force -Path $WatchFolder | Out-Null
$root = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $root | Out-Null
$payload = Join-Path $root ('payload-' + [Guid]::NewGuid().ToString('N') + [IO.Path]::GetExtension($SourceDocument))
Copy-Item -LiteralPath $SourceDocument -Destination $payload -Force
$postScript = Join-Path $root 'verify-after-reboot.ps1'
$bootBefore = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
$escaped = {
    param([string] $Value)
    return $Value.Replace("'", "''")
}
$appEsc = & $escaped $AppPath
$watchEsc = & $escaped $WatchFolder
$payloadEsc = & $escaped $payload
$evidenceEsc = & $escaped $EvidencePath
$sourceShaEsc = & $escaped $ExpectedSourceSha256
@"
`$ErrorActionPreference = 'Stop'
`$bootAfter = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
`$watcher = `$null
for (`$i = 0; `$i -lt 60 -and `$null -eq `$watcher; `$i++) {
    `$watcher = Get-CimInstance Win32_Process -Filter "Name='$([IO.Path]::GetFileName($AppPath))'" -ErrorAction SilentlyContinue |
        Where-Object { `$_.CommandLine -match '--background-watch' } | Select-Object -First 1
    if (`$null -eq `$watcher) { Start-Sleep -Seconds 2 }
}
`$watcherStarted = `$null -ne `$watcher
`$destination = Join-Path '$watchEsc' ([IO.Path]::GetFileName('$payloadEsc'))
Copy-Item -LiteralPath '$payloadEsc' -Destination `$destination -Force
`$output = `$null
for (`$i = 0; `$i -lt 120 -and `$null -eq `$output; `$i++) {
    `$output = Get-ChildItem -LiteralPath '$watchEsc' -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { `$_.FullName -ne `$destination -and `$_.Extension -in @('.docx', '.pdf') } |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (`$null -eq `$output) { Start-Sleep -Seconds 2 }
}
`$completed = `$null -ne `$output
`$hash = if (`$completed) { (Get-FileHash -LiteralPath `$output.FullName -Algorithm SHA256).Hash.ToLowerInvariant() } else { '' }
`$record = [ordered]@{
    schema = 'dokkomplekt.windows-reboot-e2e.v1'
    source_sha256 = '$sourceShaEsc'
    boot_id_before = '$bootBefore'
    boot_id_after = `$bootAfter
    watcher_started_after_reboot = `$watcherStarted
    post_reboot_case_completed = `$completed
    post_reboot_output_sha256 = `$hash
    post_reboot_output_path = if (`$completed) { `$output.FullName } else { '' }
}
`$parent = Split-Path -Parent '$evidenceEsc'
if (-not [string]::IsNullOrWhiteSpace(`$parent)) { New-Item -ItemType Directory -Force -Path `$parent | Out-Null }
`$record | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath '$evidenceEsc' -Encoding utf8
Unregister-ScheduledTask -TaskName 'DokkomplektE2EAfterReboot' -Confirm:`$false -ErrorAction SilentlyContinue
"@ | Set-Content -LiteralPath $postScript -Encoding utf8
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$postScript`""
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Highest
Register-ScheduledTask -TaskName 'DokkomplektE2EAfterReboot' -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
[ordered]@{
    schema = 'dokkomplekt.windows-reboot-e2e.pending.v1'
    source_sha256 = $ExpectedSourceSha256
    boot_id_before = $bootBefore
    app_path = $AppPath
    watch_folder = $WatchFolder
    evidence_path = $EvidencePath
    scheduled_task = 'DokkomplektE2EAfterReboot'
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'pending-reboot.json') -Encoding utf8
Write-Host "REBOOT E2E PREPARED. Reboot Windows and log in as $env:USERNAME; evidence will be written to $EvidencePath"
