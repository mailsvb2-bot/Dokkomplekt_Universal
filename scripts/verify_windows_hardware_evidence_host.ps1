[CmdletBinding()]
param(
    [string] $PrinterName = '',
    [string] $RebootEvidencePath = '',
    [string] $RunnerRoot = '',
    [string] $OutputPath = 'verification/release/HARDWARE_RUNNER_HOST.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$checks = [System.Collections.Generic.List[object]]::new()
$failures = [System.Collections.Generic.List[string]]::new()
$importedHardwareConfig = ''

function Add-Check {
    param(
        [Parameter(Mandatory = $true)] [string] $Name,
        [Parameter(Mandatory = $true)] [bool] $Ok,
        [string] $Detail = ''
    )
    $checks.Add([ordered]@{ name = $Name; ok = $Ok; detail = $Detail })
    if (-not $Ok) { $failures.Add("${Name}: ${Detail}") }
}

function Get-VcBuildToolsInstallation {
    $programFilesX86 = ${env:ProgramFiles(x86)}
    if ([string]::IsNullOrWhiteSpace($programFilesX86)) { return '' }
    $vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return '' }
    return [string] (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
}

function Resolve-HardwareRunnerRoot {
    param([string] $RequestedRoot)
    if (-not [string]::IsNullOrWhiteSpace($RequestedRoot)) { return $RequestedRoot }

    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'DokkomplektHardwareRunner'),
        'C:\actions-runner-hardware'
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath (Join-Path $candidate '.runner') -PathType Leaf) { return $candidate }
    }
    return $candidates[0]
}

function Publish-GitHubEnvironmentValue {
    param(
        [Parameter(Mandatory = $true)] [string] $Name,
        [AllowEmptyString()] [string] $Value
    )
    [Environment]::SetEnvironmentVariable($Name, $Value, 'Process')
    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
        Add-Content -LiteralPath $env:GITHUB_ENV -Value ("{0}={1}" -f $Name, $Value) -Encoding utf8
    }
}

function Import-LocalHardwareConfiguration {
    param([Parameter(Mandatory = $true)] [string] $ResolvedRunnerRoot)

    $configPath = Join-Path $ResolvedRunnerRoot 'hardware-config.cmd'
    if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) { return '' }

    $allowedNames = @(
        'DOKKOMPLEKT_TEST_PRINTER',
        'DOKKOMPLEKT_TEST_DUPLEX',
        'DOKKOMPLEKT_TEST_TRAY',
        'DOKKOMPLEKT_REBOOT_EVIDENCE_PATH',
        'DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT',
        'DOKKOMPLEKT_WORD_PATH'
    )
    foreach ($line in Get-Content -LiteralPath $configPath) {
        if ($line -notmatch '^\s*set\s+"(?<name>DOKKOMPLEKT_[A-Z0-9_]+)=(?<value>.*)"\s*$') { continue }
        $name = $Matches.name
        if ($allowedNames -notcontains $name) { continue }
        $existing = [Environment]::GetEnvironmentVariable($name, 'Process')
        if ([string]::IsNullOrWhiteSpace($existing)) {
            Publish-GitHubEnvironmentValue -Name $name -Value $Matches.value
        }
    }
    return $configPath
}

$RunnerRoot = Resolve-HardwareRunnerRoot -RequestedRoot $RunnerRoot
$importedHardwareConfig = Import-LocalHardwareConfiguration -ResolvedRunnerRoot $RunnerRoot

if ([string]::IsNullOrWhiteSpace($PrinterName)) {
    $PrinterName = [Environment]::GetEnvironmentVariable('DOKKOMPLEKT_TEST_PRINTER', 'Process')
}
if ([string]::IsNullOrWhiteSpace($RebootEvidencePath)) {
    $RebootEvidencePath = [Environment]::GetEnvironmentVariable('DOKKOMPLEKT_REBOOT_EVIDENCE_PATH', 'Process')
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sessionId = (Get-Process -Id $PID).SessionId
$interactive = (-not $identity.IsSystem) -and $sessionId -ne 0
Add-Check -Name 'interactive-user-session' -Ok $interactive -Detail "user=$($identity.Name); session_id=$sessionId"
Add-Check -Name 'not-local-system' -Ok (-not $identity.IsSystem) -Detail "user=$($identity.Name)"

$runnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
Add-Check -Name 'actions-runner-not-service' -Ok ($runnerServices.Count -eq 0) -Detail (($runnerServices | ForEach-Object { "$($_.Name):$($_.Status)" }) -join ', ')

$startupCmd = Join-Path ([Environment]::GetFolderPath('Startup')) 'DokkomplektHardwareRunner.cmd'
Add-Check -Name 'interactive-runner-logon-autostart' -Ok (Test-Path -LiteralPath $startupCmd -PathType Leaf) -Detail $startupCmd

$runnerConfig = Join-Path $RunnerRoot '.runner'
Add-Check -Name 'runner-config-present' -Ok (Test-Path -LiteralPath $runnerConfig -PathType Leaf) -Detail $runnerConfig

$localConfig = Join-Path $RunnerRoot 'hardware-config.cmd'
Add-Check -Name 'hardware-local-config-present' -Ok (Test-Path -LiteralPath $localConfig -PathType Leaf) -Detail $localConfig

$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sessionId })
Add-Check -Name 'runner-listener-interactive' -Ok ($listener.Count -gt 0) -Detail "count=$($listener.Count); session_id=$sessionId"

