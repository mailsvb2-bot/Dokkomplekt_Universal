param(
    [Parameter(Mandatory = $true)] [string] $InstallerRoot,
    [Parameter(Mandatory = $true)] [string] $RuntimeRoot,
    [Parameter(Mandatory = $true)] [string] $ApplicationPath,
    [Parameter(Mandatory = $true)] [string] $RuntimeTrustedPublicKey,
    [string] $OutputPath = '.release-gate/WINDOWS_SIGNED_BUILD_PASSED.json'
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path '.cargo-gate/CARGO_GATE_ATTESTATION.json' -PathType Leaf) -or
    -not (Test-Path '.cargo-gate/CARGO_GATE_ATTESTATION.sig' -PathType Leaf)) {
    throw 'Signed Rust gate attestation is missing.'
}
python scripts/assert_release_ready.py
$installers = @(Get-ChildItem -LiteralPath $InstallerRoot -Recurse -File | Where-Object { $_.Extension -in @('.exe', '.msi') })
if ($installers.Count -eq 0) { throw 'No Windows installer artifact found.' }
$installerEvidence = foreach ($file in $installers) {
    $signature = Get-AuthenticodeSignature $file.FullName
    if ($signature.Status -ne 'Valid') { throw "Invalid signature: $($file.FullName)" }
    [ordered]@{
        name = $file.Name
        sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        size_bytes = $file.Length
        signer_thumbprint = $signature.SignerCertificate.Thumbprint
        signer_subject = $signature.SignerCertificate.Subject
    }
}
$runtime = Get-ChildItem -LiteralPath $RuntimeRoot -File -Filter '*.zip' | Select-Object -First 1
if ($null -eq $runtime) { throw 'Offline runtime ZIP not found.' }
$runtimePayload = "$($runtime.FullName).signing.json"
$runtimeSignature = "$runtimePayload.sig"
$runtimeApprovalSignature = "$runtimePayload.approval.sig"
if (-not (Test-Path -LiteralPath $runtimePayload -PathType Leaf)) { throw 'Offline runtime signing payload not found.' }
if (-not (Test-Path -LiteralPath $runtimeSignature -PathType Leaf)) { throw 'Offline runtime signature not found.' }
if (-not (Test-Path -LiteralPath $runtimeApprovalSignature -PathType Leaf)) { throw 'Offline runtime approval signature not found.' }
if (-not (Test-Path -LiteralPath $RuntimeTrustedPublicKey -PathType Leaf)) { throw 'Pinned runtime public key not found.' }
# The protected pinned key is the only runtime trust root. Do not trust or require
# an artifact-provided public key, which would re-introduce trust-on-first-use.
python scripts/verify_offline_runtime_bundle.py $runtime.FullName --payload $runtimePayload --signature $runtimeSignature --trusted-public-key $RuntimeTrustedPublicKey --require-signature
if ($LASTEXITCODE -ne 0) { throw 'Offline runtime verification against pinned public key failed.' }
$app = Get-Item -LiteralPath $ApplicationPath -ErrorAction Stop
$appSignature = Get-AuthenticodeSignature $app.FullName
if ($appSignature.Status -ne 'Valid') { throw "Application binary is not validly signed: $($app.FullName)" }
$identityOutput = @(python scripts/release_source_identity.py)
if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve checked-out public release identity.' }
try {
    $sourceIdentity = (($identityOutput -join "`n") | ConvertFrom-Json)
} catch {
    throw "Invalid release source identity JSON: $($_.Exception.Message)"
}
if ([string] $sourceIdentity.schema -ne 'dokkomplekt.release-source-identity.v1' -or
    [string] $sourceIdentity.source_repository -ne 'mailsvb2-bot/Dokkomplekt_Universal' -or
    [string] $sourceIdentity.release_sha -notmatch '^[0-9a-f]{40}$') {
    throw 'Release source identity is not canonical.'
}
$sourceFingerprint = (python scripts/source_fingerprint.py).Trim()
$trustedKeySha256 = (Get-FileHash -LiteralPath $RuntimeTrustedPublicKey -Algorithm SHA256).Hash.ToLowerInvariant()
$evidence = [ordered]@{
    schema = 'dokkomplekt.windows-signed-build.v1'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    version = (Get-Content VERSION -Raw).Trim()
    source_repository = [string] $sourceIdentity.source_repository
    release_sha = [string] $sourceIdentity.release_sha
    source_sha256 = $sourceFingerprint
    rust_gate_attestation_sha256 = (Get-FileHash '.cargo-gate/CARGO_GATE_ATTESTATION.json' -Algorithm SHA256).Hash.ToLowerInvariant()
    rust_gate_signature_sha256 = (Get-FileHash '.cargo-gate/CARGO_GATE_ATTESTATION.sig' -Algorithm SHA256).Hash.ToLowerInvariant()
    application = [ordered]@{
        name = $app.Name
        sha256 = (Get-FileHash $app.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        size_bytes = $app.Length
        signer_thumbprint = $appSignature.SignerCertificate.Thumbprint
        signer_subject = $appSignature.SignerCertificate.Subject
    }
    installers = @($installerEvidence)
    offline_runtime = [ordered]@{
        name = $runtime.Name
        sha256 = (Get-FileHash $runtime.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        signature_sha256 = (Get-FileHash $runtimeSignature -Algorithm SHA256).Hash.ToLowerInvariant()
        approval_signature_sha256 = (Get-FileHash $runtimeApprovalSignature -Algorithm SHA256).Hash.ToLowerInvariant()
        public_key_sha256 = $trustedKeySha256
        trusted_public_key_sha256 = $trustedKeySha256
        trust_source = 'protected_pinned_public_key'
    }
    hardware_e2e = 'not_executed_in_signed_build_job'
}
$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
$evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Host "WINDOWS SIGNED BUILD EVIDENCE: $OutputPath; release=$($sourceIdentity.source_repository)@$($sourceIdentity.release_sha)"
