[CmdletBinding()]
param(
    [string] $PrinterName = '',
    [string] $RebootSourceDocument = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$PrivateRepositoryUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation'
$RunnerSettingsUrl = 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation/settings/actions/runners/new?arch=x64&os=win'
$RunnerRoot = 'C:\actions-runner-hardware'
$RunnerTaskName = 'Dokkomplekt Hardware Actions Runner'
$ConfigRoot = 'C:\ProgramData\DokkomplektE2E'
$ConfigPath = Join-Path $ConfigRoot 'hardware-runner.json'
$RebootEvidencePath = Join-Path $ConfigRoot 'WINDOWS_REBOOT_E2E_RAW.json'

function Assert-AdministratorAndInteractive {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'CMD bootstrap must run elevated. Re-run SETUP_HARDWARE_RUNNER.cmd and accept the UAC prompt.'
    }
    $sessionId = (Get-Process -Id $PID).SessionId
    if ($identity.IsSystem -or $sessionId -eq 0) {
        throw 'Hardware runner setup requires an interactive Windows desktop session.'
    }
    return [ordered]@{ user = $identity.Name; session_id = $sessionId }
}

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = @($machine, $user) -join ';'
}

function Get-GitHubReleaseAsset {
    param(
        [Parameter(Mandatory = $true)] [string] $Repository,
        [Parameter(Mandatory = $true)] [string] $NamePattern
    )
    $headers = @{
        Accept = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
        'User-Agent' = 'Dokkomplekt-Hardware-Runner-Setup'
    }
    $release = Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "https://api.github.com/repos/$Repository/releases/latest"
    $asset = @($release.assets | Where-Object { [string]$_.name -match $NamePattern }) | Select-Object -First 1
    if ($null -eq $asset) { throw "Release asset not found for $Repository / $NamePattern" }
    $digest = [string]$asset.digest
    if ($digest -notmatch '^sha256:([0-9a-fA-F]{64})$') {
        throw "GitHub release asset $($asset.name) has no SHA-256 digest; refusing an unpinned prerequisite download."
    }
    return [ordered]@{
        name = [string]$asset.name
        url = [string]$asset.browser_download_url
        sha256 = $Matches[1].ToLowerInvariant()
    }
}

function Download-VerifiedGitHubAsset {
    param(
        [Parameter(Mandatory = $true)] $Asset,
        [Parameter(Mandatory = $true)] [string] $Destination
    )
    Invoke-WebRequest -UseBasicParsing -Uri $Asset.url -OutFile $Destination
    $actual = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Asset.sha256) {
        Remove-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue
        throw "Downloaded prerequisite SHA-256 mismatch for $($Asset.name)."
    }
}

function Assert-MicrosoftSignedFile {
    param([Parameter(Mandatory = $true)] [string] $Path)
    $signature = Get-AuthenticodeSignature -FilePath $Path
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch '(?i)Microsoft') {
        throw "Microsoft prerequisite has invalid Authenticode signature: $Path / $($signature.Status)"
    }
}

function Get-VcBuildToolsInstallation {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return '' }
    return [string] (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
}

function Ensure-Git {
    Refresh-Path
    if ($null -ne (Get-Command git.exe -ErrorAction SilentlyContinue)) { return }
    Write-Host '[SETUP] Git for Windows is missing; installing verified latest x64 release...'
    $asset = Get-GitHubReleaseAsset -Repository 'git-for-windows/git' -NamePattern '^Git-[0-9.]+-64-bit\.exe$'
    $installer = Join-Path $env:TEMP $asset.name
    try {
        Download-VerifiedGitHubAsset -Asset $asset -Destination $installer
        $process = Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT','/NORESTART','/NOCANCEL','/SP-') -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) { throw "Git installer failed with exit code $($process.ExitCode)." }
    } finally {
        Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
    }
    Refresh-Path
    if ($null -eq (Get-Command git.exe -ErrorAction SilentlyContinue)) { throw 'git.exe remains unavailable after installation.' }
}

