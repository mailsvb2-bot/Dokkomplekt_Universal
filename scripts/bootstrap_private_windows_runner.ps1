[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('runtime', 'hardware')]
    [string] $Role,
    [Parameter(Mandatory = $true)] [string] $RepositoryUrl,
    [Parameter(Mandatory = $true)] [string] $RegistrationToken,
    [string] $PrinterName = '',
    [string] $SidecarManifestPath = '',
    [string] $RuntimeRoot = 'C:\ProgramData\DokkomplektRuntime',
    [string] $RunnerRoot = '',
    [string] $RunnerName = '',
    [string] $RunnerTaskName = '',
    [switch] $InstallPrerequisites
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$PrivateRepositoryUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation'
$PublicRepositoryUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Universal'
$ExpectedRuntimeRoot = 'C:\ProgramData\DokkomplektRuntime'
$RuntimeServiceAccount = 'NT AUTHORITY\NETWORK SERVICE'
$RuntimeServiceSid = 'S-1-5-20'
$RuntimeAclEvidencePath = 'C:\ProgramData\DokkomplektE2E\RUNTIME_SERVICE_ACL.json'
$RunnerLabel = if ($Role -eq 'runtime') { 'dokkomplekt-runtime' } else { 'dokkomplekt-hardware' }
if ([string]::IsNullOrWhiteSpace($RunnerRoot)) { $RunnerRoot = if ($Role -eq 'runtime') { 'C:\actions-runner-runtime' } else { 'C:\actions-runner-hardware' } }
if ([string]::IsNullOrWhiteSpace($RunnerTaskName)) { $RunnerTaskName = if ($Role -eq 'runtime') { 'Dokkomplekt Runtime Actions Runner' } else { 'Dokkomplekt Hardware Actions Runner' } }
if ([string]::IsNullOrWhiteSpace($RunnerName)) { $RunnerName = "dokkomplekt-$Role-$env:COMPUTERNAME" }

function Assert-AdministratorAndInteractive {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run from an elevated PowerShell window.' }
    $sessionId = (Get-Process -Id $PID).SessionId
    if ($identity.IsSystem -or $identity.Name -eq 'NT AUTHORITY\SYSTEM' -or $sessionId -eq 0) { throw 'Runner bootstrap must be launched from an elevated interactive Windows session.' }
    return [ordered]@{ user = $identity.Name; session_id = $sessionId }
}

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = @($machine, $user) -join ';'
}

function Install-WingetPackage([string] $Id) {
    if ($null -eq (Get-Command winget.exe -ErrorAction SilentlyContinue)) { throw "WinGet is unavailable; install $Id manually." }
    & winget.exe install --id $Id --exact --source winget --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
    if ($LASTEXITCODE -notin @(0, 3010)) { throw "WinGet failed for $Id with exit code $LASTEXITCODE." }
    Refresh-Path
}

function Get-VcBuildToolsInstallation {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return '' }
    return [string] (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
}

function Install-VcBuildTools {
    $bootstrapper = Join-Path $env:TEMP ('vs_BuildTools-' + [Guid]::NewGuid().ToString('N') + '.exe')
    Invoke-WebRequest -UseBasicParsing -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $bootstrapper
    try {
        $p = Start-Process -FilePath $bootstrapper -ArgumentList @('--quiet','--wait','--norestart','--nocache','--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended') -Wait -PassThru
        if ($p.ExitCode -notin @(0, 3010)) { throw "Visual Studio Build Tools failed with exit code $($p.ExitCode)." }
        if ($p.ExitCode -eq 3010) { Write-Warning 'Visual Studio Build Tools requested a reboot.' }
    } finally {
        Remove-Item -LiteralPath $bootstrapper -Force -ErrorAction SilentlyContinue
    }
}

