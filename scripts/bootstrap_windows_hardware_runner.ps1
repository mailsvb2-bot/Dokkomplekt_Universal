[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RegistrationToken,
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RepositoryUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Universal',
    [string] $RunnerRoot = 'C:\actions-runner',
    [string] $RunnerName = '',
    [string] $RunnerLabel = 'dokkomplekt-hardware-e2e',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this script from an elevated Windows PowerShell/PowerShell window.'
    }
}

function Refresh-ProcessPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = @($machine, $user) -join ';'
}

function Install-WingetPackage {
    param([Parameter(Mandatory = $true)] [string] $Id)
    if ($null -eq (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
        throw "WinGet is required to install $Id automatically. Install App Installer/WinGet first or rerun without -InstallPrerequisites after installing prerequisites manually."
    }
    Write-Host "Installing $Id ..."
    & winget.exe install --id $Id --exact --source winget --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "WinGet returned $LASTEXITCODE while installing $Id. The bootstrap will verify the resulting command before continuing."
    }
    Refresh-ProcessPath
}

function Test-VcBuildTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return $false }
    $installation = (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
    return -not [string]::IsNullOrWhiteSpace($installation)
}

function Install-VcBuildTools {
    $bootstrapper = Join-Path $env:TEMP 'vs_BuildTools.exe'
    Invoke-WebRequest -UseBasicParsing -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $bootstrapper
    $arguments = @(
        '--quiet', '--wait', '--norestart', '--nocache',
        '--add', 'Microsoft.VisualStudio.Workload.VCTools',
        '--includeRecommended'
    )
    $process = Start-Process -FilePath $bootstrapper -ArgumentList $arguments -Wait -PassThru
    Remove-Item -LiteralPath $bootstrapper -Force -ErrorAction SilentlyContinue
    if ($process.ExitCode -notin @(0, 3010)) {
        throw "Visual Studio Build Tools installer failed with exit code $($process.ExitCode)."
    }
    if (-not (Test-VcBuildTools)) {
        throw 'Visual Studio Build Tools C++ workload is still unavailable after installation.'
    }
    if ($process.ExitCode -eq 3010) {
        Write-Warning 'Visual Studio Build Tools requested a reboot. Reboot Windows before running production hardware E2E.'
    }
}

function Assert-InteractiveSession {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if ($identity.IsSystem -or $identity.Name -eq 'NT AUTHORITY\SYSTEM') {
        throw 'The hardware runner must run as a dedicated interactive Windows user, not LocalSystem.'
    }
    $sessionId = (Get-Process -Id $PID).SessionId
    if ($sessionId -eq 0) {
        throw 'The hardware runner must run in an interactive user session. Session 0/service execution is forbidden for Word COM and visible-GUI evidence.'
    }
    return [ordered]@{ user = $identity.Name; session_id = $sessionId }
}

function Assert-WordCom {
    $wordPath = $null
    try {
        $wordPath = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Winword.exe' -ErrorAction Stop).'(default)'
    } catch {
        $command = Get-Command winword.exe -ErrorAction SilentlyContinue
        if ($null -ne $command) { $wordPath = $command.Source }
    }
    if ([string]::IsNullOrWhiteSpace([string] $wordPath) -or -not (Test-Path -LiteralPath $wordPath -PathType Leaf)) {
        throw 'Licensed desktop Microsoft Word is not installed for the hardware runner.'
    }

    $word = $null
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        $version = [string] $word.Version
        if ([string]::IsNullOrWhiteSpace($version)) { throw 'Word COM returned an empty version.' }
        return [ordered]@{ path = [string] $wordPath; version = $version }
    } finally {
        if ($null -ne $word) {
            try { $word.Quit() } catch { }
            [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null
        }
    }
}

function Assert-PhysicalPrinterQueue {
    param([Parameter(Mandatory = $true)] [string] $Name)
    $forbidden = '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)'
    if ($Name -match $forbidden) {
        throw "Printer '$Name' is a virtual/document printer. Hardware E2E requires a dedicated real printer queue."
    }
    $printer = Get-Printer -Name $Name -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace([string] $printer.PortName)) {
        throw "Printer '$Name' has no configured port."
    }
    $port = Get-PrinterPort -Name $printer.PortName -ErrorAction Stop
    try {
        wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
    } catch {
        throw "Could not enable PrintService Operational log: $($_.Exception.Message)"
    }
    return [ordered]@{
        name = $printer.Name
        driver = $printer.DriverName
        port = $printer.PortName
        port_description = $port.Description
    }
}

