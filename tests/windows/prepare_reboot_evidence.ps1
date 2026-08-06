param(
    [Parameter(Mandatory = $true)] [string] $AppPath,
    [Parameter(Mandatory = $true)] [string] $WatchFolder,
    [Parameter(Mandatory = $true)] [string] $SourceDocument,
    [Parameter(Mandatory = $true)] [string] $ExpectedSourceSha256,
    [Parameter(Mandatory = $true)] [string] $EvidencePath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-Sha256 {
    param([Parameter(Mandatory = $true)] [string] $Value, [Parameter(Mandatory = $true)] [string] $Label)
    $normalized = $Value.Trim().ToLowerInvariant()
    if ($normalized -notmatch '^[0-9a-f]{64}$') { throw "$Label must be a lowercase SHA-256 value." }
    return $normalized
}

function Resolve-DirectFile {
    param([Parameter(Mandatory = $true)] [string] $Path, [Parameter(Mandatory = $true)] [string] $Label)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw "$Label must be an absolute path." }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw "$Label must be a direct regular file, not a directory or reparse point."
    }
    return $item.FullName
}

function Escape-SingleQuotedPowerShell {
    param([Parameter(Mandatory = $true)] [string] $Value)
    return $Value.Replace("'", "''")
}

if ($env:DOKKOMPLEKT_RUN_HARDWARE_E2E -ne '1') {
    throw 'Reboot E2E is opt-in: set DOKKOMPLEKT_RUN_HARDWARE_E2E=1 on a disposable Windows runner.'
}
$sourceTreeSha = Assert-Sha256 -Value $ExpectedSourceSha256 -Label 'ExpectedSourceSha256'
$app = Resolve-DirectFile -Path $AppPath -Label 'AppPath'
$source = Resolve-DirectFile -Path $SourceDocument -Label 'SourceDocument'
if (-not [IO.Path]::IsPathFullyQualified($WatchFolder)) { throw 'WatchFolder must be an absolute path.' }
if (-not [IO.Path]::IsPathFullyQualified($EvidencePath)) { throw 'EvidencePath must be an absolute path.' }
if ([IO.Path]::GetExtension($EvidencePath) -ine '.json') { throw 'EvidencePath must end with .json.' }

