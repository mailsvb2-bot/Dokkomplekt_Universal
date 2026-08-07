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


# The GUI/print/Authenticode evidence records are already added explicitly to
# allRecords after their hashes are cross-checked. Do not add the same paths a
# second time through requiredEvidence.
replace_once(
    "scripts/write_windows_hardware_evidence_index.ps1",
    """$requiredEvidence = @(
    @{ Path = $signedBuildPath; Kind = 'release-evidence' },
    @{ Path = $hardwarePath; Kind = 'hardware-evidence' },
    @{ Path = $guiPath; Kind = 'hardware-evidence' },
    @{ Path = $printPath; Kind = 'hardware-evidence' },
    @{ Path = $authenticodePath; Kind = 'hardware-evidence' },
    @{ Path = $rebootPath; Kind = 'hardware-evidence' },
""",
    """$requiredEvidence = @(
    @{ Path = $signedBuildPath; Kind = 'release-evidence' },
    @{ Path = $hardwarePath; Kind = 'hardware-evidence' },
    @{ Path = $rebootPath; Kind = 'hardware-evidence' },
""",
    "deduplicated hardware evidence list",
)

replace_once(
    "tests/test_windows_hardware_evidence_index.py",
    '        assert index["record_count"] >= 25\n',
    '        assert index["record_count"] == 24\n',
    "deterministic hardware evidence record count",
)

# PowerShell variable names are case-insensitive. The lower-case $outputPath
# local therefore aliases the $OutputPath parameter and redirected the verified
# evidence JSON onto the generated DOCX/PDF path. Give the generated document
# its own unambiguous variable name while preserving the output parameter.
verifier = ROOT / "tests/windows/verify_reboot_evidence.ps1"
verifier_text = verifier.read_text(encoding="utf-8")
local_output_uses = verifier_text.count("$outputPath")
if local_output_uses < 5:
    raise SystemExit(f"unexpected generated-output variable use count: {local_output_uses}")
verifier_text = verifier_text.replace("$outputPath", "$generatedOutputPath")
if "$OutputPath" not in verifier_text:
    raise SystemExit("verified evidence OutputPath parameter was unexpectedly removed")
verifier.write_text(verifier_text, encoding="utf-8")

binding_test = ROOT / "tests/test_windows_reboot_evidence_binding.py"
binding_text = binding_test.read_text(encoding="utf-8")
anchor = '    assert "Post-reboot output is outside the prepared watch folder" in verify\n'
addition = (
    '    assert "$generatedOutputPath = Normalize-PathValue $evidence.post_reboot_output_path" in verify\n'
    '    assert "post_reboot_output_path = $generatedOutputPath" in verify\n'
)
if addition not in binding_text:
    if binding_text.count(anchor) != 1:
        raise SystemExit("expected reboot generated-output assertion anchor")
    binding_test.write_text(binding_text.replace(anchor, anchor + addition, 1), encoding="utf-8")

# Both helper transports must disappear from the final branch before the source
# manifest is rebuilt.
for temporary in (
    ROOT / ".github/workflows/finalize-windows-evidence-pr.yml",
    Path(__file__).resolve(),
):
    temporary.unlink(missing_ok=True)

candidate = ROOT / "target/final-windows-evidence-source-manifest-v2.txt"
candidate.parent.mkdir(parents=True, exist_ok=True)
run("scripts/verify_source_manifest.py", "--candidate", str(candidate), allow_failure=True)
if not candidate.is_file():
    raise SystemExit("source manifest generator did not create the final candidate file")
(ROOT / "SOURCE_MANIFEST_SHA256.txt").write_bytes(candidate.read_bytes())
run("scripts/verify_source_manifest.py")
