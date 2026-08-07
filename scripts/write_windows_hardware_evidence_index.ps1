param(
    [Parameter(Mandatory = $true)] [string] $InstallerRoot,
    [Parameter(Mandatory = $true)] [string] $RuntimeRoot,
    [Parameter(Mandatory = $true)] [string] $ApplicationPath,
    [string] $VerificationRoot = 'verification/release',
    [string] $CargoGateRoot = '.cargo-gate',
    [string] $ReleaseGateRoot = '.release-gate',
    [string] $OutputPath = '.release-gate/WINDOWS_HARDWARE_EVIDENCE_INDEX.json'
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path '.').Path

function Resolve-RequiredFile {
    param([Parameter(Mandatory = $true)] [string] $Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required hardware evidence file is missing: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Get-RelativeRepositoryPath {
    param([Parameter(Mandatory = $true)] [string] $Path)
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $repoPrefix = $repoRoot.TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($repoPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Evidence path escapes repository workspace: $resolved"
    }
    return $resolved.Substring($repoPrefix.Length).Replace('\', '/')
}

function Get-FileRecord {
    param(
        [Parameter(Mandatory = $true)] [string] $Path,
        [Parameter(Mandatory = $true)] [string] $Kind
    )
    $resolved = Resolve-RequiredFile $Path
    $file = Get-Item -LiteralPath $resolved -ErrorAction Stop
    return [pscustomobject][ordered]@{
        kind = $Kind
        path = Get-RelativeRepositoryPath $resolved
        sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
        size_bytes = [long] $file.Length
    }
}

function Read-RequiredJson {
    param(
        [Parameter(Mandatory = $true)] [string] $Path,
        [Parameter(Mandatory = $true)] [string] $ExpectedSchema
    )
    $resolved = Resolve-RequiredFile $Path
    try {
        $payload = Get-Content -LiteralPath $resolved -Raw -Encoding utf8 | ConvertFrom-Json
    } catch {
        throw "Invalid JSON evidence '$Path': $($_.Exception.Message)"
    }
    if ([string] $payload.schema -ne $ExpectedSchema) {
        throw "Unexpected schema in '$Path': expected $ExpectedSchema, got $($payload.schema)"
    }
    return $payload
}

function Assert-Sha256Equal {
    param(
        [Parameter(Mandatory = $true)] [string] $Actual,
        [Parameter(Mandatory = $true)] [string] $Expected,
        [Parameter(Mandatory = $true)] [string] $Label
    )
    if ($Actual.ToLowerInvariant() -ne $Expected.ToLowerInvariant()) {
        throw "$Label SHA-256 mismatch: expected $Expected, got $Actual"
    }
}

$releaseSha = [string] $env:GITHUB_SHA
if ($releaseSha -notmatch '^[0-9a-f]{40}$') {
    throw 'GITHUB_SHA must be the exact lowercase release commit SHA.'
}
$sourceSha256 = (python scripts/source_fingerprint.py).Trim()
if ($LASTEXITCODE -ne 0 -or $sourceSha256 -notmatch '^[0-9a-f]{64}$') {
    throw 'Unable to calculate the release source fingerprint.'
}

$signedBuildPath = Join-Path $ReleaseGateRoot 'WINDOWS_SIGNED_BUILD_PASSED.json'
$hardwarePath = Join-Path $ReleaseGateRoot 'WINDOWS_HARDWARE_E2E_PASSED.json'
$guiPath = Join-Path $ReleaseGateRoot 'GUI_AND_CONSOLE_EVIDENCE.json'
$printPath = Join-Path $ReleaseGateRoot 'PRINT_EVENT_307.json'
$authenticodePath = Join-Path $ReleaseGateRoot 'AUTHENTICODE_SIGNATURES.json'
$rebootPath = Join-Path $ReleaseGateRoot 'WINDOWS_REBOOT_E2E_PASSED.json'
$watcherInstallPath = Join-Path $ReleaseGateRoot 'WATCHER_INSTALL.json'
$watcherUninstallPath = Join-Path $ReleaseGateRoot 'WATCHER_UNINSTALL.json'
$cargoAttestationPath = Join-Path $CargoGateRoot 'CARGO_GATE_ATTESTATION.json'
$cargoSignaturePath = Join-Path $CargoGateRoot 'CARGO_GATE_ATTESTATION.sig'

$signedBuild = Read-RequiredJson $signedBuildPath 'dokkomplekt.windows-signed-build.v1'
$hardware = Read-RequiredJson $hardwarePath 'dokkomplekt.windows-hardware-e2e.v3'
$gui = Read-RequiredJson $guiPath 'dokkomplekt.gui-console-evidence.v1'
$authenticode = Read-RequiredJson $authenticodePath 'dokkomplekt.authenticode-evidence.v1'
$reboot = Read-RequiredJson $rebootPath 'dokkomplekt.windows-reboot-e2e.verified.v2'

if ([string] $signedBuild.source_sha256 -ne $sourceSha256) {
    throw 'Signed build evidence is not bound to the current source fingerprint.'
}
if ([string] $hardware.source_sha256 -ne $sourceSha256) {
    throw 'Hardware E2E evidence is not bound to the current source fingerprint.'
}
if ([string] $reboot.source_sha256 -ne $sourceSha256) {
    throw 'Reboot evidence is not bound to the current source fingerprint.'
}
$requiredTrueFlags = @(
    'word_available',
    'watcher_autostart_found',
    'application_restart_passed',
    'gui_window_observed',
    'operating_system_reboot_tested',
    'watcher_started_after_reboot',
    'post_reboot_case_completed',
    'print_spooler_completion_observed',
    'installed_application_signature_valid',
    'silent_uninstall_passed'
)
foreach ($flag in $requiredTrueFlags) {
    if ([string] $hardware.$flag -ne 'True') {
        throw "Hardware E2E required flag is not true: $flag"
    }
}
if ([string] $hardware.unexpected_console_windows_observed -ne 'False') {
    throw 'Hardware E2E observed an unexpected console window.'
}
$guiLaunches = @($gui.launches)
if ($guiLaunches.Count -ne 2 -or
    @($guiLaunches | Where-Object { [string]::IsNullOrWhiteSpace([string] $_.visible_window_title) }).Count -ne 0 -or
    @($guiLaunches | Where-Object { @($_.unexpected_visible_console_windows).Count -ne 0 }).Count -ne 0) {
    throw 'GUI evidence must contain two titled launches with no unexpected console windows.'
}

$appRecord = Get-FileRecord $ApplicationPath 'application'
Assert-Sha256Equal $appRecord.sha256 ([string] $signedBuild.application.sha256) 'Signed application'
Assert-Sha256Equal $appRecord.sha256 ([string] $gui.application_sha256) 'GUI evidence application'
Assert-Sha256Equal $appRecord.sha256 ([string] $authenticode.installed_application.sha256) 'Hardware Authenticode application'
Assert-Sha256Equal $appRecord.sha256 ([string] $reboot.application_sha256) 'Reboot evidence application'
Assert-Sha256Equal $appRecord.sha256 ([string] $reboot.watcher_executable_sha256) 'Reboot watcher executable'

$guiRecord = Get-FileRecord $guiPath 'hardware-evidence'
$printRecord = Get-FileRecord $printPath 'hardware-evidence'
$authenticodeRecord = Get-FileRecord $authenticodePath 'hardware-evidence'
Assert-Sha256Equal $guiRecord.sha256 ([string] $hardware.gui_console_evidence_sha256) 'GUI/console evidence'
Assert-Sha256Equal $printRecord.sha256 ([string] $hardware.print_event_evidence_sha256) 'Print event evidence'
Assert-Sha256Equal $authenticodeRecord.sha256 ([string] $hardware.authenticode_evidence_sha256) 'Authenticode evidence'
$rebootEvidenceSha256 = (Get-FileHash -LiteralPath $rebootPath -Algorithm SHA256).Hash.ToLowerInvariant()

$installerFiles = @(Get-ChildItem -LiteralPath $InstallerRoot -Recurse -File | Where-Object { $_.Extension -in @('.exe', '.msi') })
if ($installerFiles.Count -eq 0) { throw 'No installer artifacts were found for the final evidence index.' }
$installerRecords = @($installerFiles | ForEach-Object { Get-FileRecord $_.FullName 'installer' })
$signedInstallerHashes = @($signedBuild.installers | ForEach-Object { ([string] $_.sha256).ToLowerInvariant() })
foreach ($installerRecord in $installerRecords) {
    if ($signedInstallerHashes -notcontains $installerRecord.sha256) {
        throw "Installer is absent from signed build evidence: $($installerRecord.path)"
    }
}
if ($installerRecords.sha256 -notcontains ([string] $hardware.installer_sha256).ToLowerInvariant()) {
    throw 'Hardware E2E installer SHA-256 does not match the indexed signed installer set.'
}

$runtimeFiles = @(Get-ChildItem -LiteralPath $RuntimeRoot -File -Filter '*.zip')
if ($runtimeFiles.Count -ne 1) {
    throw "Expected exactly one offline runtime ZIP, found $($runtimeFiles.Count)."
}
$runtime = $runtimeFiles[0]
$runtimeRecord = Get-FileRecord $runtime.FullName 'offline-runtime'
Assert-Sha256Equal $runtimeRecord.sha256 ([string] $signedBuild.offline_runtime.sha256) 'Offline runtime'
$runtimePayloadRecord = Get-FileRecord "$($runtime.FullName).signing.json" 'offline-runtime-signing'
$runtimeSignatureRecord = Get-FileRecord "$($runtime.FullName).signing.json.sig" 'offline-runtime-signature'
$runtimePublicKeyRecord = Get-FileRecord "$($runtime.FullName).signing.json.public.pem" 'offline-runtime-public-key'
$trustedKeyRecord = Get-FileRecord (Join-Path $VerificationRoot 'runtime-trusted-public.pem') 'trusted-public-key'
Assert-Sha256Equal $runtimeSignatureRecord.sha256 ([string] $signedBuild.offline_runtime.signature_sha256) 'Offline runtime signature'
Assert-Sha256Equal $runtimePublicKeyRecord.sha256 ([string] $signedBuild.offline_runtime.public_key_sha256) 'Offline runtime public key'
Assert-Sha256Equal $trustedKeyRecord.sha256 ([string] $signedBuild.offline_runtime.trusted_public_key_sha256) 'Pinned runtime public key'

$cargoAttestationRecord = Get-FileRecord $cargoAttestationPath 'rust-gate-evidence'
$cargoSignatureRecord = Get-FileRecord $cargoSignaturePath 'rust-gate-signature'
Assert-Sha256Equal $cargoAttestationRecord.sha256 ([string] $signedBuild.rust_gate_attestation_sha256) 'Rust gate attestation'
Assert-Sha256Equal $cargoSignatureRecord.sha256 ([string] $signedBuild.rust_gate_signature_sha256) 'Rust gate signature'

$requiredEvidence = @(
    @{ Path = $signedBuildPath; Kind = 'release-evidence' },
    @{ Path = $hardwarePath; Kind = 'hardware-evidence' },
    @{ Path = $rebootPath; Kind = 'hardware-evidence' },
    @{ Path = $watcherInstallPath; Kind = 'hardware-evidence' },
    @{ Path = $watcherUninstallPath; Kind = 'hardware-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'production-build-preflight.json'); Kind = 'preflight-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'windows-runtime-preflight.json'); Kind = 'preflight-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'hardware-preflight.json'); Kind = 'preflight-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'sidecar-status.json'); Kind = 'runtime-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'SIDECAR_AUTHENTICODE.json'); Kind = 'runtime-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'offline-runtime-probe.log'); Kind = 'runtime-evidence' },
    @{ Path = (Join-Path $VerificationRoot 'scanned-pdf-ocr.json'); Kind = 'ocr-evidence' }
)
$evidenceRecords = @($requiredEvidence | ForEach-Object { Get-FileRecord $_.Path $_.Kind })