function Ensure-PowerShell7 {
    Refresh-Path
    if ($null -ne (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { return }
    Write-Host '[SETUP] PowerShell 7 is missing; installing verified latest x64 MSI...'
    $asset = Get-GitHubReleaseAsset -Repository 'PowerShell/PowerShell' -NamePattern '^PowerShell-[0-9.]+-win-x64\.msi$'
    $installer = Join-Path $env:TEMP $asset.name
    try {
        Download-VerifiedGitHubAsset -Asset $asset -Destination $installer
        $process = Start-Process -FilePath msiexec.exe -ArgumentList @('/i', $installer, '/qn', '/norestart') -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) { throw "PowerShell 7 MSI failed with exit code $($process.ExitCode)." }
    } finally {
        Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
    }
    Refresh-Path
    if ($null -eq (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) { throw 'pwsh.exe remains unavailable after installation.' }
}

function Ensure-WebView2 {
    $existing = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
    if ($existing.Count -gt 0) { return }
    Write-Host '[SETUP] Microsoft Edge WebView2 Runtime is missing; installing Microsoft Evergreen Runtime...'
    $installer = Join-Path $env:TEMP ('MicrosoftEdgeWebview2Setup-' + [Guid]::NewGuid().ToString('N') + '.exe')
    try {
        Invoke-WebRequest -UseBasicParsing -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $installer
        Assert-MicrosoftSignedFile -Path $installer
        $process = Start-Process -FilePath $installer -ArgumentList @('/silent','/install') -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) { throw "WebView2 installer failed with exit code $($process.ExitCode)." }
    } finally {
        Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
    }
    $existing = @(Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application" -Recurse -File -Filter 'msedgewebview2.exe' -ErrorAction SilentlyContinue)
    if ($existing.Count -eq 0) { throw 'WebView2 Runtime remains unavailable after installation.' }
}

function Ensure-VcBuildTools {
    if (-not [string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { return }
    Write-Host '[SETUP] Visual Studio C++ Build Tools are missing; installing the required VC toolchain...'
    $installer = Join-Path $env:TEMP ('vs_BuildTools-' + [Guid]::NewGuid().ToString('N') + '.exe')
    try {
        Invoke-WebRequest -UseBasicParsing -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $installer
        Assert-MicrosoftSignedFile -Path $installer
        $process = Start-Process -FilePath $installer -ArgumentList @('--quiet','--wait','--norestart','--nocache','--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended') -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) { throw "Visual Studio Build Tools failed with exit code $($process.ExitCode)." }
        if ($process.ExitCode -eq 3010) { Write-Warning 'Visual Studio Build Tools requested a reboot. Finish setup now; reboot before the first Hardware E2E run.' }
    } finally {
        Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
    }
    if ([string]::IsNullOrWhiteSpace((Get-VcBuildToolsInstallation))) { throw 'Visual Studio C++ Build Tools remain unavailable after installation.' }
}

function Ensure-RustToolchain {
    Refresh-Path
    $cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue
    $rustup = Get-Command rustup.exe -ErrorAction SilentlyContinue
    if ($null -ne $cargo -and $null -ne $rustup) {
        & $rustup.Source toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy | Out-Null
        if ($LASTEXITCODE -eq 0) { return }
    }

    Write-Host '[SETUP] Rust toolchain is missing; installing verified Rust 1.97.1 through official rustup...'
    $rustupUrl = 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe'
    $shaUrl = "$rustupUrl.sha256"
    $installer = Join-Path $env:TEMP ('rustup-init-' + [Guid]::NewGuid().ToString('N') + '.exe')
    try {
        $shaText = [string](Invoke-WebRequest -UseBasicParsing -Uri $shaUrl).Content
        if ($shaText -notmatch '(?i)([0-9a-f]{64})') { throw 'Official rustup SHA-256 file did not contain a digest.' }
        $expected = $Matches[1].ToLowerInvariant()
        Invoke-WebRequest -UseBasicParsing -Uri $rustupUrl -OutFile $installer
        $actual = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) { throw "rustup-init SHA-256 mismatch: expected $expected got $actual" }
        $process = Start-Process -FilePath $installer -ArgumentList @('-y','--default-toolchain','1.97.1','--profile','minimal','--component','rustfmt','--component','clippy') -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "rustup-init failed with exit code $($process.ExitCode)." }
    } finally {
        Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
    }
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($parts -notcontains $cargoBin) { [Environment]::SetEnvironmentVariable('Path', (($parts + $cargoBin) -join ';'), 'User') }
    Refresh-Path
    if ($null -eq (Get-Command cargo.exe -ErrorAction SilentlyContinue)) { throw 'cargo.exe remains unavailable after rustup installation.' }
}

function Assert-WordCom {
    $word = $null
    try {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        if ([string]::IsNullOrWhiteSpace([string]$word.Version)) { throw 'Microsoft Word COM returned no version.' }
        Write-Host "[SETUP] Microsoft Word COM detected: version $($word.Version)"
    } catch {
        throw 'Desktop Microsoft Word is required on this PC and must be activatable through Word.Application COM.'
    } finally {
        if ($null -ne $word) {
            try { $word.Quit() } catch { }
            try { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null } catch { }
        }
    }
}

function Get-RealPrinterCandidates {
    $virtualPattern = '(?i)(Microsoft Print to PDF|Microsoft XPS|OneNote|Fax|PDFCreator|CutePDF)'
    $result = [System.Collections.Generic.List[object]]::new()
    foreach ($printer in @(Get-Printer -ErrorAction Stop)) {
        if ([string]::IsNullOrWhiteSpace([string]$printer.Name) -or [string]$printer.Name -match $virtualPattern) { continue }
        if ([string]::IsNullOrWhiteSpace([string]$printer.PortName)) { continue }
        try { Get-PrinterPort -Name $printer.PortName -ErrorAction Stop | Out-Null } catch { continue }
        $result.Add($printer)
    }
    return @($result)
}

function Resolve-Printer {
    param([string] $Requested, $PreviousConfig)
    $candidates = @(Get-RealPrinterCandidates)
    if ($candidates.Count -eq 0) { throw 'No real printer queue was found. Connect/install a physical printer and run the CMD again.' }

    $preferred = $Requested
    if ([string]::IsNullOrWhiteSpace($preferred) -and $null -ne $PreviousConfig) { $preferred = [string]$PreviousConfig.printer_name }
    if (-not [string]::IsNullOrWhiteSpace($preferred)) {
        $match = @($candidates | Where-Object { $_.Name -eq $preferred })
        if ($match.Count -eq 1) { return [string]$match[0].Name }
    }

    $defaultName = [string](Get-CimInstance Win32_Printer -ErrorAction SilentlyContinue | Where-Object { $_.Default -eq $true } | Select-Object -First 1 -ExpandProperty Name)
    if (-not [string]::IsNullOrWhiteSpace($defaultName)) {
        $defaultPrinter = @($candidates | Where-Object { $_.Name -eq $defaultName }) | Select-Object -First 1
        if ($null -ne $defaultPrinter) {
            Write-Host "[SETUP] Using default real printer: $($defaultPrinter.Name)"
            return [string]$defaultPrinter.Name
        }
    }
    if ($candidates.Count -eq 1) {
        Write-Host "[SETUP] Using the only real printer: $($candidates[0].Name)"
        return [string]$candidates[0].Name
    }

    Write-Host 'Available real printer queues:'
    for ($i = 0; $i -lt $candidates.Count; $i++) { Write-Host "  [$($i + 1)] $($candidates[$i].Name)" }
    while ($true) {
        $choice = Read-Host 'Printer number'
        $index = 0
        if ([int]::TryParse($choice, [ref]$index) -and $index -ge 1 -and $index -le $candidates.Count) {
            return [string]$candidates[$index - 1].Name
        }
        Write-Warning 'Enter one of the printer numbers shown above.'
    }
}

function Resolve-RebootSourceDocument {
    param([string] $Requested, $PreviousConfig)
    $candidate = $Requested
    if ([string]::IsNullOrWhiteSpace($candidate) -and $null -ne $PreviousConfig) { $candidate = [string]$PreviousConfig.reboot_source_document }
    if (-not [string]::IsNullOrWhiteSpace($candidate) -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        return (Get-Item -LiteralPath $candidate).FullName
    }

    Write-Host '[SETUP] Choose one real DOCX source/primary document for the reboot watcher test.'
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $dialog = New-Object System.Windows.Forms.OpenFileDialog
        $dialog.Title = 'Dokkomplekt Hardware E2E - choose a source DOCX'
        $dialog.Filter = 'Word documents (*.docx)|*.docx|All files (*.*)|*.*'
        $dialog.Multiselect = $false
        if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
            return (Get-Item -LiteralPath $dialog.FileName).FullName
        }
    } catch {
        Write-Warning "File picker unavailable: $($_.Exception.Message)"
    }
    $manual = Read-Host 'Full path to a source DOCX'
    if ([string]::IsNullOrWhiteSpace($manual) -or -not (Test-Path -LiteralPath $manual -PathType Leaf)) {
        throw 'A persistent source DOCX is required for the real reboot/watcher test.'
    }
    return (Get-Item -LiteralPath $manual).FullName
}

