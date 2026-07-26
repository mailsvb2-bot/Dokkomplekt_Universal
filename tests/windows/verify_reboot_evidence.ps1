param(
    [Parameter(Mandatory = $true)] [string] $EvidencePath,
    [Parameter(Mandatory = $true)] [string] $ExpectedSourceSha256,
    [string] $OutputPath = ".release-gate/WINDOWS_REBOOT_E2E_PASSED.json"
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $EvidencePath -PathType Leaf)) { throw 'Reboot evidence file is missing.' }
$evidence = Get-Content -LiteralPath $EvidencePath -Raw | ConvertFrom-Json
if ($evidence.schema -ne 'dokkomplekt.windows-reboot-e2e.v1') { throw 'Unsupported reboot evidence schema.' }
if ($evidence.source_sha256 -ne $ExpectedSourceSha256) { throw 'Reboot evidence belongs to another source tree.' }
if ([string]::IsNullOrWhiteSpace($evidence.boot_id_before) -or [string]::IsNullOrWhiteSpace($evidence.boot_id_after)) { throw 'Both boot identifiers are required.' }
if ($evidence.boot_id_before -eq $evidence.boot_id_after) { throw 'No operating-system reboot was demonstrated.' }
if ($evidence.watcher_started_after_reboot -ne $true) { throw 'Watcher did not start after reboot.' }
if ($evidence.post_reboot_case_completed -ne $true) { throw 'No post-reboot document case completed.' }
if ([string]::IsNullOrWhiteSpace($evidence.post_reboot_output_sha256)) { throw 'Post-reboot output hash is missing.' }
$normalized = [ordered]@{
    schema = 'dokkomplekt.windows-reboot-e2e.verified.v1'
    verified_at_utc = [DateTime]::UtcNow.ToString('o')
    source_sha256 = $ExpectedSourceSha256
    boot_id_before = $evidence.boot_id_before
    boot_id_after = $evidence.boot_id_after
    watcher_started_after_reboot = $true
    post_reboot_case_completed = $true
    post_reboot_output_sha256 = $evidence.post_reboot_output_sha256
}
$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$normalized | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Host "WINDOWS REBOOT EVIDENCE VERIFIED: $OutputPath"
