[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $SpecPath,
    [string] $OutputDir = 'C:\DokkomplektRuntime\locked',
    [string] $Python = 'python'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-DirectFile {
    param([Parameter(Mandatory = $true)] [string] $Path, [Parameter(Mandatory = $true)] [string] $Label)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw "$Label must be an absolute path." }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw "$Label must be a direct regular file, not a directory or reparse point."
    }
    return $item.FullName
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)] [string] $Executable,
        [Parameter(Mandatory = $true)] [string[]] $Arguments,
        [Parameter(Mandatory = $true)] [string] $Label
    )
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE." }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $repoRoot

$resolvedSpec = Resolve-DirectFile -Path $SpecPath -Label 'Runtime-kit specification'
if (-not [IO.Path]::IsPathFullyQualified($OutputDir)) { throw 'OutputDir must be an absolute path.' }
$resolvedOutput = [IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null
$outputItem = Get-Item -LiteralPath $resolvedOutput -Force -ErrorAction Stop
if (($outputItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'OutputDir must not be a reparse point.'
}

$pythonCommand = Get-Command $Python -ErrorAction Stop
$pythonExe = $pythonCommand.Source

Invoke-Checked -Executable $pythonExe -Arguments @(
    'scripts/build_windows_runtime_kit.py',
    $resolvedSpec,
    '--output-dir',
    $resolvedOutput
) -Label 'Production runtime lock build'

$manifest = Resolve-DirectFile -Path (Join-Path $resolvedOutput 'windows-x86_64-manifest.json') -Label 'Generated runtime manifest'

Invoke-Checked -Executable $pythonExe -Arguments @(
    'scripts/prepare_sidecars.py',
    $manifest,
    '--clean'
) -Label 'Verified sidecar staging'

Invoke-Checked -Executable $pythonExe -Arguments @(
    'scripts/assert_offline_runtime_ready.py',
    '--target',
    'windows-x86_64',
    '--require-semantic-model',
    '--require-supply-chain',
    '--production'
) -Label 'Production offline runtime verification'

$manifestHash = (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
$report = Join-Path $resolvedOutput 'RUNTIME_KIT_REPORT.json'
$reportHash = if (Test-Path -LiteralPath $report -PathType Leaf) {
    (Get-FileHash -LiteralPath $report -Algorithm SHA256).Hash.ToLowerInvariant()
} else {
    throw 'RUNTIME_KIT_REPORT.json is missing after successful preparation.'
}
$signature = "$manifest.sig"
if (Test-Path -LiteralPath $signature -PathType Leaf) {
    Remove-Item -LiteralPath $signature -Force
    Write-Warning 'Removed a stale runtime-lock approval signature because the manifest was rebuilt. A fresh offline approval is required.'
}

Write-Host 'DOKKOMPLEKT WINDOWS PRODUCTION RUNTIME KIT VERIFIED'
Write-Host "Manifest: $manifest"
Write-Host "Manifest SHA-256: $manifestHash"
Write-Host "Report: $report"
Write-Host "Report SHA-256: $reportHash"
Write-Host 'NEXT REQUIRED RELEASE STEP: obtain a fresh offline Ed25519 approval signature for this exact manifest.'
Write-Host "Expected detached signature path on the runner: $signature"
Write-Host 'Hardware validation will fail closed until the signature verifies against DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64.'