function Read-PreviousConfig {
    if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) { return $null }
    try {
        $config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
        if ([string]$config.schema -eq 'dokkomplekt.hardware-runner-local-config.v1') { return $config }
    } catch { }
    return $null
}

function Write-LocalConfig {
    param(
        [Parameter(Mandatory = $true)] [string] $SelectedPrinter,
        [Parameter(Mandatory = $true)] [string] $SelectedSource,
        [Parameter(Mandatory = $true)] $Interactive
    )
    New-Item -ItemType Directory -Force -Path $ConfigRoot | Out-Null
    [ordered]@{
        schema = 'dokkomplekt.hardware-runner-local-config.v1'
        updated_at_utc = [DateTime]::UtcNow.ToString('o')
        computer = $env:COMPUTERNAME
        user = $Interactive.user
        printer_name = $SelectedPrinter
        test_duplex = ''
        test_tray = ''
        reboot_evidence_path = $RebootEvidencePath
        reboot_source_document = $SelectedSource
        runner_root = $RunnerRoot
        runner_task_name = $RunnerTaskName
        repository_url = $PrivateRepositoryUrl
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ConfigPath -Encoding utf8
}

function Test-ExistingRunner {
    $runnerConfig = Join-Path $RunnerRoot '.runner'
    if (-not (Test-Path -LiteralPath $runnerConfig -PathType Leaf)) { return $false }
    $task = Get-ScheduledTask -TaskName $RunnerTaskName -ErrorAction SilentlyContinue
    if ($null -eq $task) { throw "Runner config exists but scheduled task '$RunnerTaskName' is missing. Remove $RunnerRoot and run setup again." }
    $raw = Get-Content -LiteralPath $runnerConfig -Raw
    if ($raw -notmatch 'Dokkomplekt_Hardware_Validation') { throw 'Existing runner is not bound to the private Dokkomplekt_Hardware_Validation repository.' }
    return $true
}

$interactive = Assert-AdministratorAndInteractive
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 is required.' }

Write-Host '=== Dokkomplekt single-PC Hardware E2E setup ==='
Ensure-Git
Ensure-PowerShell7
Ensure-WebView2
Ensure-VcBuildTools
Ensure-RustToolchain
Assert-WordCom

$previous = Read-PreviousConfig
$selectedPrinter = Resolve-Printer -Requested $PrinterName -PreviousConfig $previous
$selectedSource = Resolve-RebootSourceDocument -Requested $RebootSourceDocument -PreviousConfig $previous

if (Test-ExistingRunner) {
    Write-Host '[SETUP] Existing private hardware runner found; reusing it.'
    Write-LocalConfig -SelectedPrinter $selectedPrinter -SelectedSource $selectedSource -Interactive $interactive
    Start-ScheduledTask -TaskName $RunnerTaskName
    Start-Sleep -Seconds 4
    $listeners = @(Get-Process -Name 'Runner.Listener' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $interactive.session_id })
    if ($listeners.Count -eq 0) { throw 'Existing Runner.Listener did not start in the interactive session.' }
    Write-Host "HARDWARE RUNNER SETUP PASS: reused existing runner; printer=$selectedPrinter; config=$ConfigPath"
    exit 0
}

