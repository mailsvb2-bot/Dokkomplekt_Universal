[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $RegistrationToken,
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $SidecarManifestPath,
    [string] $RunnerRoot = 'C:\actions-runner-runtime',
    [string] $RunnerName = '',
    [string] $RunnerLabel = 'dokkomplekt-runtime',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$ExpectedRepository = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation'

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run runtime runner bootstrap from an elevated PowerShell window.'
    }
}

function Refresh-Path {
    $env:Path = @(
        [Environment]::GetEnvironmentVariable('Path', 'Machine'),
        [Environment]::GetEnvironmentVariable('Path', 'User')
    ) -join ';'
}

function Install-WingetPackage([string] $Id) {
    if ($null -eq (Get-Command winget.exe -ErrorAction SilentlyContinue)) { throw "WinGet is required to install $Id automatically." }
    & winget.exe install --id $Id --exact --source winget --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
    if ($LASTEXITCODE -notin @(0, 3010)) { throw "WinGet failed to install $Id with exit code $LASTEXITCODE." }
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
            '--quiet','--wait','--norestart','--nocache',
            '--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended'
        ) -Wait -PassThru
        if ($process.ExitCode -notin @(0,3010)) { throw "Visual Studio Build Tools installer failed: $($process.ExitCode)" }
    } finally {
        Remove-Item -LiteralPath $bootstrapper -Force -ErrorAction SilentlyContinue
    }
}

function Ensure-OpenSsl {
    if ($null -ne (Get-Command openssl.exe -ErrorAction SilentlyContinue)) { return }
    $git = Get-Command git.exe -ErrorAction SilentlyContinue
    if ($null -eq $git) { throw 'Git for Windows is required before OpenSSL can be exposed.' }
    $gitRoot = Split-Path (Split-Path $git.Source -Parent) -Parent
    $opensslDir = Join-Path $gitRoot 'usr\bin'
    $opensslExe = Join-Path $opensslDir 'openssl.exe'
    if (-not (Test-Path -LiteralPath $opensslExe -PathType Leaf)) { throw 'OpenSSL was not found in Git for Windows.' }
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $parts = @($machinePath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($parts -notcontains $opensslDir) { [Environment]::SetEnvironmentVariable('Path', (($parts + $opensslDir) -join ';'), 'Machine') }
    Refresh-Path
    if ($null -eq (Get-Command openssl.exe -ErrorAction SilentlyContinue)) { throw 'OpenSSL is still unavailable on PATH.' }
}

function Assert-RuntimeManifest([string] $Path) {
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw 'SidecarManifestPath must be absolute.' }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'Runtime manifest must be a direct regular file.' }
    $manifest = Get-Content -LiteralPath $item.FullName -Raw | ConvertFrom-Json
    if ([int] $manifest.schema -ne 1 -or [string] $manifest.target -ne 'windows-x86_64' -or $manifest.supply_chain_locked -ne $true) {
        throw 'Runtime manifest must be schema=1, target=windows-x86_64 and supply_chain_locked=true.'
    }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) { throw 'Runtime manifest file inventory is empty.' }
    if (-not (Test-Path -LiteralPath "$($item.FullName).sig" -PathType Leaf)) { throw 'Offline runtime-lock approval signature is missing.' }
    return $item.FullName
}

