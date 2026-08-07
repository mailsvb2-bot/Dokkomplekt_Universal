param(
    [Parameter(Mandatory = $true)] [string] $EvidencePath,
    [Parameter(Mandatory = $true)] [string] $ExpectedSourceSha256,
    [string] $OutputPath = ".release-gate/WINDOWS_REBOOT_E2E_PASSED.json"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Normalize-Sha256 {
    param([Parameter(Mandatory = $true)] [string] $Value, [Parameter(Mandatory = $true)] [string] $Label)
    $normalized = $Value.Trim().ToLowerInvariant()
    if ($normalized -notmatch '^[0-9a-f]{64}$') { throw "$Label must be a SHA-256 value." }
    return $normalized
}
function Normalize-PathValue([string] $Path) { return [IO.Path]::GetFullPath($Path) }
function Require-DirectFile([string] $Path, [string] $Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label file is missing." }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw "$Label must be a direct regular file, not a reparse point."
    }
    return $item
}
function Require-PathEqual([string] $Actual, [string] $Expected, [string] $Label) {
    if ((Normalize-PathValue $Actual) -ine (Normalize-PathValue $Expected)) { throw "$Label path mismatch." }
}
function Require-FileHash([string] $Path, [string] $ExpectedSha256, [string] $Label) {
    $item = Require-DirectFile $Path $Label
    $actual = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne (Normalize-Sha256 $ExpectedSha256 "$Label expected hash")) { throw "$Label SHA-256 mismatch." }
    return $actual
}
function Parse-Utc([string] $Value, [string] $Label) {
    if ([string]::IsNullOrWhiteSpace($Value)) { throw "$Label is required." }
    try { return [DateTimeOffset]::Parse($Value).ToUniversalTime() } catch { throw "$Label is invalid." }
}

$expectedSource = Normalize-Sha256 $ExpectedSourceSha256 'ExpectedSourceSha256'
if (-not [IO.Path]::IsPathFullyQualified($EvidencePath)) { throw 'EvidencePath must be absolute.' }
$evidenceItem = Require-DirectFile $EvidencePath 'Reboot evidence'
$EvidencePath = $evidenceItem.FullName
$root = Join-Path $env:ProgramData 'DokkomplektE2E'
$pendingPath = Join-Path $root 'pending-reboot.json'
$pendingItem = Require-DirectFile $pendingPath 'Pending reboot plan'
$pendingPath = $pendingItem.FullName
$pending = Get-Content -LiteralPath $pendingPath -Raw | ConvertFrom-Json
$evidence = Get-Content -LiteralPath $EvidencePath -Raw | ConvertFrom-Json
if ($pending.schema -ne 'dokkomplekt.windows-reboot-e2e.pending.v2') { throw 'Unsupported pending reboot plan schema.' }
if ($evidence.schema -ne 'dokkomplekt.windows-reboot-e2e.v2') { throw 'Unsupported reboot evidence schema.' }
if ($pending.source_tree_sha256 -ne $expectedSource -or $evidence.source_tree_sha256 -ne $expectedSource) {
    throw 'Reboot evidence belongs to another source tree.'
}
if ($pending.nonce -notmatch '^[0-9a-f]{32}$' -or $evidence.nonce -ne $pending.nonce) { throw 'Reboot nonce mismatch.' }
Require-PathEqual $EvidencePath $pending.evidence_path 'Evidence'
if ($evidence.boot_id_before -ne $pending.boot_id_before) { throw 'Boot-before identifier mismatch.' }
$bootBefore = Parse-Utc $evidence.boot_id_before 'boot_id_before'
$bootAfter = Parse-Utc $evidence.boot_id_after 'boot_id_after'
if ($bootAfter -le $bootBefore) { throw 'No operating-system reboot was demonstrated.' }
if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT -and $null -ne (Get-Command Get-CimInstance -ErrorAction SilentlyContinue)) {
    $currentBoot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime()
    if ([Math]::Abs(($currentBoot - $bootAfter.UtcDateTime).TotalSeconds) -gt 2) {
        throw 'Raw reboot evidence does not match the current Windows boot.'
    }
}
if ($evidence.watcher_started_after_reboot -ne $true -or [int] $evidence.watcher_process_id -le 0) {
    throw 'Watcher did not start after reboot.'
}

