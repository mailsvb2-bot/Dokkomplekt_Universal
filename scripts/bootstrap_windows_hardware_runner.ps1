[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RegistrationToken,
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $PrinterName,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RunnerRoot = 'C:\actions-runner',
    [string] $RunnerName = '',
    [string] $RunnerLabel = 'dokkomplekt-hardware-e2e',
    [string] $RunnerTaskName = 'Dokkomplekt Hardware Actions Runner',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$PublicSourceRepositoryUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Universal'

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this script from an elevated interactive PowerShell window.'
    }
}

function Assert-InteractiveSession {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $sessionId = (Get-Process -Id $PID).SessionId
    if ($identity.IsSystem -or $identity.Name -eq 'NT AUTHORITY\SYSTEM' -or $sessionId -eq 0) {
        throw 'The hardware runner must run as a dedicated interactive Windows user. Session 0/service execution is forbidden for Word COM and visible-GUI evidence.'
    }
    return [ordered]@{ user = $identity.Name; session_id = $sessionId }
}

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = @($machine, $user) -join ';'
}

function Install-WingetPackage {
    param([Parameter(Mandatory = $true)] [string] $Id)
    if ($null -eq (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
        throw "WinGet is required to install $Id automatically. Install App Installer/WinGet first, or install prerequisites manually."
    }
    & winget.exe install --id $Id --exact --source winget --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
    if ($LASTEXITCODE -notin @(0, 3010)) {
        Write-Warning "WinGet returned $LASTEXITCODE while installing $Id; the bootstrap will verify the result."
    }
    Refresh-Path
}

function Get-VcBuildToolsInstallation {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return '' }
    return [string] (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
}

function Install-VcBuildTools {
    $bootstrapper = Join-Path $env:TEMP 'vs_BuildTools.exe'
    Invoke-WebRequest -UseBasicParsing -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $bootstrapper
    try {
        $process = Start-Process -FilePath $bootstrapper -ArgumentList @(
            '--quiet', '--wait', '--norestart', '--nocache',
            '--add', 'Microsoft.VisualStudio.Workload.VCTools',
            '--includeRecommended'
        ) -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) {
            throw "Visual Studio Build Tools installer failed with exit code $($process.ExitCode)."
        }
        if ($process.ExitCode -eq 3010) {
            Write-Warning 'Visual Studio Build Tools requested a reboot. Reboot before production hardware E2E.'
        }
    } finally {
        Remove-Item -LiteralPath $bootstrapper -Force -ErrorAction SilentlyContinue
    }
    if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) {
        throw 'Visual Studio Build Tools C++ workload is unavailable after installation.'
    }
}

function Ensure-OpenSsl {
    if ($null -ne (Get-Command openssl.exe -ErrorAction SilentlyContinue)) { return }
    $git = Get-Command git.exe -ErrorAction SilentlyContinue
    if ($null -eq $git) { throw 'Git for Windows is required before OpenSSL can be exposed.' }
    $gitRoot = Split-Path (Split-Path $git.Source -Parent) -Parent
    $opensslDir = Join-Path $gitRoot 'usr\bin'
    if (-not (Test-Path -LiteralPath (Join-Path $opensslDir 'openssl.exe') -PathType Leaf)) {
        throw 'OpenSSL was not found in the Git for Windows installation.'
    }
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $parts = @($machinePath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($parts -notcontains $opensslDir) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $opensslDir) -join ';'), 'Machine')
    }
    Refresh-Path
    if ($null -eq (Get-Command openssl.exe -ErrorAction SilentlyContinue)) {
        throw 'OpenSSL is still unavailable on PATH.'
    }
}

function Assert-WordCom {
    $wordPath = ''
    try {
        $wordPath = [string] (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Winword.exe' -ErrorAction Stop).'(default)'
    } catch {
        $command = Get-Command winword.exe -ErrorAction SilentlyContinue
        if ($null -ne $command) { $wordPath = $command.Source }
    }
    if ([string]::IsNullOrWhiteSpace($wordPath) -or -not (Test-Path -LiteralPath $wordPath -PathType Leaf)) {
        throw 'Licensed desktop Microsoft Word is not installed for this runner user.'
    }

    $word = $null
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        $version = [string] $word.Version
        if ([string]::IsNullOrWhiteSpace($version)) { throw 'Word COM returned an empty version.' }
        return [ordered]@{ path = $wordPath; version = $version }
    } finally {
        if ($null -ne $word) {
            try { $word.Quit() } catch { }
            [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null
        }
    }
}

function Assert-RealPrinter {
    param([Parameter(Mandatory = $true)] [string] $Name)
    if ($Name -match '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)') {
        throw "Printer '$Name' is a virtual/document printer. Hardware E2E requires a dedicated real printer queue."
    }
    $printer = Get-Printer -Name $Name -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace([string] $printer.PortName)) { throw "Printer '$Name' has no port." }
    $port = Get-PrinterPort -Name $printer.PortName -ErrorAction Stop
    wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
    return [ordered]@{
        name = $printer.Name
        driver = $printer.DriverName
        port = $printer.PortName
        port_description = $port.Description
    }
}

function Assert-SidecarManifest {
    param([Parameter(Mandatory = $true)] [string] $Path)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw 'SidecarManifestPath must be absolute.' }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw 'SidecarManifestPath must be a direct regular file, not a directory or reparse point.'
    }
    $manifest = Get-Content -LiteralPath $item.FullName -Raw | ConvertFrom-Json
    if ([int] $manifest.schema -ne 1 -or [string] $manifest.target -ne 'windows-x86_64') {
        throw 'Runner-owned sidecar manifest must use schema=1 and target=windows-x86_64.'
    }
    if ($manifest.supply_chain_locked -ne $true) { throw 'Runner-owned sidecar manifest must be supply_chain_locked=true.' }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) { throw 'Runner-owned sidecar manifest has no files.' }
    return $item.FullName
}

