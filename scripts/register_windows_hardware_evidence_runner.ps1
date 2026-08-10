[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [string] $RunnerRoot = 'C:\actions-runner-hardware',
    [string] $RunnerName = '',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$expectedRepository = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation'
$normalized = $RepositoryUrl.Trim().TrimEnd('/')
if ($normalized -ine $expectedRepository) { throw "Hardware evidence runner must be registered only in $expectedRepository" }
$secureToken = Read-Host 'GitHub self-hosted runner registration token' -AsSecureString
if ($secureToken.Length -eq 0) { throw 'GitHub runner registration token is required.' }
$credential = [Net.NetworkCredential]::new('', $secureToken)
$plainToken = $credential.Password
try {
    if ($plainToken -notmatch '^[A-Za-z0-9_-]{20,}$') { throw 'The supplied value does not look like a GitHub runner registration token.' }
    $bootstrap = Join-Path $PSScriptRoot 'bootstrap_windows_hardware_evidence_runner.ps1'
    if (-not (Test-Path -LiteralPath $bootstrap -PathType Leaf)) { throw "Hardware evidence runner bootstrap is missing: $bootstrap" }
    & $bootstrap `
        -RegistrationToken $plainToken `
        -RepositoryUrl $normalized `
        -PrinterName $PrinterName `
        -RunnerRoot $RunnerRoot `
        -RunnerName $RunnerName `
        -RunnerTaskName $RunnerTaskName `
        -InstallPrerequisites:$InstallPrerequisites
    if ($LASTEXITCODE -notin @(0, $null)) { throw "Hardware evidence runner bootstrap failed with exit code $LASTEXITCODE." }
} finally {
    $plainToken = $null
    $credential = $null
    $secureToken.Dispose()
}
