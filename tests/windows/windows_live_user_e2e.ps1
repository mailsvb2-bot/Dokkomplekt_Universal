[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $InstallerRoot,
    [string] $ScenarioRegistry = 'verification/e2e/LIVE_USER_SCENARIOS.json',
    [string] $OutputPath = 'verification/release/LIVE_USER_E2E.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($env:DOKKOMPLEKT_RUN_HARDWARE_E2E -ne '1') { throw 'Live Windows user E2E is opt-in and may run only on the dedicated hardware runner.' }
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Live E2E requires Windows x64.' }

$os = Get-CimInstance Win32_OperatingSystem
if ([string]$os.Caption -notmatch 'Windows 11') { throw "Live E2E requires Windows 11; detected: $($os.Caption)" }
if ([string]$os.OSArchitecture -notmatch '64') { throw "Live E2E requires a 64-bit OS; detected: $($os.OSArchitecture)" }

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sessionId = (Get-Process -Id $PID).SessionId
if ($identity.IsSystem -or $sessionId -eq 0) { throw 'Live E2E must execute in a real interactive user session, never Session 0/service mode.' }
if (@(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue).Count -gt 0) { throw 'Live E2E refuses Actions runner service mode.' }

if (-not (Test-Path -LiteralPath $ScenarioRegistry -PathType Leaf)) { throw "Scenario registry missing: $ScenarioRegistry" }
$registry = Get-Content -LiteralPath $ScenarioRegistry -Raw | ConvertFrom-Json
if ($registry.schema -ne 'dokkomplekt.live-user-scenarios.v1') { throw 'Unsupported live scenario registry schema.' }
$scenarios = @($registry.scenarios)
if ($scenarios.Count -eq 0) { throw 'Live scenario registry is empty.' }
$ids = @($scenarios | ForEach-Object { [string]$_.id })
if (@($ids | Sort-Object -Unique).Count -ne $ids.Count) { throw 'Live scenario registry contains duplicate IDs.' }
foreach ($number in 1..23) {
    $requiredId = 'FPR-{0:d2}' -f $number
    if ($ids -notcontains $requiredId) { throw "Live scenario registry is missing $requiredId." }
}
foreach ($scenario in $scenarios) {
    if ($scenario.required -ne $true) { throw "Scenario $($scenario.id) is not fail-closed required." }
    if ([string]::IsNullOrWhiteSpace([string]$scenario.executor)) { throw "Scenario $($scenario.id) has no executor." }
    if ([string]::IsNullOrWhiteSpace([string]$scenario.evidence_marker)) { throw "Scenario $($scenario.id) has no evidence marker." }
}

$installer = @(Get-ChildItem -LiteralPath $InstallerRoot -Recurse -File -Filter '*.exe' | Where-Object { $_.DirectoryName -match 'offline|nsis' } | Sort-Object Length -Descending | Select-Object -First 1)
if ($installer.Count -ne 1) { throw "Expected one signed offline NSIS installer under $InstallerRoot." }
$signature = Get-AuthenticodeSignature -FilePath $installer[0].FullName
if ($signature.Status -ne 'Valid') { throw "Live E2E refuses unsigned/invalid installer: $($signature.Status)" }

$evidenceRoot = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($evidenceRoot)) { New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null }
$logRoot = Join-Path $env:RUNNER_TEMP ("dokkomplekt-live-e2e-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
$baselineLog = Join-Path $logRoot 'installed-baseline.log'
$e1Log = Join-Path $logRoot 'installed-e1.log'

$previousAdversarial = $env:DOKKOMPLEKT_ADVERSARIAL
try {
    $env:DOKKOMPLEKT_ADVERSARIAL = '1'
    & pwsh -NoProfile -File tests/installer/windows_installer_contract.ps1 -BundleDir $InstallerRoot -TauriConfig src-tauri/tauri.offline.conf.json -ExpectedWebViewMode offlineInstaller *>&1 | Tee-Object -FilePath $baselineLog
    if ($LASTEXITCODE -ne 0) { throw "Installed baseline live E2E failed with exit code $LASTEXITCODE." }

    & pwsh -NoProfile -File tests/installer/windows_e1_accounting_contract.ps1 -BundleDir $InstallerRoot -TauriConfig src-tauri/tauri.offline.conf.json -ExpectedWebViewMode offlineInstaller *>&1 | Tee-Object -FilePath $e1Log
    if ($LASTEXITCODE -ne 0) { throw "Installed E1 live E2E failed with exit code $LASTEXITCODE." }
} finally {
    if ($null -eq $previousAdversarial) { Remove-Item Env:DOKKOMPLEKT_ADVERSARIAL -ErrorAction SilentlyContinue } else { $env:DOKKOMPLEKT_ADVERSARIAL = $previousAdversarial }
}

$logs = @{
    'installed-baseline' = if (Test-Path $baselineLog) { Get-Content $baselineLog -Raw } else { '' }
    'installed-e1' = if (Test-Path $e1Log) { Get-Content $e1Log -Raw } else { '' }
}
$results = [System.Collections.Generic.List[object]]::new()
foreach ($scenario in $scenarios) {
    $lane = [string]$scenario.lane
    if ($lane -notin @('installed-baseline','installed-e1')) { continue }
    $marker = [string]$scenario.evidence_marker
    $passed = $logs[$lane].Contains($marker)
    $results.Add([ordered]@{ id=[string]$scenario.id; lane=$lane; executor=[string]$scenario.executor; evidence_marker=$marker; passed=$passed })
    if (-not $passed) { throw "Live E2E scenario $($scenario.id) produced no required evidence marker: $marker" }
}

$report = [ordered]@{
    schema = 'dokkomplekt.live-user-e2e.v1'
    completed_at_utc = [DateTime]::UtcNow.ToString('o')
    computer = $env:COMPUTERNAME
    user = $identity.Name
    session_id = $sessionId
    os_caption = [string]$os.Caption
    os_version = [string]$os.Version
    os_architecture = [string]$os.OSArchitecture
    installer = $installer[0].Name
    installer_sha256 = (Get-FileHash $installer[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    scenario_registry_sha256 = (Get-FileHash $ScenarioRegistry -Algorithm SHA256).Hash.ToLowerInvariant()
    installed_scenario_count = $results.Count
    installed_scenarios_passed = @($results | Where-Object passed).Count
    results = $results
    remaining_live_lanes = @('hardware-live','reboot-live','update-live')
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Copy-Item -LiteralPath $baselineLog -Destination (Join-Path $evidenceRoot 'LIVE_USER_E2E_BASELINE.log') -Force
Copy-Item -LiteralPath $e1Log -Destination (Join-Path $evidenceRoot 'LIVE_USER_E2E_E1.log') -Force
Write-Host "LIVE WINDOWS USER E2E INSTALLED LANES PASSED: $($results.Count)/$($results.Count) scenarios on $($os.Caption) x64 interactive session."