function Ensure-OpenSslFromGit {
    if ($null -ne (Get-Command openssl.exe -ErrorAction SilentlyContinue)) { return }
    $git = Get-Command git.exe -ErrorAction Stop
    $gitRoot = Split-Path (Split-Path $git.Source -Parent) -Parent
    $opensslDir = Join-Path $gitRoot 'usr\bin'
    if (-not (Test-Path -LiteralPath (Join-Path $opensslDir 'openssl.exe') -PathType Leaf)) { throw 'OpenSSL was not found inside Git for Windows.' }
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $parts = @($machinePath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($parts -notcontains $opensslDir) { [Environment]::SetEnvironmentVariable('Path', (($parts + $opensslDir) -join ';'), 'Machine') }
    Refresh-Path
    if ($null -eq (Get-Command openssl.exe -ErrorAction SilentlyContinue)) { throw 'OpenSSL remains unavailable.' }
}

function Get-RuleSid($Rule) {
    try { return $Rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } catch { return '' }
}

function Assert-RuntimeHost {
    if ([IO.Path]::GetFullPath($RuntimeRoot).TrimEnd('\') -ine [IO.Path]::GetFullPath($ExpectedRuntimeRoot).TrimEnd('\')) { throw "Production RuntimeRoot is fixed to $ExpectedRuntimeRoot" }
    if ([string]::IsNullOrWhiteSpace($SidecarManifestPath)) { throw 'SidecarManifestPath is required for Role=runtime.' }
    if (-not [IO.Path]::IsPathFullyQualified($SidecarManifestPath)) { throw 'SidecarManifestPath must be absolute.' }
    $item = Get-Item -LiteralPath $SidecarManifestPath -Force -ErrorAction Stop
    if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw 'Runtime manifest must be a direct regular file.' }
    $root = (Get-Item -LiteralPath $RuntimeRoot -Force -ErrorAction Stop).FullName.TrimEnd('\')
    $manifestFull = $item.FullName
    if ($manifestFull -ine $root -and -not $manifestFull.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Runtime manifest must remain under $ExpectedRuntimeRoot" }
    $manifest = Get-Content -LiteralPath $item.FullName -Raw | ConvertFrom-Json
    if ([int] $manifest.schema -ne 1 -or [string] $manifest.target -ne 'windows-x86_64' -or $manifest.supply_chain_locked -ne $true) { throw 'Runtime manifest must be schema=1, target=windows-x86_64, supply_chain_locked=true.' }
    if ($null -eq $manifest.files -or @($manifest.files).Count -eq 0) { throw 'Runtime manifest has no files.' }
    if (-not (Test-Path -LiteralPath ($item.FullName + '.sig') -PathType Leaf)) { throw 'Offline approval signature beside the runtime manifest is required.' }
    if (-not (Test-Path -LiteralPath $RuntimeAclEvidencePath -PathType Leaf)) { throw "Runtime service ACL evidence is missing: $RuntimeAclEvidencePath. Use register_windows_runtime_runner.ps1 instead of bypassing the secure registration entrypoint." }
    $aclEvidence = Get-Content -LiteralPath $RuntimeAclEvidencePath -Raw | ConvertFrom-Json
    if ([string]$aclEvidence.schema -ne 'dokkomplekt.runtime-service-acl.v2') { throw 'Runtime service ACL evidence schema mismatch.' }
    if ([string]$aclEvidence.service_sid -ne $RuntimeServiceSid -or [string]$aclEvidence.access -ne 'ReadAndExecute' -or $aclEvidence.recursive_acl_applied -ne $true) { throw 'Runtime service ACL evidence does not prove bounded Network Service ReadAndExecute.' }
    if ([IO.Path]::GetFullPath([string]$aclEvidence.runtime_root).TrimEnd('\') -ine $root) { throw 'Runtime service ACL root mismatch.' }
    if ([IO.Path]::GetFullPath([string]$aclEvidence.manifest_path) -ine [IO.Path]::GetFullPath($manifestFull)) { throw 'Runtime service ACL manifest mismatch.' }
    $acl = Get-Acl -LiteralPath $root
    $rules = @($acl.Access | Where-Object { (Get-RuleSid $_) -eq $RuntimeServiceSid -and ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::ReadAndExecute) })
    if ($rules.Count -eq 0) { throw 'Runtime root no longer grants Network Service SID ReadAndExecute.' }
    if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { throw 'Visual Studio C++ Build Tools are required on the runtime/signing host.' }
    Ensure-OpenSslFromGit
}

function Assert-HardwareHost {
    if (-not [string]::IsNullOrWhiteSpace($SidecarManifestPath)) { throw 'Role=hardware must not accept a runtime manifest.' }
    if ([string]::IsNullOrWhiteSpace($PrinterName)) { throw 'PrinterName is required for Role=hardware.' }
    if ($PrinterName -match '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)') { throw 'A real printer queue is required.' }
    $printer = Get-Printer -Name $PrinterName -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace([string] $printer.PortName)) { throw 'Configured printer has no port.' }
    Get-PrinterPort -Name $printer.PortName -ErrorAction Stop | Out-Null
    wevtutil sl Microsoft-Windows-PrintService/Operational /e:true | Out-Null
    $word = $null
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        if ([string]::IsNullOrWhiteSpace([string] $word.Version)) { throw 'Word COM returned no version.' }
    } finally {
        if ($null -ne $word) {
            try { $word.Quit() } catch { }
            [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null
        }
    }
    $webView = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
    if ($webView.Count -eq 0) { throw 'Microsoft Edge WebView2 Runtime is required.' }
    foreach ($forbidden in @('DOKKOMPLEKT_SIDECAR_MANIFEST_PATH','DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64','DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD','DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64','DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64','DOKKOMPLEKT_GATE_PRIVATE_KEY_B64')) {
        if (-not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($forbidden))) { throw "Forbidden production value is exposed to the hardware host: $forbidden" }
    }
}

function Get-RunnerAsset {
    $headers = @{ Accept='application/vnd.github+json'; 'X-GitHub-Api-Version'='2022-11-28'; 'User-Agent'='Dokkomplekt-Private-Runner-Bootstrap' }
    $release = Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/actions/runner/releases/latest'
    $asset = @($release.assets | Where-Object { $_.name -match '^actions-runner-win-x64-[0-9.]+\.zip$' }) | Select-Object -First 1
    if ($null -eq $asset) { throw 'GitHub Actions runner win-x64 asset not found.' }
    $digest = [string] $asset.digest
    if ($digest -notmatch '^sha256:([0-9a-fA-F]{64})$') { throw 'Runner release has no usable SHA-256 digest.' }
    return [ordered]@{ name=[string]$asset.name; url=[string]$asset.browser_download_url; sha256=$Matches[1].ToLowerInvariant() }
}

function Register-InteractiveTask([string] $UserName) {
    $runCmd = Join-Path $RunnerRoot 'run.cmd'
    $action = New-ScheduledTaskAction -Execute $env:ComSpec -Argument "/d /s /c `"$runCmd`"" -WorkingDirectory $RunnerRoot
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $UserName
    $principal = New-ScheduledTaskPrincipal -UserId $UserName -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RestartCount 10 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero)
    Register-ScheduledTask -TaskName $RunnerTaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    Start-ScheduledTask -TaskName $RunnerTaskName
}

$RepositoryUrl = $RepositoryUrl.Trim().TrimEnd('/')
if ($RepositoryUrl -ieq $PublicRepositoryUrl) { throw 'Refusing to register a production runner in the public source repository.' }
if ($RepositoryUrl -ine $PrivateRepositoryUrl) { throw "Production runners may register only to $PrivateRepositoryUrl" }
if ($RegistrationToken -match '\s' -or $RegistrationToken.Length -lt 20) { throw 'RegistrationToken does not look valid.' }
$interactive = Assert-AdministratorAndInteractive
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 is required.' }

$runnerServices = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
if ($Role -eq 'hardware' -and $runnerServices.Count -gt 0) { throw 'Remove Actions runner Windows services from the hardware host; Word/printer validation must remain interactive.' }
if ($Role -eq 'runtime' -and $runnerServices.Count -gt 0) { throw 'Runtime runner service already exists. Use a clean runtime host/root or remove the stale runner before re-registration.' }

if ($InstallPrerequisites) {
    if ($null -eq (Get-Command git.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage 'Git.Git' }
    if ($null -eq (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { Install-WingetPackage 'Microsoft.PowerShell' }
    if ($Role -eq 'runtime' -and [string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { Install-VcBuildTools }
    if ($Role -eq 'hardware') {
        $webView = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
        if ($webView.Count -eq 0) { Install-WingetPackage 'Microsoft.EdgeWebView2Runtime' }
    }
}
Refresh-Path
foreach ($required in @('git.exe','pwsh.exe')) { if ($null -eq (Get-Command $required -ErrorAction SilentlyContinue)) { throw "$required is required." } }
if ($Role -eq 'runtime') { Assert-RuntimeHost } else { Assert-HardwareHost }

if (Test-Path -LiteralPath $RunnerRoot) {
    if (@(Get-ChildItem -LiteralPath $RunnerRoot -Force).Count -gt 0) { throw "Runner root must be empty: $RunnerRoot" }
} else { New-Item -ItemType Directory -Force -Path $RunnerRoot | Out-Null }

$asset = Get-RunnerAsset
$zip = Join-Path $env:TEMP $asset.name
Invoke-WebRequest -UseBasicParsing -Uri $asset.url -OutFile $zip
try {
    $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $asset.sha256) { throw "Runner package SHA-256 mismatch: expected $($asset.sha256), got $actual" }
    Expand-Archive -LiteralPath $zip -DestinationPath $RunnerRoot -Force
} finally { Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue }

Push-Location $RunnerRoot
try {
    if ($Role -eq 'runtime') {
        & .\config.cmd --unattended --url $RepositoryUrl --token $RegistrationToken --name $RunnerName --labels $RunnerLabel --work _work --replace --runasservice --windowslogonaccount $RuntimeServiceAccount
    } else {
        & .\config.cmd --unattended --url $RepositoryUrl --token $RegistrationToken --name $RunnerName --labels $RunnerLabel --work _work --replace
    }
    if ($LASTEXITCODE -ne 0) { throw "config.cmd failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }

$executionMode = ''
$runtimeServiceName = ''
if ($Role -eq 'runtime') {
    Start-Sleep -Seconds 3
    $services = @(Get-Service -Name 'actions.runner.*' -ErrorAction SilentlyContinue)
    if ($services.Count -ne 1) { throw "Expected exactly one Actions runner service after runtime registration, found $($services.Count)." }
    $runtimeServiceName = $services[0].Name
    if ($services[0].Status -ne 'Running') { Start-Service -Name $runtimeServiceName }
    Start-Sleep -Seconds 2
    if ((Get-Service -Name $runtimeServiceName).Status -ne 'Running') { throw 'Runtime Actions runner service is not running.' }
    $executionMode = 'windows-service-network-service'
} else {
    Register-InteractiveTask -UserName $interactive.user
    Start-Sleep -Seconds 4
    $listener = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $interactive.session_id })
    if ($listener.Count -eq 0) { throw 'Runner.Listener did not start in the interactive session.' }
    $executionMode = 'interactive-at-logon'
}

$evidenceRoot = Join-Path $env:ProgramData 'DokkomplektE2E'
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$evidencePath = Join-Path $evidenceRoot ("RUNNER_BOOTSTRAP_$($Role.ToUpperInvariant()).json")
[ordered]@{
    schema='dokkomplekt.private-runner-bootstrap.v2'
    created_at_utc=[DateTime]::UtcNow.ToString('o')
    role=$Role
    computer=$env:COMPUTERNAME
    bootstrap_user=$interactive.user
    bootstrap_session_id=$interactive.session_id
    repository_url=$RepositoryUrl
    public_repository_forbidden=$PublicRepositoryUrl
    runner_name=$RunnerName
    runner_label=$RunnerLabel
    runner_root=$RunnerRoot
    execution_mode=$executionMode
    task_name=if ($Role -eq 'hardware') { $RunnerTaskName } else { '' }
    service_name=if ($Role -eq 'runtime') { $runtimeServiceName } else { '' }
    service_account=if ($Role -eq 'runtime') { $RuntimeServiceAccount } else { '' }
    service_sid=if ($Role -eq 'runtime') { $RuntimeServiceSid } else { '' }
    runtime_root=if ($Role -eq 'runtime') { $ExpectedRuntimeRoot } else { '' }
    runtime_acl_evidence=if ($Role -eq 'runtime') { $RuntimeAclEvidencePath } else { '' }
    sidecar_manifest_used=if ($Role -eq 'runtime') { $SidecarManifestPath } else { '' }
    printer_used=if ($Role -eq 'hardware') { $PrinterName } else { '' }
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $evidencePath -Encoding utf8
Write-Host "PRIVATE RUNNER BOOTSTRAP PASS: role=$Role label=$RunnerLabel mode=$executionMode name=$RunnerName"
