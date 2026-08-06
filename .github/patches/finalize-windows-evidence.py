from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_once(relative: str, old: str, new: str, label: str) -> None:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one {label} block in {relative}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def run(*args: str, allow_failure: bool = False) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        [sys.executable, *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0 and not allow_failure:
        raise SystemExit(completed.stdout + completed.stderr)
    return completed


replace_once(
    "scripts/write_windows_hardware_evidence_index.ps1",
    """    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if (-not $resolved.StartsWith($repoRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Evidence path escapes repository workspace: $resolved"
    }
    return $resolved.Substring($repoRoot.Length).TrimStart([char[]]@('\\', '/')).Replace('\\', '/')
""",
    """    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $repoPrefix = $repoRoot.TrimEnd([char[]]@('\\', '/')) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($repoPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Evidence path escapes repository workspace: $resolved"
    }
    return $resolved.Substring($repoPrefix.Length).Replace('\\', '/')
""",
    "repository boundary",
)

replace_once(
    "tests/windows/verify_reboot_evidence.ps1",
    """$archivedSourcePath = Normalize-PathValue $evidence.archived_source_path
$expectedArchivedPath = Join-Path (Split-Path -Parent $receiptPath) ([string] $receiptPayload.archived_name)
Require-PathEqual $archivedSourcePath $expectedArchivedPath 'Archived source'
""",
    """$archivedSourcePath = Normalize-PathValue $evidence.archived_source_path
$expectedArchivedPath = Normalize-PathValue (Join-Path (Split-Path -Parent $receiptPath) ([string] $receiptPayload.archived_name))
if (-not $expectedArchivedPath.StartsWith($watchPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Archived source is outside the prepared watch folder.'
}
Require-PathEqual $archivedSourcePath $expectedArchivedPath 'Archived source'
""",
    "archived source boundary",
)

replace_once(
    "tests/windows/windows_hardware_e2e.ps1",
    """$verifiedReboot = Get-Content -LiteralPath $verifiedRebootPath -Raw | ConvertFrom-Json

$finalCleanup = Start-Process""",
    """$verifiedReboot = Get-Content -LiteralPath $verifiedRebootPath -Raw | ConvertFrom-Json
$currentApplicationSha256 = (Get-FileHash -LiteralPath $app -Algorithm SHA256).Hash.ToLowerInvariant()
if ([string] $verifiedReboot.application_sha256 -ne $currentApplicationSha256) {
    throw 'Verified reboot evidence belongs to a different installed application binary.'
}
if ([string] $verifiedReboot.watcher_executable_sha256 -ne $currentApplicationSha256) {
    throw 'Verified reboot watcher belongs to a different installed application binary.'
}

$finalCleanup = Start-Process""",
    "current reboot application binding",
)
replace_once(
    "tests/windows/windows_hardware_e2e.ps1",
    """    post_reboot_output_sha256 = $verifiedReboot.post_reboot_output_sha256
    print_spooler_completion_observed = $true
""",
    """    post_reboot_output_sha256 = $verifiedReboot.post_reboot_output_sha256
    reboot_application_sha256 = $currentApplicationSha256
    reboot_evidence_sha256 = (Get-FileHash -LiteralPath $verifiedRebootPath -Algorithm SHA256).Hash.ToLowerInvariant()
    print_spooler_completion_observed = $true
""",
    "hardware reboot evidence fields",
)

replace_once(
    "scripts/write_windows_hardware_evidence_index.ps1",
    """$gui = Read-RequiredJson $guiPath 'dokkomplekt.gui-console-evidence.v1'
$authenticode = Read-RequiredJson $authenticodePath 'dokkomplekt.authenticode-evidence.v1'
""",
    """$gui = Read-RequiredJson $guiPath 'dokkomplekt.gui-console-evidence.v1'
$authenticode = Read-RequiredJson $authenticodePath 'dokkomplekt.authenticode-evidence.v1'
$reboot = Read-RequiredJson $rebootPath 'dokkomplekt.windows-reboot-e2e.verified.v2'
""",
    "reboot JSON loading",
)
replace_once(
    "scripts/write_windows_hardware_evidence_index.ps1",
    """if ([string] $hardware.source_sha256 -ne $sourceSha256) {
    throw 'Hardware E2E evidence is not bound to the current source fingerprint.'
}
$requiredTrueFlags""",
    """if ([string] $hardware.source_sha256 -ne $sourceSha256) {
    throw 'Hardware E2E evidence is not bound to the current source fingerprint.'
}
if ([string] $reboot.source_sha256 -ne $sourceSha256) {
    throw 'Reboot evidence is not bound to the current source fingerprint.'
}
$requiredTrueFlags""",
    "reboot source binding",
)
replace_once(
    "scripts/write_windows_hardware_evidence_index.ps1",
    """Assert-Sha256Equal $appRecord.sha256 ([string] $authenticode.installed_application.sha256) 'Hardware Authenticode application'

$guiRecord""",
    """Assert-Sha256Equal $appRecord.sha256 ([string] $authenticode.installed_application.sha256) 'Hardware Authenticode application'
Assert-Sha256Equal $appRecord.sha256 ([string] $reboot.application_sha256) 'Reboot evidence application'
Assert-Sha256Equal $appRecord.sha256 ([string] $reboot.watcher_executable_sha256) 'Reboot watcher executable'
Assert-Sha256Equal $appRecord.sha256 ([string] $hardware.reboot_application_sha256) 'Hardware reboot application'

$guiRecord""",
    "reboot application hashes",
)
replace_once(
    "scripts/write_windows_hardware_evidence_index.ps1",
    """Assert-Sha256Equal $authenticodeRecord.sha256 ([string] $hardware.authenticode_evidence_sha256) 'Authenticode evidence'

$installerFiles""",
    """Assert-Sha256Equal $authenticodeRecord.sha256 ([string] $hardware.authenticode_evidence_sha256) 'Authenticode evidence'
$rebootEvidenceSha256 = (Get-FileHash -LiteralPath $rebootPath -Algorithm SHA256).Hash.ToLowerInvariant()
Assert-Sha256Equal $rebootEvidenceSha256 ([string] $hardware.reboot_evidence_sha256) 'Reboot evidence file'

$installerFiles""",
    "reboot evidence file hash",
)

reboot_test = ROOT / "tests/test_windows_reboot_evidence_binding.py"
reboot_test_text = reboot_test.read_text(encoding="utf-8")
needle = '    assert "Archive receipt is not bound to the prepared payload" in verify\n'
addition = '    assert "Archived source is outside the prepared watch folder" in verify\n'
if addition not in reboot_test_text:
    if reboot_test_text.count(needle) != 1:
        raise SystemExit("expected reboot assertion anchor")
    reboot_test.write_text(reboot_test_text.replace(needle, needle + addition, 1), encoding="utf-8")

index_test = ROOT / "tests/test_windows_hardware_evidence_index.py"
index_text = index_test.read_text(encoding="utf-8")
old_fixture = """        hardware_path = release_root / "WINDOWS_HARDWARE_E2E_PASSED.json"
        write_json(
            hardware_path,
"""
new_fixture = """        reboot_path = release_root / "WINDOWS_REBOOT_E2E_PASSED.json"
        write_json(
            reboot_path,
            {
                "schema": "dokkomplekt.windows-reboot-e2e.verified.v2",
                "source_sha256": source_sha,
                "application_sha256": sha256(application),
                "watcher_executable_sha256": sha256(application),
            },
        )
        hardware_path = release_root / "WINDOWS_HARDWARE_E2E_PASSED.json"
        write_json(
            hardware_path,
"""
if index_text.count(old_fixture) != 1:
    raise SystemExit("expected hardware fixture anchor")
index_text = index_text.replace(old_fixture, new_fixture, 1)
index_text = index_text.replace(
    '                "authenticode_evidence_sha256": sha256(authenticode_path),\n',
    '                "authenticode_evidence_sha256": sha256(authenticode_path),\n'
    '                "reboot_application_sha256": sha256(application),\n'
    '                "reboot_evidence_sha256": sha256(reboot_path),\n',
    1,
)
index_text = index_text.replace(
    '        for name in (\n            "WINDOWS_REBOOT_E2E_PASSED.json",\n            "WATCHER_INSTALL.json",\n',
    '        for name in (\n            "WATCHER_INSTALL.json",\n',
    1,
)
if "def test_repository_boundary_requires_separator() -> None:" not in index_text:
    index_text += """


def test_repository_boundary_requires_separator() -> None:
    source = SCRIPT.read_text(encoding="utf-8")
    assert "$repoPrefix = $repoRoot.TrimEnd" in source
    assert "[IO.Path]::DirectorySeparatorChar" in source
    assert "$resolved.StartsWith($repoPrefix" in source
    assert "$resolved.StartsWith($repoRoot" not in source
    assert "Read-RequiredJson $rebootPath 'dokkomplekt.windows-reboot-e2e.verified.v2'" in source
    assert "Reboot evidence application" in source
    assert "Reboot watcher executable" in source
    assert "Reboot evidence file" in source
"""
index_test.write_text(index_text, encoding="utf-8")

for temporary in (
    ROOT / ".github/workflows/refresh-gui-evidence-manifest.yml",
    ROOT / ".github/patches/refresh-gui-manifest.trigger",
    Path(__file__).resolve(),
):
    temporary.unlink(missing_ok=True)

candidate = ROOT / "target/final-windows-evidence-source-manifest.txt"
candidate.parent.mkdir(parents=True, exist_ok=True)
run("scripts/verify_source_manifest.py", "--candidate", str(candidate), allow_failure=True)
if not candidate.is_file():
    raise SystemExit("source manifest generator did not create the candidate file")
(ROOT / "SOURCE_MANIFEST_SHA256.txt").write_bytes(candidate.read_bytes())
run("scripts/verify_source_manifest.py")
