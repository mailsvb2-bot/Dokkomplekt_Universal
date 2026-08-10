[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RuntimeRoot = 'C:\ProgramData\DokkomplektRuntime',
    [string] $RunnerRoot = 'C:\actions-runner-runtime',
    [string] $OutputPath = 'verification/release/RUNTIME_RUNNER_HOST.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$ServiceIdentity = 'NT AUTHORITY\NETWORK SERVICE'
$AclEvidencePath = 'C:\ProgramData\DokkomplektE2E\RUNTIME_SERVICE_ACL.json'
$checks = [System.Collections.Generic.List[object]]::new()
$failures = [System.Collections.Generic.List[string]]::new()

function Add-Check {
    param([string] $Name, [bool] $Ok, [string] $Detail = '')
    $checks.Add([ordered]@{ name=$Name; ok=$Ok; detail=$Detail })
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

function Is-UnderRoot([string] $Path, [string] $Root) {
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $base = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    return $full -ieq $base -or $full.StartsWith($base + '\', [StringComparison]::OrdinalIgnoreCase)
}

if (-not [Environment]::Is64BitOperatingSystem) { Add-Check 'windows-x64' $false '64-bit Windows is required' } else { Add-Check 'windows-x64' $true '64-bit OS' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sessionId = (Get-Process -Id $PID).SessionId
$services = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
$serviceMode = $services.Count -gt 0 -and $sessionId -eq 0
Add-Check 'actions-runner-service-mode' $serviceMode ("services={0}; session_id={1}; user={2}" -f $services.Count, $sessionId, $identity.Name)
Add-Check 'runtime-service-identity' ($identity.Name -ieq $ServiceIdentity) ("expected=$ServiceIdentity; actual=$($identity.Name)")

$runnerConfig = Join-Path $RunnerRoot '.runner'
Add-Check 'runtime-runner-config-present' (Test-Path -LiteralPath $runnerConfig -PathType Leaf) $runnerConfig
foreach ($required in @('git.exe','pwsh.exe','openssl.exe')) {
    $command = Get-Command $required -ErrorAction SilentlyContinue
    Add-Check ("tool-" + $required) ($null -ne $command) (if ($null -eq $command) { 'missing' } else { $command.Source })
}
$buildTools = Get-VcBuildToolsInstallation
Add-Check 'visual-studio-vctools' (-not [string]::IsNullOrWhiteSpace($buildTools)) $buildTools

$rootOk = $false
$rootDetail = ''
try {
    $rootItem = Get-Item -LiteralPath $RuntimeRoot -Force -ErrorAction Stop
    if (-not $rootItem.PSIsContainer -or (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'runtime root must be a direct directory' }
    $rootOk = $true
    $rootDetail = $rootItem.FullName
} catch { $rootDetail = $_.Exception.Message }
Add-Check 'bounded-runtime-root' $rootOk $rootDetail

$manifestOk = $false
$manifestDetail = ''
$manifestSha = ''
$manifestFull = ''
try {
    if (-not [IO.Path]::IsPathFullyQualified($SidecarManifestPath)) { throw 'manifest path must be absolute' }
    $item = Get-Item -LiteralPath $SidecarManifestPath -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'manifest must be a direct regular file' }
    if (-not (Is-UnderRoot $item.FullName $RuntimeRoot)) { throw 'manifest is outside bounded runtime root' }
    $manifest = Get-Content -LiteralPath $item.FullName -Raw | ConvertFrom-Json
    if ([int]$manifest.schema -ne 1) { throw 'schema must be 1' }
    if ([string]$manifest.target -ne 'windows-x86_64') { throw 'target must be windows-x86_64' }
    if ($manifest.supply_chain_locked -ne $true) { throw 'supply_chain_locked must be true' }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) { throw 'files must be non-empty' }
    $signaturePath = "$($item.FullName).sig"
    if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) { throw 'offline approval signature is missing beside runtime manifest' }
    foreach ($entry in @($manifest.files)) {
        foreach ($field in @('source','license_file')) {
            $path = [string]$entry.$field
            if ([string]::IsNullOrWhiteSpace($path) -or -not [IO.Path]::IsPathFullyQualified($path)) { throw "manifest $field must be an absolute path" }
            if (-not (Is-UnderRoot $path $RuntimeRoot)) { throw "manifest $field escapes bounded runtime root" }
            $sourceItem = Get-Item -LiteralPath $path -Force -ErrorAction Stop
            if ($sourceItem.PSIsContainer -or (($sourceItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "manifest $field must be a direct regular file" }
        }
    }
    $inventory = [string]$manifest.distribution_review.inventory_file
    if ([string]::IsNullOrWhiteSpace($inventory) -or -not [IO.Path]::IsPathFullyQualified($inventory) -or -not (Is-UnderRoot $inventory $RuntimeRoot)) { throw 'distribution inventory must remain inside bounded runtime root' }
    $inventoryItem = Get-Item -LiteralPath $inventory -Force -ErrorAction Stop
    if ($inventoryItem.PSIsContainer -or (($inventoryItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'distribution inventory must be a direct regular file' }
    $manifestSha = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestFull = $item.FullName
    $manifestDetail = "path=$($item.FullName); files=$(@($manifest.files).Count); sha256=$manifestSha"
    $manifestOk = $true
} catch { $manifestDetail = $_.Exception.Message }
Add-Check 'runner-owned-approved-runtime-manifest' $manifestOk $manifestDetail

$aclEvidenceOk = $false
$aclEvidenceDetail = ''
try {
    $record = Get-Content -LiteralPath $AclEvidencePath -Raw -ErrorAction Stop | ConvertFrom-Json
    if ([string]$record.schema -ne 'dokkomplekt.runtime-service-acl.v1') { throw 'ACL evidence schema mismatch' }
    if ([IO.Path]::GetFullPath([string]$record.runtime_root).TrimEnd('\') -ine [IO.Path]::GetFullPath($RuntimeRoot).TrimEnd('\')) { throw 'ACL evidence runtime root mismatch' }
    if ($manifestOk -and [IO.Path]::GetFullPath([string]$record.manifest_path) -ine [IO.Path]::GetFullPath($manifestFull)) { throw 'ACL evidence manifest mismatch' }
    if ([string]$record.service_identity -ine $ServiceIdentity -or [string]$record.access -ne 'ReadAndExecute' -or $record.recursive_acl_applied -ne $true) { throw 'ACL evidence access mismatch' }
    $acl = Get-Acl -LiteralPath $RuntimeRoot
    $rules = @($acl.Access | Where-Object {
        $_.IdentityReference.Value -ieq $ServiceIdentity -and
        ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::ReadAndExecute)
    })
    if ($rules.Count -eq 0) { throw 'runtime root ACL no longer grants Network Service ReadAndExecute' }
    $aclEvidenceOk = $true
    $aclEvidenceDetail = $AclEvidencePath
} catch { $aclEvidenceDetail = $_.Exception.Message }
Add-Check 'bounded-runtime-service-acl' $aclEvidenceOk $aclEvidenceDetail

$hardwareOnlyVars = @('DOKKOMPLEKT_TEST_PRINTER','DOKKOMPLEKT_TEST_DUPLEX','DOKKOMPLEKT_TEST_TRAY','DOKKOMPLEKT_REBOOT_EVIDENCE_PATH','DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT')
$exposedHardwareVars = @($hardwareOnlyVars | Where-Object { -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_,'Process')) })
Add-Check 'hardware-only-environment-not-exposed' ($exposedHardwareVars.Count -eq 0) ($exposedHardwareVars -join ', ')

$fingerprint = ''
try { $fingerprint = Get-MachineFingerprint; Add-Check 'machine-fingerprint' ($fingerprint -match '^[0-9a-f]{64}$') $fingerprint }
catch { Add-Check 'machine-fingerprint' $false $_.Exception.Message }

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$report = [ordered]@{
    schema='dokkomplekt.runtime-signing-host-preflight.v2'
    created_at_utc=[DateTime]::UtcNow.ToString('o')
    computer=$env:COMPUTERNAME
    machine_fingerprint_sha256=$fingerprint
    runner_name=$env:RUNNER_NAME
    user=$identity.Name
    session_id=$sessionId
    service_mode_required=$true
    service_identity=$ServiceIdentity
    runtime_root=$RuntimeRoot
    runtime_manifest_sha256=$manifestSha
    runtime_service_acl_evidence=$AclEvidencePath
    hardware_only_environment_exposed=@($exposedHardwareVars)
    ok=$failures.Count -eq 0
    checks=$checks
    failures=$failures
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8
if ($failures.Count -gt 0) { Write-Error ("RUNTIME/SIGNING HOST PREFLIGHT FAILED:`n - " + ($failures -join "`n - ")); exit 1 }
Write-Host "RUNTIME/SIGNING HOST PREFLIGHT PASSED: host=$fingerprint; runner=$env:RUNNER_NAME; service=$ServiceIdentity"
