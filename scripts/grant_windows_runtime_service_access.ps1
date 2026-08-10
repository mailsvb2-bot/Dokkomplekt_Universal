[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $ManifestPath,
    [string] $RuntimeRoot = 'C:\ProgramData\DokkomplektRuntime',
    [string] $ServiceSid = 'S-1-5-20',
    [string] $OutputPath = 'C:\ProgramData\DokkomplektE2E\RUNTIME_SERVICE_ACL.json'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-DirectPath {
    param([string] $Path, [string] $Label, [bool] $Directory = $false)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw "$Label must be an absolute path." }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "$Label must not be a reparse point: $Path" }
    if ($Directory) {
        if (-not $item.PSIsContainer) { throw "$Label must be a directory: $Path" }
    } elseif ($item.PSIsContainer) {
        throw "$Label must be a direct regular file: $Path"
    }
    return $item.FullName
}

function Assert-UnderRoot {
    param([string] $Path, [string] $Root, [string] $Label)
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $base = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    if ($full -ine $base -and -not $full.StartsWith($base + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label escapes the bounded runtime root '$base': $full"
    }
    return $full
}

if ($ServiceSid -ne 'S-1-5-20') { throw 'Runtime service SID is fixed to Windows Network Service S-1-5-20.' }
$serviceSecurityIdentifier = [Security.Principal.SecurityIdentifier]::new($ServiceSid)
$root = Resolve-DirectPath -Path $RuntimeRoot -Label 'RuntimeRoot' -Directory $true
$manifest = Resolve-DirectPath -Path $ManifestPath -Label 'ManifestPath'
Assert-UnderRoot -Path $manifest -Root $root -Label 'ManifestPath' | Out-Null
$signature = Resolve-DirectPath -Path ($manifest + '.sig') -Label 'Runtime lock approval signature'
Assert-UnderRoot -Path $signature -Root $root -Label 'Runtime lock approval signature' | Out-Null

$data = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
if ([int]$data.schema -ne 1 -or [string]$data.target -ne 'windows-x86_64' -or $data.supply_chain_locked -ne $true) {
    throw 'Runtime manifest must be schema=1, target=windows-x86_64 and supply_chain_locked=true.'
}
if ($null -eq $data.files -or @($data.files).Count -eq 0) { throw 'Runtime manifest file inventory is empty.' }

$checked = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$checked.Add($manifest) | Out-Null
$checked.Add($signature) | Out-Null
foreach ($entry in @($data.files)) {
    foreach ($field in @('source','license_file')) {
        $raw = [string]$entry.$field
        if ([string]::IsNullOrWhiteSpace($raw)) { throw "Runtime manifest entry is missing $field." }
        $path = Resolve-DirectPath -Path $raw -Label $field
        Assert-UnderRoot -Path $path -Root $root -Label $field | Out-Null
        $checked.Add($path) | Out-Null
    }
}
$review = $data.distribution_review
if ($null -eq $review -or [string]::IsNullOrWhiteSpace([string]$review.inventory_file)) { throw 'distribution_review.inventory_file is required.' }
$inventory = Resolve-DirectPath -Path ([string]$review.inventory_file) -Label 'distribution inventory'
Assert-UnderRoot -Path $inventory -Root $root -Label 'distribution inventory' | Out-Null
$checked.Add($inventory) | Out-Null

# Windows icacls accepts a well-known SID prefixed with '*'. The ACL mutation is
# bounded to one protected runtime root after every manifest-referenced path has
# been proven to remain inside it.
& icacls.exe $root /grant "*${ServiceSid}:(OI)(CI)(RX)" /T /C | Out-Null
if ($LASTEXITCODE -ne 0) { throw "icacls failed with exit code $LASTEXITCODE." }

$acl = Get-Acl -LiteralPath $root
$matchingRules = @($acl.Access | Where-Object {
    try {
        $sid = $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
        $sid -eq $ServiceSid -and ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::ReadAndExecute)
    } catch { $false }
})
if ($matchingRules.Count -eq 0) { throw "$ServiceSid did not receive ReadAndExecute on runtime root." }

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
[ordered]@{
    schema='dokkomplekt.runtime-service-acl.v2'
    created_at_utc=[DateTime]::UtcNow.ToString('o')
    runtime_root=$root
    manifest_path=$manifest
    service_sid=$ServiceSid
    service_account=$serviceSecurityIdentifier.Translate([Security.Principal.NTAccount]).Value
    access='ReadAndExecute'
    bounded_paths_verified=$checked.Count
    recursive_acl_applied=$true
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding utf8

Write-Host "RUNTIME SERVICE ACL PASSED: root=$root; sid=$ServiceSid; checked=$($checked.Count)"
