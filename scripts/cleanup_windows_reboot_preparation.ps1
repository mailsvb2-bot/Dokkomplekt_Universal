[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $EvidencePath,
    [string] $OutputPath = 'verification/release/REBOOT_PREPARATION_CLEANUP.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-DirectFile {
    param([Parameter(Mandatory = $true)] [string] $Path, [Parameter(Mandatory = $true)] [string] $Label)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw "$Label must be absolute." }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw "$Label must be a direct regular file."
    }
    return $item.FullName
}

function Assert-UnderDokkomplektProgramData {
    param([Parameter(Mandatory = $true)] [string] $Path, [Parameter(Mandatory = $true)] [string] $Label)
    $root = [IO.Path]::GetFullPath((Join-Path $env:ProgramData 'DokkomplektE2E')).TrimEnd('\') + '\'
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label is outside the dedicated DokkomplektE2E ProgramData root."
    }
    return $full
}

$rawEvidence = Resolve-DirectFile -Path $EvidencePath -Label 'Reboot evidence'
$evidence = Get-Content -LiteralPath $rawEvidence -Raw | ConvertFrom-Json
if ($evidence.schema -ne 'dokkomplekt.windows-reboot-e2e.v2') {
    throw 'Unsupported reboot evidence schema.'
}

$appPath = Assert-UnderDokkomplektProgramData -Path ([string] $evidence.application_path) -Label 'Prepared application'
$installRoot = Split-Path -Parent $appPath
$destinationPath = Assert-UnderDokkomplektProgramData -Path ([string] $evidence.destination_path) -Label 'Prepared watcher destination'
$watchRoot = Split-Path -Parent $destinationPath

$stoppedWatcherPids = [System.Collections.Generic.List[int]]::new()
foreach ($process in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) {
    if ([string]::IsNullOrWhiteSpace([string] $process.ExecutablePath)) { continue }
    try { $candidatePath = [IO.Path]::GetFullPath([string] $process.ExecutablePath) } catch { continue }
    if ($candidatePath -ine $appPath) { continue }
    if ([string] $process.CommandLine -notmatch '(?i)(?:^|\s)--background-watch(?:\s|$)') { continue }
    $result = Invoke-CimMethod -InputObject $process -MethodName Terminate -Arguments @{ Reason = 0 } -ErrorAction Stop
    if ([int] $result.ReturnValue -ne 0) { throw "Failed to terminate prepared watcher process $($process.ProcessId)." }
    $stoppedWatcherPids.Add([int] $process.ProcessId)
}

$uninstallerExitCode = $null
if (Test-Path -LiteralPath $installRoot -PathType Container) {
    $uninstallers = @(Get-ChildItem -LiteralPath $installRoot -Recurse -File -Filter '*.exe' -ErrorAction Stop | Where-Object { $_.Name -match '(?i)uninstall' })
    if ($uninstallers.Count -gt 1) { throw "Expected at most one prepared NSIS uninstaller, found $($uninstallers.Count)." }
    if ($uninstallers.Count -eq 1) {
        $uninstaller = Assert-UnderDokkomplektProgramData -Path $uninstallers[0].FullName -Label 'Prepared uninstaller'
        $process = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
        $uninstallerExitCode = $process.ExitCode
        if ($process.ExitCode -ne 0) { throw "Prepared NSIS uninstall failed with exit code $($process.ExitCode)." }
        Start-Sleep -Seconds 2
    }
}

if (Test-Path -LiteralPath $appPath -PathType Leaf) {
    throw 'Prepared application still exists after cleanup.'
}
if (Test-Path -LiteralPath $watchRoot -PathType Container) {
    Remove-Item -LiteralPath $watchRoot -Recurse -Force
}
if (Test-Path -LiteralPath $installRoot -PathType Container) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force
}

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
[ordered]@{
    schema = 'dokkomplekt.windows-reboot-preparation-cleanup.v1'
    cleaned_at_utc = [DateTime]::UtcNow.ToString('o')
    raw_evidence_path = $rawEvidence
    prepared_application_path = $appPath
    prepared_install_root = $installRoot
    prepared_watch_root = $watchRoot
    stopped_watcher_process_ids = $stoppedWatcherPids
    uninstaller_exit_code = $uninstallerExitCode
    application_removed = -not (Test-Path -LiteralPath $appPath -PathType Leaf)
    watch_root_removed = -not (Test-Path -LiteralPath $watchRoot -PathType Container)
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutputPath -Encoding utf8

Write-Host "PERSISTENT REBOOT PREPARATION CLEANED: $installRoot"
