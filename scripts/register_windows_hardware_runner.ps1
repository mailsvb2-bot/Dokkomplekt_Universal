[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RepositoryUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Universal',
    [string] $RunnerRoot = 'C:\actions-runner',
    [string] $RunnerName = '',
    [string] $RunnerLabel = 'dokkomplekt-hardware-e2e',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$secureToken = Read-Host 'GitHub self-hosted runner registration token' -AsSecureString
if ($secureToken.Length -eq 0) {
    throw 'GitHub runner registration token is required.'
}

$credential = [Net.NetworkCredential]::new('', $secureToken)
$plainToken = $credential.Password
try {
    if ($plainToken -notmatch '^[A-Za-z0-9_-]{20,}$') {
        throw 'The supplied value does not look like a GitHub runner registration token.'
    }

    $bootstrap = Join-Path $PSScriptRoot 'bootstrap_windows_hardware_runner.ps1'
    if (-not (Test-Path -LiteralPath $bootstrap -PathType Leaf)) {
        throw "Hardware runner bootstrap is missing: $bootstrap"
    }

    $arguments = @{
        RegistrationToken = $plainToken
        PrinterName = $PrinterName
        SidecarManifestPath = $SidecarManifestPath
        RepositoryUrl = $RepositoryUrl
        RunnerRoot = $RunnerRoot
        RunnerName = $RunnerName
        RunnerLabel = $RunnerLabel
        RunnerTaskName = $RunnerTaskName
        InstallPrerequisites = $InstallPrerequisites
    }
    & $bootstrap @arguments
    if ($LASTEXITCODE -notin @(0, $null)) {
        throw "Hardware runner bootstrap failed with exit code $LASTEXITCODE."
    }
} finally {
    $plainToken = $null
    $credential = $null
    $secureToken.Dispose()
}
