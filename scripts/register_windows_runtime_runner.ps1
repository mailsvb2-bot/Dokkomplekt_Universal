[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$secureToken = Read-Host 'GitHub self-hosted runner registration token' -AsSecureString
if ($secureToken.Length -eq 0) { throw 'GitHub runner registration token is required.' }
$credential = [Net.NetworkCredential]::new('', $secureToken)
$plainToken = $credential.Password
try {
    $bootstrap = Join-Path $PSScriptRoot 'bootstrap_private_windows_runner.ps1'
    & $bootstrap `
      -Role runtime `
      -RepositoryUrl 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation' `
      -RegistrationToken $plainToken `
      -SidecarManifestPath $SidecarManifestPath `
      -InstallPrerequisites:$InstallPrerequisites
    if ($LASTEXITCODE -notin @(0, $null)) { throw "Runtime runner bootstrap failed with exit code $LASTEXITCODE." }
} finally {
    $plainToken = $null
    $credential = $null
    $secureToken.Dispose()
}
