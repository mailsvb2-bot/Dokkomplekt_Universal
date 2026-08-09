[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RebootEvidencePath = '',
    [string] $RunnerRoot = 'C:\actions-runner',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
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

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
$isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Add-Check -Name 'administrator' -Ok $isAdmin -Detail "user=$($identity.Name)"

$sessionId = (Get-Process -Id $PID).SessionId
$interactive = (-not $identity.IsSystem) -and $sessionId -ne 0
Add-Check -Name 'interactive-user-session' -Ok $interactive -Detail "user=$($identity.Name); session_id=$sessionId"

$runnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
Add-Check -Name 'actions-runner-not-service' -Ok ($runnerServices.Count -eq 0) -Detail (($runnerServices | ForEach-Object { "$($_.Name):$($_.Status)" }) -join ', ')

$task = Get-ScheduledTask -TaskName $RunnerTaskName -ErrorAction SilentlyContinue
$taskOk = $null -ne $task
$taskDetail = if ($null -eq $task) { 'interactive runner scheduled task is missing' } else { "state=$($task.State)" }
Add-Check -Name 'interactive-runner-scheduled-task' -Ok $taskOk -Detail $taskDetail

$runnerConfig = Join-Path $RunnerRoot '.runner'
Add-Check -Name 'runner-config-present' -Ok (Test-Path -LiteralPath $runnerConfig -PathType Leaf) -Detail $runnerConfig

$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sessionId })
Add-Check -Name 'runner-listener-interactive' -Ok ($listener.Count -gt 0) -Detail "count=$($listener.Count); session_id=$sessionId"

$git = Get-Command git.exe -ErrorAction SilentlyContinue
Add-Check -Name 'git' -Ok ($null -ne $git) -Detail (if ($null -eq $git) { 'missing' } else { $git.Source })
$pwsh = Get-Command pwsh.exe -ErrorAction SilentlyContinue
Add-Check -Name 'powershell7' -Ok ($null -ne $pwsh) -Detail (if ($null -eq $pwsh) { 'missing' } else { $pwsh.Source })
$openssl = Get-Command openssl.exe -ErrorAction SilentlyContinue
Add-Check -Name 'openssl' -Ok ($null -ne $openssl) -Detail (if ($null -eq $openssl) { 'missing' } else { $openssl.Source })

$buildTools = Get-VcBuildToolsInstallation
Add-Check -Name 'visual-studio-vctools' -Ok (-not [string]::IsNullOrWhiteSpace($buildTools)) -Detail $buildTools

$webViewCandidates = @(
    Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue
)
Add-Check -Name 'webview2-runtime' -Ok ($webViewCandidates.Count -gt 0) -Detail (if ($webViewCandidates.Count -gt 0) { $webViewCandidates[0].FullName } else { 'missing' })

$wordPath = ''
try {
    $wordPath = [string] (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Winword.exe' -ErrorAction Stop).'(default)'
} catch {
    $winword = Get-Command winword.exe -ErrorAction SilentlyContinue
    if ($null -ne $winword) { $wordPath = $winword.Source }
}
$wordInstalled = -not [string]::IsNullOrWhiteSpace($wordPath) -and (Test-Path -LiteralPath $wordPath -PathType Leaf)
Add-Check -Name 'microsoft-word-installed' -Ok $wordInstalled -Detail $wordPath

$wordComOk = $false
$wordVersion = ''
$word = $null
if ($wordInstalled -and $interactive) {
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        $wordVersion = [string] $word.Version
        $wordComOk = -not [string]::IsNullOrWhiteSpace($wordVersion)
    } catch {
        $wordVersion = $_.Exception.Message
    } finally {
        if ($null -ne $word) {
            try { $word.Quit() } catch { }
            [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null
        }
    }
}
Add-Check -Name 'microsoft-word-com' -Ok $wordComOk -Detail $wordVersion

$virtualPrinterPattern = '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)'
$printer = $null
$printerOk = $false
$printerDetail = ''
try {
    if ($PrinterName -match $virtualPrinterPattern) { throw 'virtual/document printers are forbidden' }
    $printer = Get-Printer -Name $PrinterName -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace([string] $printer.PortName)) { throw 'printer has no port' }
    $port = Get-PrinterPort -Name $printer.PortName -ErrorAction Stop
    $printerDetail = "driver=$($printer.DriverName); port=$($printer.PortName); description=$($port.Description)"
    $printerOk = $true
} catch {
    $printerDetail = $_.Exception.Message
}
Add-Check -Name 'dedicated-real-printer' -Ok $printerOk -Detail $printerDetail

