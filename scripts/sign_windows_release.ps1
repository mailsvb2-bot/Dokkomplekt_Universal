[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $ArtifactRoot,
    [string] $TimestampServer = $env:DOKKOMPLEKT_TIMESTAMP_SERVER,
    [string] $SigningBackend = $env:DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-SigningTargets {
    param([Parameter(Mandatory = $true)] [string] $Root)

    $resolved = [IO.Path]::GetFullPath($Root)
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "Signing target does not exist: $resolved"
    }
    if (Test-Path -LiteralPath $resolved -PathType Leaf) {
        $item = Get-Item -LiteralPath $resolved
        return @($item | Where-Object { $_.Extension -in @('.exe', '.msi') })
    }
    return @(Get-ChildItem -LiteralPath $resolved -Recurse -File | Where-Object {
        $_.Extension -in @('.exe', '.msi')
    })
}

function Get-CertificatePrivateKeyInfo {
    param([Parameter(Mandatory = $true)] $Certificate)

    if (-not $Certificate.HasPrivateKey) {
        throw 'Configured signing certificate has no private key.'
    }

    $rsa = [Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($Certificate)
    if ($null -eq $rsa) {
        throw 'Production Windows signing requires an RSA code-signing private key.'
    }

    try {
        if ($rsa -is [Security.Cryptography.RSACng]) {
            $key = $rsa.Key
            $provider = [string] $key.Provider.Provider
            $exportPolicy = $key.ExportPolicy
            $exportable = (($exportPolicy -band [Security.Cryptography.CngExportPolicies]::AllowExport) -ne 0) -or
                          (($exportPolicy -band [Security.Cryptography.CngExportPolicies]::AllowPlaintextExport) -ne 0)
            return [ordered]@{
                provider = $provider
                hardware_device = $null
                exportable = [bool] $exportable
                implementation = 'CNG'
            }
        }

        if ($rsa -is [Security.Cryptography.RSACryptoServiceProvider]) {
            $info = $rsa.CspKeyContainerInfo
            return [ordered]@{
                provider = [string] $info.ProviderName
                hardware_device = [bool] $info.HardwareDevice
                exportable = [bool] $info.Exportable
                implementation = 'CAPI'
            }
        }

        throw "Unsupported RSA private-key implementation: $($rsa.GetType().FullName)"
    } finally {
        $rsa.Dispose()
    }
}

function Assert-HardwareBackedCertificate {
    param([Parameter(Mandatory = $true)] $Certificate)

    $expectedProvider = [string] $env:DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER
    if ([string]::IsNullOrWhiteSpace($expectedProvider)) {
        throw 'DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER is required for certificate-store signing.'
    }

    $info = Get-CertificatePrivateKeyInfo -Certificate $Certificate
    if ([string]::IsNullOrWhiteSpace([string] $info.provider)) {
        throw 'Signing private-key provider could not be identified.'
    }
    if (-not ([string] $info.provider).Equals($expectedProvider.Trim(), [StringComparison]::OrdinalIgnoreCase)) {
        throw "Signing private-key provider mismatch: expected '$($expectedProvider.Trim())', got '$($info.provider)'."
    }

    $softwareProviders = @(
        'Microsoft Software Key Storage Provider',
        'Microsoft Enhanced RSA and AES Cryptographic Provider',
        'Microsoft Enhanced Cryptographic Provider v1.0',
        'Microsoft Base Cryptographic Provider v1.0'
    )
    if ($softwareProviders -contains [string] $info.provider) {
        throw "Software-backed private-key provider is forbidden for production signing: $($info.provider)"
    }
    if ([bool] $info.exportable) {
        throw 'Production signing private key is exportable; a non-exportable hardware/HSM-backed key is required.'
    }
    if ($info.implementation -eq 'CAPI' -and $info.hardware_device -ne $true) {
        throw "CAPI signing provider does not report a hardware device: $($info.provider)"
    }

    Write-Host "SIGNING KEY BOUNDARY VERIFIED: provider=$($info.provider); implementation=$($info.implementation); exportable=$($info.exportable)"
}

function Resolve-CertificateStoreCertificate {
    $thumbprint = [string] $env:DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT
    if ([string]::IsNullOrWhiteSpace($thumbprint)) {
        throw 'DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT is required for certificate-store signing.'
    }
    $normalized = ($thumbprint -replace '\s', '').ToUpperInvariant()
    if ($normalized -notmatch '^[0-9A-F]{40,128}$') {
        throw 'DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT has an invalid format.'
    }

    $matches = @(Get-ChildItem Cert:\CurrentUser\My | Where-Object {
        (($_.Thumbprint -replace '\s', '').ToUpperInvariant()) -eq $normalized
    })
    if ($matches.Count -ne 1) {
        throw "Expected exactly one signing certificate with thumbprint $normalized in Cert:\CurrentUser\My; found $($matches.Count)."
    }
    $cert = $matches[0]
    if (-not $cert.HasPrivateKey) {
        throw 'Configured signing certificate has no accessible private key.'
    }
    Assert-HardwareBackedCertificate -Certificate $cert
    return $cert
}

function Import-LegacyPfxCertificate {
    if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64)) {
        throw 'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64 is required for the legacy pfx backend.'
    }
    if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD)) {
        throw 'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD is required for the legacy pfx backend.'
    }
    if ($env:DOKKOMPLEKT_RELEASE_MODE -eq 'production') {
        throw 'The legacy pfx backend is forbidden in production. Use a non-exportable certificate-store/HSM signing key.'
    }

    $pfxPath = Join-Path ([IO.Path]::GetTempPath()) ("dokkomplekt-signing-{0}.pfx" -f [guid]::NewGuid())
    try {
        [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64))
        $secure = ConvertTo-SecureString $env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD -AsPlainText -Force
        $cert = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $secure
        if ($null -eq $cert) { throw 'PFX import returned no certificate.' }
        return $cert
    } finally {
        Remove-Item -LiteralPath $pfxPath -Force -ErrorAction SilentlyContinue
    }
}

