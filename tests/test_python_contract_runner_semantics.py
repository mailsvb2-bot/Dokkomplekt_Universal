from __future__ import annotations

import importlib
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
RUNNER_MODULE = "run_python_contracts_sharded"


def load_runner():
    sys.path.insert(0, str(SCRIPTS))
    try:
        sys.modules.pop(RUNNER_MODULE, None)
        return importlib.import_module(RUNNER_MODULE)
    finally:
        sys.path.remove(str(SCRIPTS))


def run_temporary_shard(body: str):
    runner = load_runner()
    path = ROOT / "tests" / "_runner_semantics_probe.py"
    path.write_text(body, encoding="utf-8")
    try:
        return runner.run_module(path, 30)
    finally:
        path.unlink(missing_ok=True)


def test_skip_only_shard_is_successful() -> None:
    result = run_temporary_shard(
        "import pytest\n\ndef test_platform_gate():\n    pytest.skip('platform-only evidence')\n"
    )
    assert result.returncode == 0
    assert result.passed == 0
    assert result.skipped == 1
    assert result.result == "passed"


def test_real_failure_shard_stays_failed() -> None:
    result = run_temporary_shard("def test_failure():\n    assert False\n")
    assert result.returncode != 0
    assert result.result == "failed"