if (Test-Path -LiteralPath $RunnerRoot) {
    if (@(Get-ChildItem -LiteralPath $RunnerRoot -Force -ErrorAction SilentlyContinue).Count -gt 0) {
        throw "Runner root is non-empty but not configured: $RunnerRoot. Rename/remove that stale folder and rerun setup."
    }
}

Write-Host '[SETUP] Opening GitHub runner registration page in your browser...'
Start-Process $RunnerSettingsUrl
Write-Host 'On the GitHub page select Windows/x64 if needed. Copy ONLY the value after --token from the config command.'
Write-Host 'Return to this CMD window; the next prompt hides the token while you paste it.'

$registration = Join-Path $PSScriptRoot 'register_windows_hardware_evidence_runner.ps1'
$bootstrap = Join-Path $PSScriptRoot 'bootstrap_private_windows_runner.ps1'
if (-not (Test-Path -LiteralPath $registration -PathType Leaf) -or -not (Test-Path -LiteralPath $bootstrap -PathType Leaf)) {
    throw 'Canonical registration scripts were not downloaded beside this setup script.'
}

& $registration -PrinterName $selectedPrinter -InstallPrerequisites
if ($LASTEXITCODE -notin @(0, $null)) { throw "Canonical hardware runner registration failed with exit code $LASTEXITCODE." }

Write-LocalConfig -SelectedPrinter $selectedPrinter -SelectedSource $selectedSource -Interactive $interactive
Write-Host "HARDWARE RUNNER SETUP PASS: label=dokkomplekt-hardware; printer=$selectedPrinter; source=$selectedSource; config=$ConfigPath"