function Get-MachineFingerprint {
    $machineGuid = [string] (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid -ErrorAction Stop).MachineGuid
    $bytes = [Text.Encoding]::UTF8.GetBytes($machineGuid.Trim().ToLowerInvariant())
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = $sha.ComputeHash($bytes) } finally { $sha.Dispose() }
    return ([BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
}

function Get-RunnerAsset {
    $headers = @{ Accept='application/vnd.github+json'; 'X-GitHub-Api-Version'='2022-11-28'; 'User-Agent'='Dokkomplekt-Runtime-Runner-Bootstrap' }
    $release = Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/actions/runner/releases/latest'
    $asset = @($release.assets | Where-Object { $_.name -match '^actions-runner-win-x64-[0-9.]+\.zip$' }) | Select-Object -First 1
    if ($null -eq $asset) { throw 'GitHub runner win-x64 release asset was not found.' }
    $digest = [string] $asset.digest
    if ($digest -notmatch '^sha256:([0-9a-fA-F]{64})$') { throw 'GitHub runner release asset has no usable SHA-256 digest.' }
    return [ordered]@{ name=[string]$asset.name; url=[string]$asset.browser_download_url; sha256=$Matches[1].ToLowerInvariant() }
}

Assert-Administrator
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 is required.' }
$RepositoryUrl = $RepositoryUrl.Trim().TrimEnd('/')
if ($RepositoryUrl -ine $ExpectedRepository) { throw "Runtime runner must be registered only in $ExpectedRepository" }
if ($RunnerLabel -ne 'dokkomplekt-runtime') { throw 'Runtime runner label is fixed to dokkomplekt-runtime.' }
if ($RegistrationToken -notmatch '^[A-Za-z0-9_-]{20,}$') { throw 'RegistrationToken does not look valid.' }
if ([string]::IsNullOrWhiteSpace($RunnerName)) { $RunnerName = "dokkomplekt-runtime-$env:COMPUTERNAME" }

$hardwareOnlyVars = @('DOKKOMPLEKT_TEST_PRINTER','DOKKOMPLEKT_TEST_DUPLEX','DOKKOMPLEKT_TEST_TRAY','DOKKOMPLEKT_REBOOT_EVIDENCE_PATH','DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT')
$exposed = @($hardwareOnlyVars | Where-Object { -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_, 'Process')) })
if ($exposed.Count -gt 0) { throw "Hardware-only environment must not be present on runtime runner: $($exposed -join ', ')" }

if ($InstallPrerequisites) {
    if ($null -eq (Get-Command git.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage 'Git.Git' }
    if ($null -eq (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage 'Microsoft.PowerShell' }
    if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { Install-VcBuildTools }
}
Refresh-Path
foreach ($required in @('git.exe','pwsh.exe')) { if ($null -eq (Get-Command $required -ErrorAction SilentlyContinue)) { throw "$required is required." } }
if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { throw 'Visual Studio Build Tools C++ workload is required.' }
Ensure-OpenSsl
$manifest = Assert-RuntimeManifest $SidecarManifestPath

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
    & .\config.cmd --unattended --url $RepositoryUrl --token $RegistrationToken --name $RunnerName --labels $RunnerLabel --work _work --replace --runasservice --windowslogonaccount 'NT AUTHORITY\NETWORK SERVICE'
    if ($LASTEXITCODE -ne 0) { throw "GitHub runner config.cmd failed with exit code $LASTEXITCODE." }
} finally { Pop-Location }

Start-Sleep -Seconds 3
$services = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
if ($services.Count -eq 0) { throw 'Actions runner service was not installed.' }
foreach ($service in $services | Where-Object { $_.Status -ne 'Running' }) { Start-Service -Name $service.Name }
Start-Sleep -Seconds 2
$services = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
if (@($services | Where-Object { $_.Status -eq 'Running' }).Count -eq 0) { throw 'Actions runner service is not running.' }

$evidenceRoot = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$evidencePath = Join-Path $evidenceRoot 'RUNTIME_RUNNER_BOOTSTRAP.json'
[ordered]@{
    schema='dokkomplekt.runtime-runner-bootstrap.v1'
    created_at_utc=[DateTime]::UtcNow.ToString('o')
    computer=$env:COMPUTERNAME
    machine_fingerprint_sha256=Get-MachineFingerprint
    repository_url=$RepositoryUrl
    runner_name=$RunnerName
    runner_label=$RunnerLabel
    runner_root=(Resolve-Path -LiteralPath $RunnerRoot).Path
    service_mode_required=$true
    runtime_manifest_path=$manifest
    runtime_manifest_sha256=(Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
    runtime_manifest_signature_path="$manifest.sig"
    runner_package=$asset.name
    runner_package_sha256=$actual
    visual_studio_vctools=Get-VcBuildToolsInstallation
    powershell7=(Get-Command pwsh.exe).Source
    git=(Get-Command git.exe).Source
    openssl=(Get-Command openssl.exe).Source
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $evidencePath -Encoding utf8

Write-Host "DOKKOMPLEKT RUNTIME/SIGNING RUNNER BOOTSTRAPPED: $RunnerName"
Write-Host "Evidence: $evidencePath"