$targets = @(Get-SigningTargets -Root $ArtifactRoot)
if ($targets.Count -eq 0) { throw "No Windows installer/binary was found under $ArtifactRoot" }

$backend = ([string] $SigningBackend).Trim().ToLowerInvariant()
if ([string]::IsNullOrWhiteSpace($backend)) {
    throw 'DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND must be explicitly set to certificate-store or pfx.'
}
if ($backend -notin @('certificate-store', 'pfx')) {
    throw "Unsupported Windows signing backend: $backend"
}

$cert = $null
$removeImportedCertificate = $false
try {
    if ($backend -eq 'certificate-store') {
        $cert = Resolve-CertificateStoreCertificate
    } else {
        $cert = Import-LegacyPfxCertificate
        $removeImportedCertificate = $true
    }

    foreach ($target in $targets) {
        $arguments = @{
            FilePath = $target.FullName
            Certificate = $cert
            HashAlgorithm = 'SHA256'
        }
        if (-not [string]::IsNullOrWhiteSpace($TimestampServer)) {
            $arguments['TimestampServer'] = $TimestampServer
        }
        $result = Set-AuthenticodeSignature @arguments
        if ($result.Status -ne 'Valid') {
            throw "Authenticode signing failed for $($target.FullName): $($result.Status) $($result.StatusMessage)"
        }
    }

    foreach ($target in $targets) {
        $verification = Get-AuthenticodeSignature -FilePath $target.FullName
        if ($verification.Status -ne 'Valid') {
            throw "Authenticode verification failed for $($target.FullName): $($verification.Status)"
        }
        $actualThumbprint = ($verification.SignerCertificate.Thumbprint -replace '\s', '').ToUpperInvariant()
        $expectedThumbprint = ($cert.Thumbprint -replace '\s', '').ToUpperInvariant()
        if ($actualThumbprint -ne $expectedThumbprint) {
            throw "Authenticode signer mismatch for $($target.FullName)."
        }
        Write-Host "SIGNED: $($target.FullName); backend=$backend; thumbprint=$actualThumbprint"
    }
} finally {
    if ($removeImportedCertificate -and $null -ne $cert) {
        Remove-Item -LiteralPath ("Cert:\CurrentUser\My\{0}" -f $cert.Thumbprint) -Force -ErrorAction SilentlyContinue
    }
}
