[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RunnerRoot = 'C:\actions-runner-runtime',
    [string] $RunnerName = '',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$expectedRepository = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation'
$normalized = $RepositoryUrl.Trim().TrimEnd('/')
if ($normalized -ine $expectedRepository) {
    throw "Runtime/signing runner must be registered only in $expectedRepository"
}
$secureToken = Read-Host 'GitHub self-hosted runner registration token' -AsSecureString
if ($secureToken.Length -eq 0) { throw 'GitHub runner registration token is required.' }
$credential = [Net.NetworkCredential]::new('', $secureToken)
$plainToken = $credential.Password
try {
    if ($plainToken -notmatch '^[A-Za-z0-9_-]{20,}$') { throw 'The supplied value does not look like a GitHub runner registration token.' }
    $bootstrap = Join-Path $PSScriptRoot 'bootstrap_windows_runtime_runner.ps1'
    if (-not (Test-Path -LiteralPath $bootstrap -PathType Leaf)) { throw "Runtime runner bootstrap is missing: $bootstrap" }
    & $bootstrap `
        -RegistrationToken $plainToken `
        -RepositoryUrl $normalized `
        -SidecarManifestPath $SidecarManifestPath `
        -RunnerRoot $RunnerRoot `
        -RunnerName $RunnerName `
        -InstallPrerequisites:$InstallPrerequisites
    if ($LASTEXITCODE -notin @(0, $null)) { throw "Runtime runner bootstrap failed with exit code $LASTEXITCODE." }
} finally {
    $plainToken = $null
    $credential = $null
    $secureToken.Dispose()
}
