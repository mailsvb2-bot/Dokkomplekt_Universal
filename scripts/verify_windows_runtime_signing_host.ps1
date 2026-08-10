[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RunnerRoot = 'C:\actions-runner-runtime',
    [string] $OutputPath = 'verification/release/RUNTIME_RUNNER_HOST.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$checks = [System.Collections.Generic.List[object]]::new()
$failures = [System.Collections.Generic.List[string]]::new()

function Add-Check {
    param([string] $Name, [bool] $Ok, [string] $Detail = '')
    $checks.Add([ordered]@{ name = $Name; ok = $Ok; detail = $Detail })
    if (-not $Ok) { $failures.Add("${Name}: ${Detail}") }
}

function Get-MachineFingerprint {
    $machineGuid = [string] (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid -ErrorAction Stop).MachineGuid
    if ([string]::IsNullOrWhiteSpace($machineGuid)) { throw 'Windows MachineGuid is unavailable.' }
    $bytes = [Text.Encoding]::UTF8.GetBytes($machineGuid.Trim().ToLowerInvariant())
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = $sha.ComputeHash($bytes) } finally { $sha.Dispose() }
    return ([BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
}

function Get-VcBuildToolsInstallation {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return '' }
    return [string] (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
}

if (-not [Environment]::Is64BitOperatingSystem) { Add-Check 'windows-x64' $false '64-bit Windows is required' } else { Add-Check 'windows-x64' $true '64-bit OS' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sessionId = (Get-Process -Id $PID).SessionId
$services = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
$serviceMode = $services.Count -gt 0 -and $sessionId -eq 0
Add-Check 'actions-runner-service-mode' $serviceMode ("services={0}; session_id={1}; user={2}" -f $services.Count, $sessionId, $identity.Name)

$runnerConfig = Join-Path $RunnerRoot '.runner'
Add-Check 'runtime-runner-config-present' (Test-Path -LiteralPath $runnerConfig -PathType Leaf) $runnerConfig

foreach ($required in @('git.exe', 'pwsh.exe', 'openssl.exe')) {
    $command = Get-Command $required -ErrorAction SilentlyContinue
    Add-Check ("tool-" + $required) ($null -ne $command) (if ($null -eq $command) { 'missing' } else { $command.Source })
}
$buildTools = Get-VcBuildToolsInstallation
Add-Check 'visual-studio-vctools' (-not [string]::IsNullOrWhiteSpace($buildTools)) $buildTools

$manifestOk = $false
$manifestDetail = ''
$manifestSha = ''
try {
    if (-not [IO.Path]::IsPathFullyQualified($SidecarManifestPath)) { throw 'manifest path must be absolute' }
    $item = Get-Item -LiteralPath $SidecarManifestPath -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'manifest must be a direct regular file' }
    $manifest = Get-Content -LiteralPath $item.FullName -Raw | ConvertFrom-Json
    if ([int] $manifest.schema -ne 1) { throw 'schema must be 1' }
    if ([string] $manifest.target -ne 'windows-x86_64') { throw 'target must be windows-x86_64' }
    if ($manifest.supply_chain_locked -ne $true) { throw 'supply_chain_locked must be true' }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) { throw 'files must be non-empty' }
    $signaturePath = "$($item.FullName).sig"
    if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) { throw 'offline approval signature is missing beside runtime manifest' }
    $manifestSha = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestDetail = "path=$($item.FullName); files=$(@($manifest.files).Count); sha256=$manifestSha"
    $manifestOk = $true
} catch {
    $manifestDetail = $_.Exception.Message
}
Add-Check 'runner-owned-approved-runtime-manifest' $manifestOk $manifestDetail

$hardwareOnlyVars = @(
    'DOKKOMPLEKT_TEST_PRINTER',
    'DOKKOMPLEKT_TEST_DUPLEX',
    'DOKKOMPLEKT_TEST_TRAY',
    'DOKKOMPLEKT_REBOOT_EVIDENCE_PATH',
    'DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT'
)
$exposedHardwareVars = @($hardwareOnlyVars | Where-Object { -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_, 'Process')) })
Add-Check 'hardware-only-environment-not-exposed' ($exposedHardwareVars.Count -eq 0) (($exposedHardwareVars -join ', '))

$fingerprint = ''
try { $fingerprint = Get-MachineFingerprint } catch { Add-Check 'machine-fingerprint' $false $_.Exception.Message }
if (-not [string]::IsNullOrWhiteSpace($fingerprint)) { Add-Check 'machine-fingerprint' ($fingerprint -match '^[0-9a-f]{64}$') $fingerprint }

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$report = [ordered]@{
    schema = 'dokkomplekt.runtime-signing-host-preflight.v1'
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    computer = $env:COMPUTERNAME
    machine_fingerprint_sha256 = $fingerprint
    runner_name = $env:RUNNER_NAME
    user = $identity.Name
    session_id = $sessionId
    service_mode_required = $true
    runtime_manifest_sha256 = $manifestSha
    hardware_only_environment_exposed = @($exposedHardwareVars)
    ok = $failures.Count -eq 0
    checks = $checks
    failures = $failures
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8

if ($failures.Count -gt 0) {
    Write-Error ("RUNTIME/SIGNING HOST PREFLIGHT FAILED:`n - " + ($failures -join "`n - "))
    exit 1
}
Write-Host "RUNTIME/SIGNING HOST PREFLIGHT PASSED: host=$fingerprint; runner=$env:RUNNER_NAME"
