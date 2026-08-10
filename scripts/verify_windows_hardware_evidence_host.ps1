[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [string] $RebootEvidencePath = '',
    [string] $RunnerRoot = 'C:\actions-runner-hardware',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [string] $OutputPath = 'verification/release/HARDWARE_RUNNER_HOST.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
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

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
Add-Check 'administrator' $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) "user=$($identity.Name)"

$sessionId = (Get-Process -Id $PID).SessionId
$interactive = (-not $identity.IsSystem) -and $sessionId -ne 0
Add-Check 'interactive-user-session' $interactive "user=$($identity.Name); session_id=$sessionId"

$runnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
Add-Check 'actions-runner-not-service' ($runnerServices.Count -eq 0) (($runnerServices | ForEach-Object { "$($_.Name):$($_.Status)" }) -join ', ')
$task = Get-ScheduledTask -TaskName $RunnerTaskName -ErrorAction SilentlyContinue
Add-Check 'interactive-runner-scheduled-task' ($null -ne $task) (if ($null -eq $task) { 'missing' } else { "state=$($task.State)" })
$runnerConfig = Join-Path $RunnerRoot '.runner'
Add-Check 'runner-config-present' (Test-Path -LiteralPath $runnerConfig -PathType Leaf) $runnerConfig
$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sessionId })
Add-Check 'runner-listener-interactive' ($listener.Count -gt 0) "count=$($listener.Count); session_id=$sessionId"

foreach ($required in @('git.exe','pwsh.exe')) {
    $command = Get-Command $required -ErrorAction SilentlyContinue
    Add-Check ("tool-" + $required) ($null -ne $command) (if ($null -eq $command) { 'missing' } else { $command.Source })
}
$buildTools = Get-VcBuildToolsInstallation
Add-Check 'visual-studio-vctools' (-not [string]::IsNullOrWhiteSpace($buildTools)) $buildTools
$webViewCandidates = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
Add-Check 'webview2-runtime' ($webViewCandidates.Count -gt 0) (if ($webViewCandidates.Count -gt 0) { $webViewCandidates[0].FullName } else { 'missing' })

$runtimeManifestEnv = [Environment]::GetEnvironmentVariable('DOKKOMPLEKT_SIDECAR_MANIFEST_PATH','Process')
$runtimeManifestNotExposed = [string]::IsNullOrWhiteSpace($runtimeManifestEnv)
Add-Check 'runtime-manifest-not-exposed' $runtimeManifestNotExposed (if ($runtimeManifestNotExposed) { 'DOKKOMPLEKT_SIDECAR_MANIFEST_PATH is absent' } else { 'runtime manifest must not be exposed to hardware trust domain' })
$signingSecretNames = @(
    'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64',
    'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD',
    'DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64',
    'DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64',
    'DOKKOMPLEKT_GATE_PRIVATE_KEY_B64'
)
$exposedSigningSecrets = @($signingSecretNames | Where-Object { -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_,'Process')) })
Add-Check 'signing-secrets-not-exposed' ($exposedSigningSecrets.Count -eq 0) (if ($exposedSigningSecrets.Count -eq 0) { 'no signing/private-key variables exposed' } else { 'forbidden variables: ' + ($exposedSigningSecrets -join ', ') })

$wordPath = ''
try { $wordPath = [string](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Winword.exe' -ErrorAction Stop).'(default)' }
catch { $winword = Get-Command winword.exe -ErrorAction SilentlyContinue; if ($null -ne $winword) { $wordPath = $winword.Source } }
$wordInstalled = -not [string]::IsNullOrWhiteSpace($wordPath) -and (Test-Path -LiteralPath $wordPath -PathType Leaf)
Add-Check 'microsoft-word-installed' $wordInstalled $wordPath
$wordComOk = $false
$wordVersion = ''
$word = $null
if ($wordInstalled -and $interactive) {
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        $wordVersion = [string]$word.Version
        $wordComOk = -not [string]::IsNullOrWhiteSpace($wordVersion)
    } catch { $wordVersion = $_.Exception.Message }
    finally { if ($null -ne $word) { try { $word.Quit() } catch {}; [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null } }
}
Add-Check 'microsoft-word-com' $wordComOk $wordVersion

$virtualPrinterPattern = '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)'
$printerOk = $false
$printerDetail = ''
try {
    if ($PrinterName -match $virtualPrinterPattern) { throw 'virtual/document printers are forbidden' }
    $printer = Get-Printer -Name $PrinterName -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace([string]$printer.PortName)) { throw 'printer has no port' }
    $port = Get-PrinterPort -Name $printer.PortName -ErrorAction Stop
    $printerDetail = "driver=$($printer.DriverName); port=$($printer.PortName); description=$($port.Description)"
    $printerOk = $true
} catch { $printerDetail = $_.Exception.Message }
Add-Check 'dedicated-real-printer' $printerOk $printerDetail

$printLogOk = $false
$printLogDetail = ''
try {
    wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
    $printLog = Get-WinEvent -ListLog 'Microsoft-Windows-PrintService/Operational' -ErrorAction Stop
    $printLogOk = $printLog.IsEnabled
    $printLogDetail = "enabled=$($printLog.IsEnabled)"
} catch { $printLogDetail = $_.Exception.Message }
Add-Check 'printservice-operational-log' $printLogOk $printLogDetail

$rebootPathOk = $true
$rebootPathDetail = 'not supplied'
if (-not [string]::IsNullOrWhiteSpace($RebootEvidencePath)) {
    $rebootPathOk = [IO.Path]::IsPathFullyQualified($RebootEvidencePath)
    $rebootPathDetail = $RebootEvidencePath
}
Add-Check 'reboot-evidence-path-absolute' $rebootPathOk $rebootPathDetail
$powerState = (& powercfg /getactivescheme 2>$null) -join ' '
Add-Check 'power-plan-readable' (-not [string]::IsNullOrWhiteSpace($powerState)) $powerState

$fingerprint = ''
try { $fingerprint = Get-MachineFingerprint; Add-Check 'machine-fingerprint' ($fingerprint -match '^[0-9a-f]{64}$') $fingerprint }
catch { Add-Check 'machine-fingerprint' $false $_.Exception.Message }

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$report = [ordered]@{
    schema='dokkomplekt.hardware-evidence-host-preflight.v3'
    created_at_utc=[DateTime]::UtcNow.ToString('o')
    computer=$env:COMPUTERNAME
    machine_fingerprint_sha256=$fingerprint
    runner_name=$env:RUNNER_NAME
    user=$identity.Name
    session_id=$sessionId
    runtime_manifest_env_exposed=-not $runtimeManifestNotExposed
    signing_secret_env_exposed=@($exposedSigningSecrets)
    build_toolchain_required=$true
    visual_studio_vctools=$buildTools
    ok=$failures.Count -eq 0
    checks=$checks
    failures=$failures
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8
if ($failures.Count -gt 0) { Write-Error ("HARDWARE EVIDENCE HOST PREFLIGHT FAILED:`n - " + ($failures -join "`n - ")); exit 1 }
Write-Host "HARDWARE EVIDENCE HOST PREFLIGHT PASSED: host=$fingerprint; user=$($identity.Name); printer=$PrinterName"
