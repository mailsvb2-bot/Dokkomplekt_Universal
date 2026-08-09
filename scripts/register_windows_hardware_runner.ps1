[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RunnerRoot = 'C:\actions-runner',
    [string] $RunnerName = '',
    [string] $RunnerLabel = 'dokkomplekt-hardware-e2e',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$publicSourceRepository = 'https://github.com/mailsvb2-bot/Dokkomplekt_Universal'
$normalizedRepositoryUrl = $RepositoryUrl.Trim().TrimEnd('/')
if ($normalizedRepositoryUrl -ieq $publicSourceRepository) {
    throw 'Refusing to register a persistent hardware runner in the public Dokkomplekt_Universal repository. Use the dedicated private hardware-validation repository.'
}
if ($normalizedRepositoryUrl -notmatch '^https://github\.com/[^/]+/[^/]+$') {
    throw 'RepositoryUrl must be a canonical GitHub repository URL such as https://github.com/owner/private-hardware-validation.'
}

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
        RepositoryUrl = $normalizedRepositoryUrl
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
