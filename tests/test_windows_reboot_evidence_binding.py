from pathlib import Path
import shutil
import subprocess

import pytest

ROOT = Path(__file__).resolve().parents[1]
PREPARE = ROOT / "tests/windows/prepare_reboot_evidence.ps1"
VERIFY = ROOT / "tests/windows/verify_reboot_evidence.ps1"


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_reboot_evidence_v2_binds_exact_runtime_payload_and_output() -> None:
    prepare = text(PREPARE)
    verify = text(VERIFY)

    assert "dokkomplekt.windows-reboot-e2e.pending.v2" in prepare
    assert "dokkomplekt.windows-reboot-e2e.v2" in prepare
    assert "dokkomplekt.windows-reboot-e2e.verified.v2" in verify
    assert "[Guid]::NewGuid().ToString('N')" in prepare
    assert "application_sha256" in prepare
    assert "powershell_sha256" in prepare
    assert "post_script_sha256" in prepare
    assert "payload_sha256" in prepare
    assert "watcher_executable_sha256" in prepare
    assert "destination_sha256" in prepare
    assert "archive_receipt_sha256" in prepare
    assert "archived_source_sha256" in prepare
    assert "`$candidateSha -ne `$expectedPayloadSha256" in prepare
    assert "post_reboot_output_last_write_utc" in prepare
    assert "baselinePaths" in prepare
    assert "LastWriteTimeUtc -ge `$caseStartedAt.AddSeconds(-2)" in prepare
    assert "(?i)(?:^|\\s)--background-watch(?:\\s|$)" in prepare
    assert "EvidencePath must not already exist" in prepare
    assert "} finally {" in prepare
    assert "Unregister-ScheduledTask -TaskName '$taskName'" in prepare
    assert "`$temporaryEvidence = `$evidencePath + '.' + `$nonce + '.tmp'" in prepare

    assert "Pending reboot plan" in verify
    assert "Require-DirectFile" in verify
    assert "not a reparse point" in verify
    assert "Raw reboot evidence does not match the current Windows boot" in verify
    assert "Reboot nonce mismatch" in verify
    assert "Watcher executable hash mismatch" in verify
    assert "Payload is not bound to the prepared source document" in verify
    assert "Archive receipt is not bound to the prepared payload" in verify
    assert "Post-reboot output is the archived input payload" in verify
    assert "Post-reboot output is outside the prepared watch folder" in verify
    assert "Post-reboot output aliases the input payload" in verify
    assert "Require-FileHash $outputPath $outputSha 'Post-reboot output'" in verify
    assert "Post-reboot output timestamp mismatch" in verify
    assert "Remove-Item -LiteralPath $pending.post_script_path, $pending.payload_path, $pendingPath" in verify


def test_reboot_evidence_scripts_parse_with_powershell() -> None:
    pwsh = shutil.which("pwsh")
    if pwsh is None:
        pytest.skip("PowerShell is not installed in this development environment")
    prepare_text = PREPARE.read_text(encoding="utf-8")
    generated = prepare_text.split('@"', 1)[1].split('"@ | Set-Content', 1)[0]
    generated = generated.replace("`$", "$")
    generated_path = ROOT / "verification" / "ci" / "generated-post-reboot-parser.ps1"
    generated_path.parent.mkdir(parents=True, exist_ok=True)
    generated_path.write_text(generated, encoding="utf-8")
    parser_script = r"""
$paths = @(
  'tests/windows/prepare_reboot_evidence.ps1',
  'tests/windows/verify_reboot_evidence.ps1',
  'verification/ci/generated-post-reboot-parser.ps1'
)
foreach ($path in $paths) {
  $tokens = $null
  $errors = $null
  [System.Management.Automation.Language.Parser]::ParseFile(
    (Resolve-Path $path),
    [ref]$tokens,
    [ref]$errors
  ) | Out-Null
  if ($errors.Count -gt 0) {
    $details = ($errors | ForEach-Object { $_.Message }) -join '; '
    throw "PowerShell parse failed for ${path}: ${details}"
  }
}
"""
    result = subprocess.run(
        [pwsh, "-NoProfile", "-NonInteractive", "-Command", parser_script],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    try:
        assert result.returncode == 0, result.stdout + result.stderr
    finally:
        generated_path.unlink(missing_ok=True)