Require-PathEqual $evidence.application_path $pending.application_path 'Application'
$appSha = Require-FileHash $pending.application_path $pending.application_sha256 'Installed application'
if ($evidence.application_sha256 -ne $appSha) { throw 'Evidence application hash mismatch.' }
Require-PathEqual $evidence.watcher_executable_path $pending.application_path 'Watcher executable'
if ($evidence.watcher_executable_sha256 -ne $appSha) { throw 'Watcher executable hash mismatch.' }
Require-PathEqual $evidence.powershell_path $pending.powershell_path 'PowerShell'
$powerShellSha = Require-FileHash $pending.powershell_path $pending.powershell_sha256 'PowerShell'
if ($evidence.powershell_sha256 -ne $powerShellSha) { throw 'Evidence PowerShell hash mismatch.' }
$postScriptSha = Require-FileHash $pending.post_script_path $pending.post_script_sha256 'Post-reboot script'
$payloadSha = Require-FileHash $pending.payload_path $pending.payload_sha256 'Reboot payload'
if ($pending.source_document_sha256 -ne $payloadSha -or $evidence.payload_sha256 -ne $payloadSha) {
    throw 'Payload is not bound to the prepared source document.'
}
Require-PathEqual $evidence.payload_path $pending.payload_path 'Payload'
Require-PathEqual $evidence.destination_path (Join-Path $pending.watch_folder ([IO.Path]::GetFileName($pending.payload_path))) 'Destination'
$destinationSha = Require-FileHash $evidence.destination_path $payloadSha 'Destination payload'
if ($evidence.destination_sha256 -ne $destinationSha) { throw 'Destination evidence hash mismatch.' }