$buildTools = Get-VcBuildToolsInstallation
Add-Check -Name 'visual-studio-vctools' -Ok (-not [string]::IsNullOrWhiteSpace($buildTools)) -Detail $buildTools

$programFilesX86 = ${env:ProgramFiles(x86)}
$webViewCandidates = if ([string]::IsNullOrWhiteSpace($programFilesX86)) { @() } else {
    @(Get-ChildItem (Join-Path $programFilesX86 'Microsoft\EdgeWebView\Application') -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
}
Add-Check -Name 'webview2-runtime' -Ok ($webViewCandidates.Count -gt 0) -Detail (if ($webViewCandidates.Count -gt 0) { $webViewCandidates[0].FullName } else { 'missing' })

$runtimeManifestEnv = [Environment]::GetEnvironmentVariable('DOKKOMPLEKT_SIDECAR_MANIFEST_PATH', 'Process')
$runtimeManifestNotExposed = [string]::IsNullOrWhiteSpace($runtimeManifestEnv)
Add-Check -Name 'runtime-manifest-not-exposed' -Ok $runtimeManifestNotExposed -Detail (if ($runtimeManifestNotExposed) { 'DOKKOMPLEKT_SIDECAR_MANIFEST_PATH is absent from the hardware process' } else { 'DOKKOMPLEKT_SIDECAR_MANIFEST_PATH must not be exposed to the hardware trust domain' })

$signingSecretNames = @(
    'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64',
    'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD',
    'DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64',
    'DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64',
    'DOKKOMPLEKT_GATE_PRIVATE_KEY_B64'
)
$exposedSigningSecrets = @($signingSecretNames | Where-Object {
    -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_, 'Process'))
})
Add-Check -Name 'signing-secrets-not-exposed' -Ok ($exposedSigningSecrets.Count -eq 0) -Detail (if ($exposedSigningSecrets.Count -eq 0) { 'no signing/private-key environment variables are exposed' } else { 'forbidden variables: ' + ($exposedSigningSecrets -join ', ') })

# This preflight runs before the downloaded release handoff is trusted. It must
# therefore remain side-effect-free with respect to Word, printer devices and
# PrintService configuration. The real hardware probes execute only from
# tests/windows/windows_hardware_e2e.ps1 after signed handoff, Authenticode and
# runtime-signature verification have succeeded.
$virtualPrinterPattern = '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF|AnyDesk Printer)'
$printerNameConfigured = (-not [string]::IsNullOrWhiteSpace($PrinterName)) -and ($PrinterName -notmatch $virtualPrinterPattern)
Add-Check -Name 'dedicated-printer-name-configured' -Ok $printerNameConfigured -Detail (if ($printerNameConfigured) { $PrinterName } else { 'missing or forbidden virtual/document printer name' })
Add-Check -Name 'hardware-probes-deferred-until-signed-handoff' -Ok $true -Detail 'Word COM, Get-Printer and PrintService probes are intentionally deferred to windows_hardware_e2e.ps1 after signed payload verification'

$rebootPathOk = $true
$rebootPathDetail = 'not supplied'
if (-not [string]::IsNullOrWhiteSpace($RebootEvidencePath)) {
    $rebootPathOk = [IO.Path]::IsPathFullyQualified($RebootEvidencePath)
    $rebootPathDetail = $RebootEvidencePath
}
Add-Check -Name 'reboot-evidence-path-absolute' -Ok $rebootPathOk -Detail $rebootPathDetail

$rebootSourceDocument = [Environment]::GetEnvironmentVariable('DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT', 'Process')
$rebootSourceOk = (-not [string]::IsNullOrWhiteSpace($rebootSourceDocument)) -and (Test-Path -LiteralPath $rebootSourceDocument -PathType Leaf) -and ([IO.Path]::GetExtension($rebootSourceDocument) -ieq '.docx')
Add-Check -Name 'reboot-source-docx-configured' -Ok $rebootSourceOk -Detail (if ([string]::IsNullOrWhiteSpace($rebootSourceDocument)) { 'missing' } else { $rebootSourceDocument })

$powerState = (& powercfg /getactivescheme 2>$null) -join ' '
Add-Check -Name 'power-plan-readable' -Ok (-not [string]::IsNullOrWhiteSpace($powerState)) -Detail $powerState

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$report = [ordered]@{
    schema = 'dokkomplekt.hardware-evidence-host-preflight.v3'
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    computer = $env:COMPUTERNAME
    user = $identity.Name
    session_id = $sessionId
    runner_root = $RunnerRoot
    local_hardware_config = $importedHardwareConfig
    runtime_manifest_env_exposed = -not $runtimeManifestNotExposed
    signing_secret_env_exposed = $exposedSigningSecrets
    hardware_probes_deferred_until_signed_handoff = $true
    ok = $failures.Count -eq 0
    checks = $checks
    failures = $failures
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8

if ($failures.Count -gt 0) {
    Write-Error ("HARDWARE EVIDENCE HOST PREFLIGHT FAILED:`n - " + ($failures -join "`n - "))
    exit 1
}
Write-Host "HARDWARE EVIDENCE HOST PREFLIGHT PASSED: user=$($identity.Name); session=$sessionId; runner=$RunnerRoot; local_config=$importedHardwareConfig"
