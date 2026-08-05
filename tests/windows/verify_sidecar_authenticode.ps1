param(
    [Parameter(Mandatory = $true)] [string] $RuntimeRoot,
    [string] $OutputPath = 'verification/release/SIDECAR_AUTHENTICODE.json'
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $RuntimeRoot -ErrorAction Stop).Path
$portableExecutables = @(Get-ChildItem -LiteralPath $root -Recurse -File | Where-Object {
    $_.Extension.ToLowerInvariant() -in @('.exe', '.dll')
})
if ($portableExecutables.Count -eq 0) {
    throw "No Windows PE sidecars were found under $root"
}

$records = foreach ($file in $portableExecutables) {
    $signature = Get-AuthenticodeSignature -FilePath $file.FullName
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) {
        throw "Sidecar Authenticode signature is not valid: $($file.FullName) ($($signature.Status))"
    }
    $relative = [IO.Path]::GetRelativePath($root, $file.FullName).Replace('\\', '/')
    [ordered]@{
        path = $relative
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        size_bytes = $file.Length
        status = [string]$signature.Status
        signer_subject = $signature.SignerCertificate.Subject
        signer_thumbprint = $signature.SignerCertificate.Thumbprint
        signer_not_before_utc = $signature.SignerCertificate.NotBefore.ToUniversalTime().ToString('o')
        signer_not_after_utc = $signature.SignerCertificate.NotAfter.ToUniversalTime().ToString('o')
        timestamp_subject = if ($null -ne $signature.TimeStamperCertificate) { $signature.TimeStamperCertificate.Subject } else { $null }
        timestamp_thumbprint = if ($null -ne $signature.TimeStamperCertificate) { $signature.TimeStamperCertificate.Thumbprint } else { $null }
    }
}

$payload = [ordered]@{
    schema = 'dokkomplekt.sidecar-authenticode.v1'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    runtime_root = $root
    result = 'passed'
    signed_pe_count = $records.Count
    files = @($records | Sort-Object path)
}
$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$payload | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Host "SIDECAR AUTHENTICODE PASSED: files=$($records.Count); evidence=$OutputPath"
