param(
    [Parameter(Mandatory = $true)] [string] $ArtifactRoot,
    [string] $TimestampServer = $env:DOKKOMPLEKT_TIMESTAMP_SERVER
)
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64)) {
    throw 'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64 is required for a signed release.'
}
if ([string]::IsNullOrWhiteSpace($env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD)) {
    throw 'DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD is required for a signed release.'
}
$root = [IO.Path]::GetFullPath($ArtifactRoot)
if (-not (Test-Path -LiteralPath $root)) {
    throw "Signing target does not exist: $root"
}
if (Test-Path -LiteralPath $root -PathType Leaf) {
    $item = Get-Item -LiteralPath $root
    $targets = @($item | Where-Object { $_.Extension -in @('.exe', '.msi') })
} else {
    $targets = @(Get-ChildItem -LiteralPath $root -Recurse -File | Where-Object {
        $_.Extension -in @('.exe', '.msi')
    })
}
if ($targets.Count -eq 0) { throw "No Windows installer/binary was found under $root" }
$pfxPath = Join-Path ([IO.Path]::GetTempPath()) ("dokkomplekt-signing-{0}.pfx" -f [guid]::NewGuid())
$cert = $null
try {
    [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64))
    $secure = ConvertTo-SecureString $env:DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD -AsPlainText -Force
    $cert = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $secure -Exportable
    if ($null -eq $cert) { throw 'PFX import returned no certificate.' }
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
        Write-Host "SIGNED: $($target.FullName); thumbprint=$($verification.SignerCertificate.Thumbprint)"
    }
} finally {
    if ($null -ne $cert) {
        Remove-Item -LiteralPath ("Cert:\CurrentUser\My\{0}" -f $cert.Thumbprint) -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $pfxPath -Force -ErrorAction SilentlyContinue
}