function Assert-SidecarManifest {
    param([Parameter(Mandatory = $true)] [string] $Path)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) {
        throw 'SidecarManifestPath must be an absolute runner-owned path.'
    }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw 'SidecarManifestPath must be a direct regular file, not a directory or reparse point.'
    }
    $manifest = Get-Content -LiteralPath $item.FullName -Raw | ConvertFrom-Json
    if ([int] $manifest.schema -ne 1 -or [string] $manifest.target -ne 'windows-x86_64') {
        throw 'Runner-owned sidecar manifest must use schema=1 and target=windows-x86_64.'
    }
    if ($manifest.supply_chain_locked -ne $true) {
        throw 'Runner-owned sidecar manifest must be supply_chain_locked=true.'
    }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) {
        throw 'Runner-owned sidecar manifest has no files.'
    }
    return $item.FullName
}

function Ensure-GitOpenSslOnMachinePath {
    if ($null -ne (Get-Command openssl.exe -ErrorAction SilentlyContinue)) { return }
    $gitCommand = Get-Command git.exe -ErrorAction SilentlyContinue
    if ($null -eq $gitCommand) { throw 'Git is installed but git.exe is not on PATH.' }
    $gitRoot = Split-Path (Split-Path $gitCommand.Source -Parent) -Parent
    $opensslDir = Join-Path $gitRoot 'usr\bin'
    $opensslPath = Join-Path $opensslDir 'openssl.exe'
    if (-not (Test-Path -LiteralPath $opensslPath -PathType Leaf)) {
        throw 'OpenSSL is required and was not found in the Git for Windows distribution.'
    }
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $segments = @($machinePath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($segments -notcontains $opensslDir) {
        [Environment]::SetEnvironmentVariable('Path', (($segments + $opensslDir) -join ';'), 'Machine')
    }
    Refresh-ProcessPath
    if ($null -eq (Get-Command openssl.exe -ErrorAction SilentlyContinue)) {
        throw 'OpenSSL is still unavailable on PATH.'
    }
}

function Get-LatestRunnerAsset {
    $headers = @{
        Accept = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
        'User-Agent' = 'Dokkomplekt-Hardware-Runner-Bootstrap'
    }
    $release = Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/actions/runner/releases/latest'
    $asset = @($release.assets | Where-Object { $_.name -match '^actions-runner-win-x64-[0-9.]+\.zip$' }) | Select-Object -First 1
    if ($null -eq $asset) { throw 'GitHub runner win-x64 release asset was not found.' }
    $digest = [string] $asset.digest
    if ($digest -notmatch '^sha256:([0-9a-fA-F]{64})$') {
        throw 'GitHub runner release asset does not expose a SHA-256 digest; refusing an unpinned download.'
    }
    return [ordered]@{
        url = [string] $asset.browser_download_url
        sha256 = $Matches[1].ToLowerInvariant()
        name = [string] $asset.name
    }
}

function Register-InteractiveRunnerTask {
    param(
        [Parameter(Mandatory = $true)] [string] $TaskName,
        [Parameter(Mandatory = $true)] [string] $Root,
        [Parameter(Mandatory = $true)] [string] $UserName
    )
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
if ([string]::IsNullOrWhiteSpace($RunnerName)) { $RunnerName = "dokkomplekt-hardware-$env:COMPUTERNAME" }
if ($RegistrationToken -notmatch '^[A-Za-z0-9_-]{20,}$') { throw 'RegistrationToken does not look like a GitHub runner registration token.' }

$existingRunnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
if ($existingRunnerServices.Count -gt 0) {
    throw 'An Actions runner Windows service is installed. Remove/reconfigure it: this hardware runner must execute in an interactive user session, never as a Windows service.'
}

if ($InstallPrerequisites) {
    if ($null -eq (Get-Command git.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage -Id 'Git.Git' }
    if ($null -eq (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage -Id 'Microsoft.PowerShell' }
    if (-not (Test-VcBuildTools)) { Install-VcBuildTools }
}
Refresh-ProcessPath

foreach ($requiredCommand in @('git.exe', 'pwsh.exe')) {
    if ($null -eq (Get-Command $requiredCommand -ErrorAction SilentlyContinue)) {
        throw "$requiredCommand is required. Install prerequisites or rerun with -InstallPrerequisites."
    }
}
if (-not (Test-VcBuildTools)) {
    throw 'Visual Studio Build Tools 2022 with Microsoft.VisualStudio.Workload.VCTools is required.'
}
Ensure-GitOpenSslOnMachinePath
$wordInfo = Assert-WordCom
$printerInfo = Assert-PhysicalPrinterQueue -Name $PrinterName
$sidecarManifest = Assert-SidecarManifest -Path $SidecarManifestPath

$webViewCandidates = @(
    Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue
)
if ($webViewCandidates.Count -eq 0) {
    throw 'Microsoft Edge WebView2 Runtime is required for the installed Tauri GUI.'
}

powercfg /change standby-timeout-ac 0 | Out-Null
powercfg /change hibernate-timeout-ac 0 | Out-Null
New-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -PropertyType DWord -Value 1 -Force | Out-Null
& git.exe config --system core.longpaths true

$rootParent = Split-Path -Parent $RunnerRoot
if (-not [string]::IsNullOrWhiteSpace($rootParent)) { New-Item -ItemType Directory -Force -Path $rootParent | Out-Null }
if (Test-Path -LiteralPath $RunnerRoot) {
    $entries = @(Get-ChildItem -LiteralPath $RunnerRoot -Force -ErrorAction Stop)
    if ($entries.Count -gt 0) {
        throw "RunnerRoot '$RunnerRoot' is not empty. Use a clean directory so stale credentials/config cannot be reused."
    }
} else {
    New-Item -ItemType Directory -Force -Path $RunnerRoot | Out-Null
}

$runnerAsset = Get-LatestRunnerAsset
$runnerZip = Join-Path $env:TEMP $runnerAsset.name
Invoke-WebRequest -UseBasicParsing -Uri $runnerAsset.url -OutFile $runnerZip
$actualRunnerSha = (Get-FileHash -LiteralPath $runnerZip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualRunnerSha -ne $runnerAsset.sha256) {
    Remove-Item -LiteralPath $runnerZip -Force -ErrorAction SilentlyContinue
    throw "GitHub runner package SHA-256 mismatch: expected $($runnerAsset.sha256), got $actualRunnerSha"
}
Expand-Archive -LiteralPath $runnerZip -DestinationPath $RunnerRoot -Force
Remove-Item -LiteralPath $runnerZip -Force

Push-Location $RunnerRoot
try {
    & .\config.cmd --unattended --url $RepositoryUrl --token $RegistrationToken --name $RunnerName --labels $RunnerLabel --work _work --replace
    if ($LASTEXITCODE -ne 0) { throw "GitHub runner config.cmd failed with exit code $LASTEXITCODE." }
} finally {
    Pop-Location
}

$runnerUser = $interactive.user
Register-InteractiveRunnerTask -TaskName $RunnerTaskName -Root $RunnerRoot -UserName $runnerUser
Start-Sleep -Seconds 3
$task = Get-ScheduledTask -TaskName $RunnerTaskName -ErrorAction Stop
$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue)
if ($listener.Count -eq 0) {
    throw 'Runner.Listener did not start. Inspect C:\actions-runner\_diag and the scheduled task history.'
}

$evidenceRoot = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$record = [ordered]@{
    schema = 'dokkomplekt.hardware-runner-bootstrap.v1'
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    computer = $env:COMPUTERNAME
    user = $runnerUser
    session_id = $interactive.session_id
    runner_name = $RunnerName
    runner_label = $RunnerLabel
    runner_root = (Resolve-Path -LiteralPath $RunnerRoot).Path
    scheduled_task = $task.TaskName
    service_mode_forbidden = $true
    runner_package = $runnerAsset.name
    runner_package_sha256 = $actualRunnerSha
    word_path = $wordInfo.path
    word_version = $wordInfo.version
    printer = $printerInfo
    sidecar_manifest_path = $sidecarManifest
    sidecar_manifest_sha256 = (Get-FileHash -LiteralPath $sidecarManifest -Algorithm SHA256).Hash.ToLowerInvariant()
    webview2_path = $webViewCandidates[0].FullName
    visual_studio_vctools = $true
    powershell7 = (Get-Command pwsh.exe).Source
    git = (Get-Command git.exe).Source
    openssl = (Get-Command openssl.exe).Source
}
$bootstrapEvidence = Join-Path $evidenceRoot 'HARDWARE_RUNNER_BOOTSTRAP.json'
$record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $bootstrapEvidence -Encoding utf8
Write-Host "DOKKOMPLEKT HARDWARE RUNNER BOOTSTRAPPED: $RunnerName"
Write-Host "Evidence: $bootstrapEvidence"
Write-Host 'Important: keep this dedicated Windows user logged in. The runner is intentionally not installed as a Windows service.'