$printLogOk = $false
$printLogDetail = ''
try {
    wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
    $printLog = Get-WinEvent -ListLog 'Microsoft-Windows-PrintService/Operational' -ErrorAction Stop
    $printLogOk = $printLog.IsEnabled
    $printLogDetail = "enabled=$($printLog.IsEnabled)"
} catch {
    $printLogDetail = $_.Exception.Message
}
Add-Check -Name 'printservice-operational-log' -Ok $printLogOk -Detail $printLogDetail

$manifestOk = $false
$manifestDetail = ''
try {
    if (-not [IO.Path]::IsPathFullyQualified($SidecarManifestPath)) { throw 'manifest path must be absolute' }
    $manifestItem = Get-Item -LiteralPath $SidecarManifestPath -Force -ErrorAction Stop
    if ($manifestItem.PSIsContainer -or (($manifestItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'manifest must be a direct regular file' }
    $manifest = Get-Content -LiteralPath $manifestItem.FullName -Raw | ConvertFrom-Json
    if ([int] $manifest.schema -ne 1) { throw 'schema must be 1' }
    if ([string] $manifest.target -ne 'windows-x86_64') { throw 'target must be windows-x86_64' }
    if ($manifest.supply_chain_locked -ne $true) { throw 'supply_chain_locked must be true' }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) { throw 'files must be non-empty' }
    $manifestDetail = "path=$($manifestItem.FullName); files=$(@($manifest.files).Count); sha256=$((Get-FileHash -LiteralPath $manifestItem.FullName -Algorithm SHA256).Hash.ToLowerInvariant())"
    $manifestOk = $true
} catch {
    $manifestDetail = $_.Exception.Message
}
Add-Check -Name 'runner-owned-sidecar-manifest' -Ok $manifestOk -Detail $manifestDetail

$rebootPathOk = $true
$rebootPathDetail = 'not supplied to host preflight'
if (-not [string]::IsNullOrWhiteSpace($RebootEvidencePath)) {
    $rebootPathOk = [IO.Path]::IsPathFullyQualified($RebootEvidencePath)
    $rebootPathDetail = $RebootEvidencePath
}
Add-Check -Name 'reboot-evidence-path-absolute' -Ok $rebootPathOk -Detail $rebootPathDetail

$powerState = (& powercfg /getactivescheme 2>$null) -join ' '
Add-Check -Name 'power-plan-readable' -Ok (-not [string]::IsNullOrWhiteSpace($powerState)) -Detail $powerState

$networkTargets = @(
    'https://github.com',
    'https://api.github.com',
    'https://static.rust-lang.org',
    'https://registry.npmjs.org'
)
foreach ($target in $networkTargets) {
    $ok = $false
    $detail = ''
    try {
        $response = Invoke-WebRequest -UseBasicParsing -Method Head -Uri $target -TimeoutSec 15
        $ok = [int] $response.StatusCode -ge 200 -and [int] $response.StatusCode -lt 500
        $detail = "status=$([int] $response.StatusCode)"
    } catch {
        $detail = $_.Exception.Message
    }
    Add-Check -Name ("network-" + ([Uri] $target).Host) -Ok $ok -Detail $detail
}

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$report = [ordered]@{
    schema = 'dokkomplekt.hardware-runner-host-preflight.v1'
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    computer = $env:COMPUTERNAME
    user = $identity.Name
    session_id = $sessionId
    ok = $failures.Count -eq 0
    checks = $checks
    failures = $failures
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8

if ($failures.Count -gt 0) {
    Write-Error ("HARDWARE RUNNER HOST PREFLIGHT FAILED:`n - " + ($failures -join "`n - "))
    exit 1
}
Write-Host "HARDWARE RUNNER HOST PREFLIGHT PASSED: user=$($identity.Name); session=$sessionId; printer=$PrinterName"