function Get-RunnerAsset {
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
        name = [string] $asset.name
        url = [string] $asset.browser_download_url
        sha256 = $Matches[1].ToLowerInvariant()
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

$RepositoryUrl = $RepositoryUrl.Trim().TrimEnd('/')
if ($RepositoryUrl -ieq $PublicSourceRepositoryUrl) {
    throw 'Refusing to register a persistent hardware runner in the public Dokkomplekt_Universal repository. Use the dedicated private hardware-validation repository.'
}
if ($RepositoryUrl -notmatch '^https://github\.com/[^/]+/[^/]+$') {
    throw 'RepositoryUrl must be a canonical GitHub repository URL.'
}
if ($RegistrationToken -notmatch '^[A-Za-z0-9_-]{20,}$') { throw 'RegistrationToken does not look valid.' }
if ([string]::IsNullOrWhiteSpace($RunnerName)) { $RunnerName = "dokkomplekt-hardware-$env:COMPUTERNAME" }

$runnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
if ($runnerServices.Count -gt 0) {
    throw 'An Actions runner Windows service is installed. Remove it: this hardware runner must execute in an interactive user session, never as a Windows service.'
}

if ($InstallPrerequisites) {
    if ($null -eq (Get-Command git.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage -Id 'Git.Git' }
    if ($null -eq (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage -Id 'Microsoft.PowerShell' }
    if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { Install-VcBuildTools }
}
Refresh-Path

foreach ($required in @('git.exe', 'pwsh.exe')) {
    if ($null -eq (Get-Command $required -ErrorAction SilentlyContinue)) { throw "$required is required." }
}
if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { throw 'Visual Studio Build Tools C++ workload is required.' }
Ensure-OpenSsl
$word = Assert-WordCom
$printer = Assert-RealPrinter -Name $PrinterName
$sidecarManifest = Assert-SidecarManifest -Path $SidecarManifestPath

$webView = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
if ($webView.Count -eq 0) { throw 'Microsoft Edge WebView2 Runtime is required.' }

powercfg /change standby-timeout-ac 0 | Out-Null
powercfg /change hibernate-timeout-ac 0 | Out-Null
New-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -PropertyType DWord -Value 1 -Force | Out-Null
& git.exe config --system core.longpaths true

if (Test-Path -LiteralPath $RunnerRoot) {
    if (@(Get-ChildItem -LiteralPath $RunnerRoot -Force).Count -gt 0) {
        throw "RunnerRoot '$RunnerRoot' is not empty. Use a clean directory so stale credentials/config cannot be reused."
    }
} else {
    New-Item -ItemType Directory -Force -Path $RunnerRoot | Out-Null
}

$asset = Get-RunnerAsset
$zip = Join-Path $env:TEMP $asset.name
Invoke-WebRequest -UseBasicParsing -Uri $asset.url -OutFile $zip
try {
    $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $asset.sha256) {
        throw "GitHub runner package SHA-256 mismatch: expected $($asset.sha256), got $actual"
    }
    Expand-Archive -LiteralPath $zip -DestinationPath $RunnerRoot -Force
} finally {
    Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
}

Push-Location $RunnerRoot
try {
    & .\config.cmd --unattended --url $RepositoryUrl --token $RegistrationToken --name $RunnerName --labels $RunnerLabel --work _work --replace
    if ($LASTEXITCODE -ne 0) { throw "GitHub runner config.cmd failed with exit code $LASTEXITCODE." }
} finally {
    Pop-Location
}

Register-InteractiveRunnerTask -TaskName $RunnerTaskName -Root $RunnerRoot -UserName $interactive.user
Start-Sleep -Seconds 4
$listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $interactive.session_id })
if ($listener.Count -eq 0) { throw 'Runner.Listener did not start in the interactive session. Inspect the runner _diag directory and scheduled task history.' }

$evidenceRoot = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$evidencePath = Join-Path $evidenceRoot 'HARDWARE_RUNNER_BOOTSTRAP.json'
[ordered]@{
    schema = 'dokkomplekt.hardware-runner-bootstrap.v2'
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    computer = $env:COMPUTERNAME
    user = $interactive.user
    session_id = $interactive.session_id
    repository_url = $RepositoryUrl
    public_source_repository_forbidden = $PublicSourceRepositoryUrl
    runner_name = $RunnerName
    runner_label = $RunnerLabel
    runner_root = (Resolve-Path -LiteralPath $RunnerRoot).Path
    scheduled_task = $RunnerTaskName
    service_mode_forbidden = $true
    runner_package = $asset.name
    runner_package_sha256 = $actual
    word_path = $word.path
    word_version = $word.version
    printer = $printer
    sidecar_manifest_path = $sidecarManifest
    sidecar_manifest_sha256 = (Get-FileHash -LiteralPath $sidecarManifest -Algorithm SHA256).Hash.ToLowerInvariant()
    webview2_path = $webView[0].FullName
    visual_studio_vctools = (Get-VcBuildToolsInstallation)
    powershell7 = (Get-Command pwsh.exe).Source
    git = (Get-Command git.exe).Source
    openssl = (Get-Command openssl.exe).Source
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $evidencePath -Encoding utf8

Write-Host "DOKKOMPLEKT PRIVATE HARDWARE RUNNER BOOTSTRAPPED: $RunnerName"
Write-Host "Repository: $RepositoryUrl"
Write-Host "Evidence: $evidencePath"
Write-Host 'Keep the dedicated Windows user logged in. The runner is intentionally not installed as a Windows service.'
