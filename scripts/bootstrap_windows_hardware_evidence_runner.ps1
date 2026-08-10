[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RegistrationToken,
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [string] $RunnerRoot = 'C:\actions-runner-hardware',
    [string] $RunnerName = '',
    [string] $RunnerLabel = 'dokkomplekt-hardware',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$ExpectedRepository = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation'

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run hardware evidence bootstrap from an elevated interactive PowerShell window.' }
}

function Assert-InteractiveSession {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $sessionId = (Get-Process -Id $PID).SessionId
    if ($identity.IsSystem -or $identity.Name -eq 'NT AUTHORITY\SYSTEM' -or $sessionId -eq 0) {
        throw 'Hardware evidence runner requires a dedicated interactive Windows user. Session 0/service execution is forbidden.'
    }
    return [ordered]@{ user=$identity.Name; session_id=$sessionId }
}

function Refresh-Path {
    $env:Path = @([Environment]::GetEnvironmentVariable('Path','Machine'),[Environment]::GetEnvironmentVariable('Path','User')) -join ';'
}
function Install-WingetPackage([string] $Id) {
    if ($null -eq (Get-Command winget.exe -ErrorAction SilentlyContinue)) { throw "WinGet is required to install $Id automatically." }
    & winget.exe install --id $Id --exact --source winget --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
    if ($LASTEXITCODE -notin @(0,3010)) { throw "WinGet failed to install $Id with exit code $LASTEXITCODE." }
    Refresh-Path
}
function Get-MachineFingerprint {
    $machineGuid = [string] (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid -ErrorAction Stop).MachineGuid
    $bytes = [Text.Encoding]::UTF8.GetBytes($machineGuid.Trim().ToLowerInvariant())
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = $sha.ComputeHash($bytes) } finally { $sha.Dispose() }
    return ([BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
}
function Assert-WordCom {
    $wordPath = ''
    try { $wordPath = [string](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Winword.exe' -ErrorAction Stop).'(default)' }
    catch { $command = Get-Command winword.exe -ErrorAction SilentlyContinue; if ($null -ne $command) { $wordPath = $command.Source } }
    if ([string]::IsNullOrWhiteSpace($wordPath) -or -not (Test-Path -LiteralPath $wordPath -PathType Leaf)) { throw 'Licensed desktop Microsoft Word is not installed.' }
    $word = $null
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        $version = [string]$word.Version
        if ([string]::IsNullOrWhiteSpace($version)) { throw 'Word COM returned an empty version.' }
        return [ordered]@{ path=$wordPath; version=$version }
    } finally {
        if ($null -ne $word) { try { $word.Quit() } catch {}; [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null }
    }
}
function Assert-RealPrinter([string] $Name) {
    if ($Name -match '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)') { throw "Printer '$Name' is virtual/document-only." }
    $printer = Get-Printer -Name $Name -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace([string]$printer.PortName)) { throw "Printer '$Name' has no port." }
    $port = Get-PrinterPort -Name $printer.PortName -ErrorAction Stop
    wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
    return [ordered]@{ name=$printer.Name; driver=$printer.DriverName; port=$printer.PortName; port_description=$port.Description }
}
function Get-RunnerAsset {
    $headers = @{ Accept='application/vnd.github+json'; 'X-GitHub-Api-Version'='2022-11-28'; 'User-Agent'='Dokkomplekt-Hardware-Evidence-Runner-Bootstrap' }
    $release = Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/actions/runner/releases/latest'
    $asset = @($release.assets | Where-Object { $_.name -match '^actions-runner-win-x64-[0-9.]+\.zip$' }) | Select-Object -First 1
    if ($null -eq $asset) { throw 'GitHub runner win-x64 release asset was not found.' }
    $digest = [string]$asset.digest
    if ($digest -notmatch '^sha256:([0-9a-fA-F]{64})$') { throw 'GitHub runner release asset has no usable SHA-256 digest.' }
    return [ordered]@{ name=[string]$asset.name; url=[string]$asset.browser_download_url; sha256=$Matches[1].ToLowerInvariant() }
}
function Register-InteractiveRunnerTask([string] $TaskName,[string] $Root,[string] $UserName) {
    $runCmd = Join-Path $Root 'run.cmd'
    if (-not (Test-Path -LiteralPath $runCmd -PathType Leaf)) { throw 'run.cmd is missing after runner configuration.' }
    $action = New-ScheduledTaskAction -Execute $env:ComSpec -Argument "/d /s /c `"$runCmd`"" -WorkingDirectory $Root
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $UserName
    $principal = New-ScheduledTaskPrincipal -UserId $UserName -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RestartCount 10 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero)
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    Start-ScheduledTask -TaskName $TaskName
}

Assert-Administrator
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 is required.' }
$interactive = Assert-InteractiveSession
$RepositoryUrl = $RepositoryUrl.Trim().TrimEnd('/')
if ($RepositoryUrl -ine $ExpectedRepository) { throw "Hardware evidence runner must be registered only in $ExpectedRepository" }
if ($RunnerLabel -ne 'dokkomplekt-hardware') { throw 'Hardware evidence runner label is fixed to dokkomplekt-hardware.' }
if ($RegistrationToken -notmatch '^[A-Za-z0-9_-]{20,}$') { throw 'RegistrationToken does not look valid.' }
if ([string]::IsNullOrWhiteSpace($RunnerName)) { $RunnerName = "dokkomplekt-hardware-$env:COMPUTERNAME" }

$services = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
if ($services.Count -gt 0) { throw 'Actions runner service detected. Hardware evidence host must not contain the runtime/signing service runner.' }

$sensitiveVars = @('DOKKOMPLEKT_SIDECAR_MANIFEST_PATH','DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64','DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD','DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64','DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64','DOKKOMPLEKT_GATE_PRIVATE_KEY_B64')
$exposed = @($sensitiveVars | Where-Object { -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_,'Process')) })
if ($exposed.Count -gt 0) { throw "Runtime/signing environment must not be exposed on hardware host: $($exposed -join ', ')" }

if ($InstallPrerequisites) {
    if ($null -eq (Get-Command git.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage 'Git.Git' }
    if ($null -eq (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage 'Microsoft.PowerShell' }
}
Refresh-Path
foreach ($required in @('git.exe','pwsh.exe')) { if ($null -eq (Get-Command $required -ErrorAction SilentlyContinue)) { throw "$required is required." } }
$word = Assert-WordCom
$printer = Assert-RealPrinter $PrinterName
$webView = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
if ($webView.Count -eq 0) { throw 'Microsoft Edge WebView2 Runtime is required.' }
powercfg /change standby-timeout-ac 0 | Out-Null
powercfg /change hibernate-timeout-ac 0 | Out-Null
New-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -PropertyType DWord -Value 1 -Force | Out-Null
& git.exe config --system core.longpaths true

if (Test-Path -LiteralPath $RunnerRoot) {
    if (@(Get-ChildItem -LiteralPath $RunnerRoot -Force).Count -gt 0) { throw "RunnerRoot '$RunnerRoot' is not empty." }
} else { New-Item -ItemType Directory -Force -Path $RunnerRoot | Out-Null }
$asset = Get-RunnerAsset
$zip = Join-Path $env:TEMP $asset.name
Invoke-WebRequest -UseBasicParsing -Uri $asset.url -OutFile $zip
try {
    $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $asset.sha256) { throw "GitHub runner package SHA-256 mismatch: expected $($asset.sha256), got $actual" }
    Expand-Archive -LiteralPath $zip -DestinationPath $RunnerRoot -Force
} finally { Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue }

Push-Location $RunnerRoot
try {
    & .\config.cmd --unattended --url $RepositoryUrl --token $RegistrationToken --name $RunnerName --labels $RunnerLabel --work _work --replace
    if ($LASTEXITCODE -ne 0) { throw "GitHub runner config.cmd failed with exit code $LASTEXITCODE." }
} finally { Pop-Location }
Register-InteractiveRunnerTask $RunnerTaskName $RunnerRoot $interactive.user
Start-Sleep -Seconds 4
$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $interactive.session_id })
if ($listener.Count -eq 0) { throw 'Runner.Listener did not start in the interactive session.' }

$evidenceRoot = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$evidencePath = Join-Path $evidenceRoot 'HARDWARE_RUNNER_BOOTSTRAP.json'
[ordered]@{
    schema='dokkomplekt.hardware-evidence-runner-bootstrap.v1'
    created_at_utc=[DateTime]::UtcNow.ToString('o')
    computer=$env:COMPUTERNAME
    machine_fingerprint_sha256=Get-MachineFingerprint
    user=$interactive.user
    session_id=$interactive.session_id
    repository_url=$RepositoryUrl
    runner_name=$RunnerName
    runner_label=$RunnerLabel
    runner_root=(Resolve-Path -LiteralPath $RunnerRoot).Path
    scheduled_task=$RunnerTaskName
    service_mode_forbidden=$true
    runtime_signing_environment_forbidden=$true
    runner_package=$asset.name
    runner_package_sha256=$actual
    word_path=$word.path
    word_version=$word.version
    printer=$printer
    webview2_path=$webView[0].FullName
    powershell7=(Get-Command pwsh.exe).Source
    git=(Get-Command git.exe).Source
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $evidencePath -Encoding utf8
Write-Host "DOKKOMPLEKT HARDWARE EVIDENCE RUNNER BOOTSTRAPPED: $RunnerName"
Write-Host "Evidence: $evidencePath"
Write-Host 'Keep the dedicated hardware user logged in. This runner must never be installed as a Windows service.'
