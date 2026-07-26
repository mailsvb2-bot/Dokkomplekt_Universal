import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check_reference_data_freshness.py"


class ReferenceDataFreshnessTests(unittest.TestCase):
    def run_for(self, date: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--date", date],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_provisional_next_year_is_allowed_before_october(self) -> None:
        result = self.run_for("2026-07-18")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("2027=provisional", result.stdout)

    def test_provisional_next_year_blocks_release_from_october(self) -> None:
        result = self.run_for("2026-10-01")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("2027", result.stderr)


if __name__ == "__main__":
    unittest.main()