$allRecords = @(
    $appRecord
    $installerRecords
    $runtimeRecord
    $runtimePayloadRecord
    $runtimeSignatureRecord
    $runtimePublicKeyRecord
    $trustedKeyRecord
    $cargoAttestationRecord
    $cargoSignatureRecord
    $guiRecord
    $printRecord
    $authenticodeRecord
    $evidenceRecords
) | Sort-Object path
$duplicates = @($allRecords | Group-Object path | Where-Object { $_.Count -ne 1 })
if ($duplicates.Count -gt 0) {
    throw "Duplicate paths in final hardware evidence index: $(($duplicates.Name) -join ', ')"
}

$index = [ordered]@{
    schema = 'dokkomplekt.windows-hardware-evidence-index.v1'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    release_sha = $releaseSha
    source_sha256 = $sourceSha256
    signed_build_evidence_sha256 = (Get-FileHash -LiteralPath $signedBuildPath -Algorithm SHA256).Hash.ToLowerInvariant()
    hardware_e2e_evidence_sha256 = (Get-FileHash -LiteralPath $hardwarePath -Algorithm SHA256).Hash.ToLowerInvariant()
    record_count = $allRecords.Count
    records = $allRecords
}
$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$index | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Read-RequiredJson $OutputPath 'dokkomplekt.windows-hardware-evidence-index.v1' | Out-Null
Write-Host "WINDOWS HARDWARE EVIDENCE INDEX: $OutputPath; records=$($allRecords.Count); release_sha=$releaseSha"
