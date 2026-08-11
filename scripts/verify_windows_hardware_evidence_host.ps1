[CmdletBinding()]
param(
    [string] $PrinterName = '',
    [string] $RebootEvidencePath = '',
    [string] $RunnerRoot = 'C:\actions-runner-hardware',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [string] $LocalConfigPath = 'C:\ProgramData\DokkomplektE2E\hardware-runner.json',
    [string] $OutputPath = 'verification/release/HARDWARE_RUNNER_HOST.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$checks = [System.Collections.Generic.List[object]]::new()
$failures = [System.Collections.Generic.List[string]]::new()

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
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return '' }
    return [string] (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
}

function Export-HardwareEnvironmentValue {
    param(
        [Parameter(Mandatory = $true)] [string] $Name,
        [string] $Value = ''
    )
    if ([string]::IsNullOrWhiteSpace($Value)) { return }
    if ($Value.Contains("`r") -or $Value.Contains("`n")) { throw "Local hardware configuration contains a multiline value for $Name." }
    [Environment]::SetEnvironmentVariable($Name, $Value, 'Process')
    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
        Add-Content -LiteralPath $env:GITHUB_ENV -Value ("$Name=$Value") -Encoding utf8
    }
}

$localConfigLoaded = $false
$localConfigMachine = ''
if (Test-Path -LiteralPath $LocalConfigPath -PathType Leaf) {
    $localConfig = Get-Content -LiteralPath $LocalConfigPath -Raw | ConvertFrom-Json
    if ([string]$localConfig.schema -ne 'dokkomplekt.hardware-runner-local-config.v1') {
        throw "Unsupported local hardware config schema: $($localConfig.schema)"
    }
    $localConfigMachine = [string]$localConfig.computer
    if (-not [string]::IsNullOrWhiteSpace($localConfigMachine) -and $localConfigMachine -ine $env:COMPUTERNAME) {
        throw "Local hardware config belongs to '$localConfigMachine', not '$env:COMPUTERNAME'."
    }
    if ([string]::IsNullOrWhiteSpace($PrinterName)) { $PrinterName = [string]$localConfig.printer_name }
    if ([string]::IsNullOrWhiteSpace($RebootEvidencePath)) { $RebootEvidencePath = [string]$localConfig.reboot_evidence_path }
    Export-HardwareEnvironmentValue -Name 'DOKKOMPLEKT_TEST_PRINTER' -Value ([string]$localConfig.printer_name)
    Export-HardwareEnvironmentValue -Name 'DOKKOMPLEKT_TEST_DUPLEX' -Value ([string]$localConfig.test_duplex)
    Export-HardwareEnvironmentValue -Name 'DOKKOMPLEKT_TEST_TRAY' -Value ([string]$localConfig.test_tray)
    Export-HardwareEnvironmentValue -Name 'DOKKOMPLEKT_REBOOT_EVIDENCE_PATH' -Value ([string]$localConfig.reboot_evidence_path)
    Export-HardwareEnvironmentValue -Name 'DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT' -Value ([string]$localConfig.reboot_source_document)
    $localConfigLoaded = $true
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
Add-Check -Name 'administrator' -Ok $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) -Detail "user=$($identity.Name)"

$sessionId = (Get-Process -Id $PID).SessionId
$interactive = (-not $identity.IsSystem) -and $sessionId -ne 0
Add-Check -Name 'interactive-user-session' -Ok $interactive -Detail "user=$($identity.Name); session_id=$sessionId"

$runnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
Add-Check -Name 'actions-runner-not-service' -Ok ($runnerServices.Count -eq 0) -Detail (($runnerServices | ForEach-Object { "$($_.Name):$($_.Status)" }) -join ', ')

$task = Get-ScheduledTask -TaskName $RunnerTaskName -ErrorAction SilentlyContinue
Add-Check -Name 'interactive-runner-scheduled-task' -Ok ($null -ne $task) -Detail (if ($null -eq $task) { 'missing' } else { "state=$($task.State)" })

$runnerConfig = Join-Path $RunnerRoot '.runner'
Add-Check -Name 'runner-config-present' -Ok (Test-Path -LiteralPath $runnerConfig -PathType Leaf) -Detail $runnerConfig

$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sessionId })
Add-Check -Name 'runner-listener-interactive' -Ok ($listener.Count -gt 0) -Detail "count=$($listener.Count); session_id=$sessionId"

$buildTools = Get-VcBuildToolsInstallation
Add-Check -Name 'visual-studio-vctools' -Ok (-not [string]::IsNullOrWhiteSpace($buildTools)) -Detail $buildTools

$webViewCandidates = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
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
# PrintService configuration. Loading a local non-secret config file is allowed;
# the real hardware probes execute only after signed payload verification.
$virtualPrinterPattern = '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)'
$printerNameConfigured = (-not [string]::IsNullOrWhiteSpace($PrinterName)) -and ($PrinterName -notmatch $virtualPrinterPattern)
Add-Check -Name 'dedicated-printer-name-configured' -Ok $printerNameConfigured -Detail (if ($printerNameConfigured) { $PrinterName } else { 'missing or forbidden virtual/document printer name' })
Add-Check -Name 'hardware-probes-deferred-until-signed-handoff' -Ok $true -Detail 'Word COM, Get-Printer and PrintService probes are intentionally deferred to windows_hardware_e2e.ps1 after signed payload verification'
Add-Check -Name 'local-hardware-config-loaded' -Ok $localConfigLoaded -Detail (if ($localConfigLoaded) { $LocalConfigPath } else { 'missing local hardware-runner.json; run SETUP_HARDWARE_RUNNER.cmd' })

$rebootPathOk = (-not [string]::IsNullOrWhiteSpace($RebootEvidencePath)) -and [IO.Path]::IsPathFullyQualified($RebootEvidencePath)
Add-Check -Name 'reboot-evidence-path-absolute' -Ok $rebootPathOk -Detail (if ($rebootPathOk) { $RebootEvidencePath } else { 'missing or non-absolute reboot evidence path' })

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
    local_config_path = $LocalConfigPath
    local_config_loaded = $localConfigLoaded
    local_config_machine = $localConfigMachine
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
Write-Host "HARDWARE EVIDENCE HOST PREFLIGHT PASSED: user=$($identity.Name); session=$sessionId; local config loaded; hardware probes deferred until signed handoff verification"