$receiptPath = Normalize-PathValue $evidence.archive_receipt_path
$watchItem = Get-Item -LiteralPath $pending.watch_folder -Force -ErrorAction Stop
if (-not $watchItem.PSIsContainer -or (($watchItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw 'Prepared watch folder must be a direct directory, not a reparse point.'
}
$watchPath = (Normalize-PathValue $watchItem.FullName).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
$watchPrefix = $watchPath + [IO.Path]::DirectorySeparatorChar
if (-not $receiptPath.StartsWith($watchPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Archive receipt is outside the prepared watch folder.'
}
$receiptSha = Require-FileHash $receiptPath $evidence.archive_receipt_sha256 'Archive receipt'
$receiptPayload = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
if ([int] $receiptPayload.schema -ne 1 -or $receiptPayload.sha256 -ne $payloadSha -or
    $receiptPayload.original_name -ne [IO.Path]::GetFileName($evidence.destination_path)) {
    throw 'Archive receipt is not bound to the prepared payload.'
}
$archivedSourcePath = Normalize-PathValue $evidence.archived_source_path
$expectedArchivedPath = Normalize-PathValue (Join-Path (Split-Path -Parent $receiptPath) ([string] $receiptPayload.archived_name))
if (-not $expectedArchivedPath.StartsWith($watchPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Archived source is outside the prepared watch folder.'
}
Require-PathEqual $archivedSourcePath $expectedArchivedPath 'Archived source'
$archivedSourceSha = Require-FileHash $archivedSourcePath $payloadSha 'Archived source'
if ($evidence.archived_source_sha256 -ne $archivedSourceSha) { throw 'Archived source evidence hash mismatch.' }
$receiptLastWrite = Parse-Utc $evidence.archive_receipt_last_write_utc 'archive_receipt_last_write_utc'
$receiptItem = Get-Item -LiteralPath $receiptPath -Force
if ([Math]::Abs(($receiptItem.LastWriteTimeUtc - $receiptLastWrite.UtcDateTime).TotalSeconds) -gt 2) {
    throw 'Archive receipt timestamp mismatch.'
}

if ($evidence.post_reboot_case_completed -ne $true) { throw 'No post-reboot document case completed.' }
$outputSha = Normalize-Sha256 $evidence.post_reboot_output_sha256 'Post-reboot output hash'
if ($outputSha -eq $payloadSha) { throw 'Post-reboot output is the archived input payload, not a generated document.' }
$generatedOutputPath = Normalize-PathValue $evidence.post_reboot_output_path
if (-not $generatedOutputPath.StartsWith($watchPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Post-reboot output is outside the prepared watch folder.'
}
if ($generatedOutputPath -ieq (Normalize-PathValue $evidence.destination_path) -or $generatedOutputPath -ieq (Normalize-PathValue $pending.payload_path)) {
    throw 'Post-reboot output aliases the input payload.'
}
if ([IO.Path]::GetExtension($generatedOutputPath) -notin @('.docx', '.pdf')) { throw 'Post-reboot output has an unsupported extension.' }
$verifiedOutputSha = Require-FileHash $generatedOutputPath $outputSha 'Post-reboot output'
$outputItem = Get-Item -LiteralPath $generatedOutputPath -Force
if ([long] $evidence.post_reboot_output_size_bytes -ne [long] $outputItem.Length -or $outputItem.Length -le 0) {
    throw 'Post-reboot output size mismatch.'
}
$caseStarted = Parse-Utc $evidence.case_started_at_utc 'case_started_at_utc'
$outputLastWrite = Parse-Utc $evidence.post_reboot_output_last_write_utc 'post_reboot_output_last_write_utc'
$evidenceCreated = Parse-Utc $evidence.evidence_created_at_utc 'evidence_created_at_utc'
if ($caseStarted -lt $bootAfter -or $outputLastWrite -lt $caseStarted.AddSeconds(-2) -or $evidenceCreated -lt $caseStarted) {
    throw 'Reboot case timestamps do not prove post-boot processing.'
}
if ([Math]::Abs(($outputItem.LastWriteTimeUtc - $outputLastWrite.UtcDateTime).TotalSeconds) -gt 2) {
    throw 'Post-reboot output timestamp mismatch.'
}

$normalized = [ordered]@{
    schema = 'dokkomplekt.windows-reboot-e2e.verified.v2'
    verified_at_utc = [DateTime]::UtcNow.ToString('o')
    nonce = $pending.nonce
    source_sha256 = $expectedSource
    boot_id_before = $evidence.boot_id_before
    boot_id_after = $evidence.boot_id_after
    application_sha256 = $appSha
    powershell_sha256 = $powerShellSha
    post_script_sha256 = $postScriptSha
    watcher_started_after_reboot = $true
    watcher_process_id = [int] $evidence.watcher_process_id
    watcher_executable_sha256 = $appSha
    payload_sha256 = $payloadSha
    destination_sha256 = $destinationSha
    archive_receipt_sha256 = $receiptSha
    archive_receipt_last_write_utc = $receiptItem.LastWriteTimeUtc.ToString('o')
    archived_source_sha256 = $archivedSourceSha
    case_started_at_utc = $evidence.case_started_at_utc
    post_reboot_case_completed = $true
    post_reboot_output_path = $generatedOutputPath
    post_reboot_output_sha256 = $verifiedOutputSha
    post_reboot_output_size_bytes = [long] $outputItem.Length
    post_reboot_output_last_write_utc = $outputItem.LastWriteTimeUtc.ToString('o')
}
$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$temporaryOutput = "$OutputPath.$($pending.nonce).tmp"
$normalized | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $temporaryOutput -Encoding utf8
Move-Item -LiteralPath $temporaryOutput -Destination $OutputPath -Force
if ($null -ne (Get-Command Unregister-ScheduledTask -ErrorAction SilentlyContinue)) {
    Unregister-ScheduledTask -TaskName $pending.scheduled_task -Confirm:$false -ErrorAction SilentlyContinue
}
Remove-Item -LiteralPath $pending.post_script_path, $pending.payload_path, $pendingPath -Force
Write-Host "WINDOWS REBOOT EVIDENCE V2 VERIFIED: $OutputPath"