New-Item -ItemType Directory -Force -Path $WatchFolder | Out-Null
$watch = (Resolve-Path -LiteralPath $WatchFolder).Path
$watchItem = Get-Item -LiteralPath $watch -Force
if (($watchItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'WatchFolder must not be a reparse point.'
}
$evidenceParent = Split-Path -Parent $EvidencePath
if ([string]::IsNullOrWhiteSpace($evidenceParent)) { throw 'EvidencePath must have a parent directory.' }
New-Item -ItemType Directory -Force -Path $evidenceParent | Out-Null
$evidenceParentItem = Get-Item -LiteralPath $evidenceParent -Force
if (($evidenceParentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'EvidencePath parent must not be a reparse point.'
}
$evidence = Join-Path (Resolve-Path -LiteralPath $evidenceParent).Path (Split-Path -Leaf $EvidencePath)
if (Test-Path -LiteralPath $evidence) { throw 'EvidencePath must not already exist.' }

$root = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $root | Out-Null
$rootItem = Get-Item -LiteralPath $root -Force
if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'DokkomplektE2E root must not be a reparse point.'
}
$pendingPath = Join-Path $root 'pending-reboot.json'
$taskName = 'DokkomplektE2EAfterReboot'
if (Test-Path -LiteralPath $pendingPath) { throw 'A reboot E2E plan is already pending; verify or remove it explicitly.' }
if ($null -ne (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)) {
    throw 'The reboot E2E scheduled task already exists.'
}

$nonce = [Guid]::NewGuid().ToString('N')
$appSha = (Get-FileHash -LiteralPath $app -Algorithm SHA256).Hash.ToLowerInvariant()
$sourceDocumentSha = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
$payload = Join-Path $root ("payload-$nonce" + [IO.Path]::GetExtension($source))
Copy-Item -LiteralPath $source -Destination $payload -Force
$payload = Resolve-DirectFile -Path $payload -Label 'Payload copy'
$payloadSha = (Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash.ToLowerInvariant()
if ($payloadSha -ne $sourceDocumentSha) { throw 'Payload copy hash does not match SourceDocument.' }

$windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$windowsPowerShell = Resolve-DirectFile -Path $windowsPowerShell -Label 'Windows PowerShell'
$windowsPowerShellSha = (Get-FileHash -LiteralPath $windowsPowerShell -Algorithm SHA256).Hash.ToLowerInvariant()
$postScript = Join-Path $root "verify-after-reboot-$nonce.ps1"
$bootBefore = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')

$appEsc = Escape-SingleQuotedPowerShell $app
$appShaEsc = Escape-SingleQuotedPowerShell $appSha
$watchEsc = Escape-SingleQuotedPowerShell $watch
$payloadEsc = Escape-SingleQuotedPowerShell $payload
$payloadShaEsc = Escape-SingleQuotedPowerShell $payloadSha
$evidenceEsc = Escape-SingleQuotedPowerShell $evidence
$sourceTreeShaEsc = Escape-SingleQuotedPowerShell $sourceTreeSha
$nonceEsc = Escape-SingleQuotedPowerShell $nonce
$bootBeforeEsc = Escape-SingleQuotedPowerShell $bootBefore
$powerShellEsc = Escape-SingleQuotedPowerShell $windowsPowerShell
$powerShellShaEsc = Escape-SingleQuotedPowerShell $windowsPowerShellSha

@"
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
`$expectedAppPath = '$appEsc'
`$expectedAppSha256 = '$appShaEsc'
`$watchFolder = '$watchEsc'
`$payloadPath = '$payloadEsc'
`$expectedPayloadSha256 = '$payloadShaEsc'
`$evidencePath = '$evidenceEsc'
`$sourceTreeSha256 = '$sourceTreeShaEsc'
`$nonce = '$nonceEsc'
`$bootBefore = '$bootBeforeEsc'
`$expectedPowerShellPath = '$powerShellEsc'
`$expectedPowerShellSha256 = '$powerShellShaEsc'

function Get-NormalizedPath([string] `$Path) { return [IO.Path]::GetFullPath(`$Path) }
function Get-Sha256([string] `$Path) { return (Get-FileHash -LiteralPath `$Path -Algorithm SHA256).Hash.ToLowerInvariant() }

try {
`$bootAfter = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
`$actualPowerShellPath = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
if ((Get-NormalizedPath `$actualPowerShellPath) -ine (Get-NormalizedPath `$expectedPowerShellPath)) {
    throw 'Post-reboot task is running under an unexpected PowerShell executable.'
}
`$actualPowerShellSha256 = Get-Sha256 `$actualPowerShellPath
if (`$actualPowerShellSha256 -ne `$expectedPowerShellSha256) { throw 'Windows PowerShell hash changed before reboot verification.' }
if ((Get-Sha256 `$expectedAppPath) -ne `$expectedAppSha256) { throw 'Installed application hash changed before reboot verification.' }
if ((Get-Sha256 `$payloadPath) -ne `$expectedPayloadSha256) { throw 'Reboot payload hash changed before processing.' }

`$watcher = `$null
`$watcherExecutablePath = ''
`$watcherExecutableSha256 = ''
`$expectedName = [IO.Path]::GetFileName(`$expectedAppPath)
for (`$i = 0; `$i -lt 60 -and `$null -eq `$watcher; `$i++) {
    foreach (`$candidate in @(Get-CimInstance Win32_Process -Filter "Name='`$expectedName'" -ErrorAction SilentlyContinue)) {
        if (`$candidate.CommandLine -notmatch '(?i)(?:^|\s)--background-watch(?:\s|$)' -or [string]::IsNullOrWhiteSpace(`$candidate.ExecutablePath)) { continue }
        `$candidatePath = Get-NormalizedPath `$candidate.ExecutablePath
        if (`$candidatePath -ine (Get-NormalizedPath `$expectedAppPath)) { continue }
        `$candidateSha = Get-Sha256 `$candidatePath
        if (`$candidateSha -ne `$expectedAppSha256) { continue }
        `$watcher = `$candidate
        `$watcherExecutablePath = `$candidatePath
        `$watcherExecutableSha256 = `$candidateSha
        break
    }
    if (`$null -eq `$watcher) { Start-Sleep -Seconds 2 }
}
`$watcherStarted = `$null -ne `$watcher

`$baselinePaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach (`$existing in @(Get-ChildItem -LiteralPath `$watchFolder -Recurse -File -ErrorAction SilentlyContinue | Where-Object { `$_.Extension -in @('.docx', '.pdf') })) {
    `$baselinePaths.Add((Get-NormalizedPath `$existing.FullName)) | Out-Null
}
`$destination = Join-Path `$watchFolder ([IO.Path]::GetFileName(`$payloadPath))
if (Test-Path -LiteralPath `$destination) { throw 'Unique reboot destination already exists.' }
`$caseStartedAt = [DateTime]::UtcNow
Copy-Item -LiteralPath `$payloadPath -Destination `$destination
`$destinationSha256 = Get-Sha256 `$destination
if (`$destinationSha256 -ne `$expectedPayloadSha256) { throw 'Destination payload hash mismatch.' }

`$archiveReceipt = `$null
`$archiveReceiptSha256 = ''
`$archivedSource = `$null
`$output = `$null
`$outputSha256 = ''
for (`$i = 0; `$i -lt 120 -and (`$null -eq `$archiveReceipt -or `$null -eq `$output); `$i++) {
    if (`$null -eq `$archiveReceipt) {
        foreach (`$receiptCandidate in @(Get-ChildItem -LiteralPath `$watchFolder -Recurse -File -Filter '*.dokkomplekt-receipt.json' -ErrorAction SilentlyContinue |
            Where-Object { `$_.LastWriteTimeUtc -ge `$caseStartedAt.AddSeconds(-2) } |
            Sort-Object LastWriteTimeUtc -Descending)) {
            try {
                `$receiptPayload = Get-Content -LiteralPath `$receiptCandidate.FullName -Raw | ConvertFrom-Json
                if ([int] `$receiptPayload.schema -ne 1 -or
                    `$receiptPayload.sha256 -ne `$expectedPayloadSha256 -or
                    `$receiptPayload.original_name -ne [IO.Path]::GetFileName(`$destination)) { continue }
                `$archivedCandidate = Join-Path `$receiptCandidate.DirectoryName ([string] `$receiptPayload.archived_name)
                if (-not (Test-Path -LiteralPath `$archivedCandidate -PathType Leaf)) { continue }
                `$archivedCandidateSha = Get-Sha256 `$archivedCandidate
                if (`$archivedCandidateSha -ne `$expectedPayloadSha256) { continue }
                `$archiveReceipt = `$receiptCandidate
                `$archiveReceiptSha256 = Get-Sha256 `$receiptCandidate.FullName
                `$archivedSource = Get-Item -LiteralPath `$archivedCandidate -Force
                break
            } catch { }
        }
    }
    if (`$null -eq `$output) {
        `$candidates = @(Get-ChildItem -LiteralPath `$watchFolder -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object {
                `$normalized = Get-NormalizedPath `$_.FullName
                `$normalized -ine (Get-NormalizedPath `$destination) -and
                `$_.Extension -in @('.docx', '.pdf') -and
                -not `$baselinePaths.Contains(`$normalized) -and
                `$_.LastWriteTimeUtc -ge `$caseStartedAt.AddSeconds(-2)
            } | Sort-Object LastWriteTimeUtc -Descending)
        foreach (`$candidate in `$candidates) {
            if (`$candidate.Length -le 0) { continue }
            try {
                `$candidateSha = Get-Sha256 `$candidate.FullName
                if (`$candidateSha -match '^[0-9a-f]{64}$' -and `$candidateSha -ne `$expectedPayloadSha256) {
                    `$output = `$candidate
                    `$outputSha256 = `$candidateSha
                    break
                }
            } catch { }
        }
    }
    if (`$null -eq `$archiveReceipt -or `$null -eq `$output) { Start-Sleep -Seconds 2 }
}
`$completed = `$watcherStarted -and `$null -ne `$archiveReceipt -and `$null -ne `$archivedSource -and `$null -ne `$output
`$record = [ordered]@{
    schema = 'dokkomplekt.windows-reboot-e2e.v2'
    nonce = `$nonce
    source_tree_sha256 = `$sourceTreeSha256
    boot_id_before = `$bootBefore
    boot_id_after = `$bootAfter
    application_path = `$expectedAppPath
    application_sha256 = `$expectedAppSha256
    powershell_path = `$actualPowerShellPath
    powershell_sha256 = `$actualPowerShellSha256
    watcher_started_after_reboot = `$watcherStarted
    watcher_process_id = if (`$watcherStarted) { [int] `$watcher.ProcessId } else { 0 }
    watcher_executable_path = `$watcherExecutablePath
    watcher_executable_sha256 = `$watcherExecutableSha256
    payload_path = `$payloadPath
    payload_sha256 = `$expectedPayloadSha256
    destination_path = `$destination
    destination_sha256 = `$destinationSha256
    archive_receipt_path = if (`$null -ne `$archiveReceipt) { `$archiveReceipt.FullName } else { '' }
    archive_receipt_sha256 = if (`$null -ne `$archiveReceipt) { `$archiveReceiptSha256 } else { '' }
    archive_receipt_last_write_utc = if (`$null -ne `$archiveReceipt) { `$archiveReceipt.LastWriteTimeUtc.ToString('o') } else { '' }
    archived_source_path = if (`$null -ne `$archivedSource) { `$archivedSource.FullName } else { '' }
    archived_source_sha256 = if (`$null -ne `$archivedSource) { `$expectedPayloadSha256 } else { '' }
    case_started_at_utc = `$caseStartedAt.ToString('o')
    post_reboot_case_completed = `$completed
    post_reboot_output_path = if (`$completed) { `$output.FullName } else { '' }
    post_reboot_output_sha256 = if (`$completed) { `$outputSha256 } else { '' }
    post_reboot_output_size_bytes = if (`$completed) { [long] `$output.Length } else { 0 }
    post_reboot_output_last_write_utc = if (`$completed) { `$output.LastWriteTimeUtc.ToString('o') } else { '' }
    evidence_created_at_utc = [DateTime]::UtcNow.ToString('o')
}
`$parent = Split-Path -Parent `$evidencePath
if (-not [string]::IsNullOrWhiteSpace(`$parent)) { New-Item -ItemType Directory -Force -Path `$parent | Out-Null }
`$temporaryEvidence = `$evidencePath + '.' + `$nonce + '.tmp'
`$record | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath `$temporaryEvidence -Encoding utf8
Move-Item -LiteralPath `$temporaryEvidence -Destination `$evidencePath -Force
} finally {
    Unregister-ScheduledTask -TaskName '$taskName' -Confirm:`$false -ErrorAction SilentlyContinue
}
"@ | Set-Content -LiteralPath $postScript -Encoding utf8
$postScript = Resolve-DirectFile -Path $postScript -Label 'Post-reboot script'
$postScriptSha = (Get-FileHash -LiteralPath $postScript -Algorithm SHA256).Hash.ToLowerInvariant()

$pending = [ordered]@{
    schema = 'dokkomplekt.windows-reboot-e2e.pending.v2'
    nonce = $nonce
    source_tree_sha256 = $sourceTreeSha
    boot_id_before = $bootBefore
    application_path = $app
    application_sha256 = $appSha
    source_document_path = $source
    source_document_sha256 = $sourceDocumentSha
    payload_path = $payload
    payload_sha256 = $payloadSha
    watch_folder = $watch
    evidence_path = $evidence
    post_script_path = $postScript
    post_script_sha256 = $postScriptSha
    powershell_path = $windowsPowerShell
    powershell_sha256 = $windowsPowerShellSha
    scheduled_task = $taskName
}
$pendingTemporary = "$pendingPath.$nonce.tmp"
$pending | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $pendingTemporary -Encoding utf8
Move-Item -LiteralPath $pendingTemporary -Destination $pendingPath

try {
    $action = New-ScheduledTaskAction -Execute $windowsPowerShell -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$postScript`""
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Highest
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
} catch {
    Remove-Item -LiteralPath $pendingPath, $postScript, $payload -Force -ErrorAction SilentlyContinue
    throw
}
Write-Host "REBOOT E2E V2 PREPARED. Reboot Windows and log in as $env:USERNAME; evidence will be written to $evidence"
