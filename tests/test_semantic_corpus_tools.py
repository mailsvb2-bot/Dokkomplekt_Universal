from __future__ import annotations

import base64
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def corpus_payload(count: int = 30, *, wrong_every: int | None = None) -> dict:
    entries = []
    for index in range(count):
        expected = f"final-{index}"
        proposed = expected if not wrong_every or index % wrong_every else f"wrong-{index}"
        entry = {
            "entry_id": f"entry-{index:03d}",
            "case_id": f"case-{index:03d}",
            "source_sha256": "a" * 64,
            "input_text_sha256": "b" * 64,
            "domain": "Hr",
            "pack_id": "hr-pack",
            "model_proposals": [
                {
                    "field_id": "document.number",
                    "value_sha256": proposed,
                    "source": "Model",
                    "confidence": 0.999 if proposed == expected else 0.6,
                    "evidence_sha256": ["c" * 64],
                }
            ],
            "deterministic": [],
            "final_accepted": [
                {
                    "field_id": "document.number",
                    "value_sha256": expected,
                    "source": "UserConfirmed",
                    "confidence": 1.0,
                    "evidence_sha256": [],
                }
            ],
            "proposed_kit_documents": ["employment_contract", "employment_order"],
            "kit_proposal_source": "curated-router:review",
            "kit_documents": ["employment_contract", "employment_order"],
            "created_at": "2026-07-21T12:00:00Z",
            "evaluation": {
                "zero_touch_attempted": True,
                "zero_touch_completed": index % 10 != 0,
                "automation_blocked": index % 10 == 0,
                "block_was_false_positive": index == 0,
                "generation_correct": index % 15 != 0,
                "auto_print_attempted": index % 3 == 0,
                "auto_print_correct": index % 15 != 0,
                "formatting_retention": 1.0 if index % 5 else 0.9,
                "processing_time_ms": 1000 + index * 10,
            },
        }
        entries.append({"entry": entry, "metrics": {}})
    return {
        "schema": "dokkomplekt.ground-truth-corpus.v1",
        "privacy": {
            "raw_source_text": False,
            "raw_field_values": False,
            "comparison_values": "installation-keyed-hmac-sha256",
        },
        "entries": entries,
    }


def run(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
        env=env,
    )


def test_measure_domain_uses_specialist_final_and_kit_truth(tmp_path: Path) -> None:
    corpus = tmp_path / "corpus.json"
    corpus.write_text(json.dumps(corpus_payload(wrong_every=5)), encoding="utf-8")
    result = run("scripts/measure_domain.py", str(corpus), "--domain", "hr", "--auto-threshold", "0.99")
    assert result.returncode == 0, result.stderr
    report = json.loads(result.stdout)
    assert report["entry_count"] == 30
    assert report["kit_completeness"] == 1.0
    assert report["field_accuracy"] < 1.0
    assert report["auto_bucket_error_rate"] == 0.0
    assert report["field_recall"] < 1.0
    assert report["bundle_selection_accuracy"] == 1.0
    assert report["bundle_precision"] == 1.0
    assert report["bundle_recall"] == 1.0
    assert 0.0 < report["zero_touch_rate"] < 1.0
    assert report["false_block_rate"] > 0.0
    assert report["wrong_generation_rate"] > 0.0
    assert report["wrong_auto_print_rate"] > 0.0
    assert 0.0 < report["formatting_retention_rate"] <= 1.0
    assert report["average_processing_time_ms"] > 0


def test_calibration_writes_fail_closed_empirical_thresholds(tmp_path: Path) -> None:
    corpus = tmp_path / "corpus.json"
    output = tmp_path / "thresholds.json"
    corpus.write_text(json.dumps(corpus_payload()), encoding="utf-8")
    result = run(
        "scripts/calibrate_thresholds.py",
        str(corpus),
        "--domain",
        "hr",
        "--output",
        str(output),
        "--min-auto-samples",
        "2",
        "--min-review-samples",
        "2",
        "--min-holdout-auto-samples",
        "2",
    )
    assert result.returncode == 0, result.stderr
    payload = json.loads(output.read_text(encoding="utf-8"))
    assert payload["schema"] == "dokkomplekt.calibrated-thresholds.v1"
    assert payload["policy"]["source_of_truth"] == "specialist_final_accepted"
    assert payload["max_auto_error_rate"] == 0.005
    assert payload["auto_min_confidence"] >= payload["review_min_confidence"]


def test_calibration_refuses_insufficient_corpus(tmp_path: Path) -> None:
    corpus = tmp_path / "tiny.json"
    output = tmp_path / "thresholds.json"
    corpus.write_text(json.dumps(corpus_payload(2)), encoding="utf-8")
    result = run(
        "scripts/calibrate_thresholds.py",
        str(corpus),
        "--domain",
        "hr",
        "--output",
        str(output),
        "--min-auto-samples",
        "50",
    )
    assert result.returncode != 0
    assert "FAILED CLOSED" in result.stderr
    assert not output.exists()


def test_calibration_signature_covers_exact_canonical_payload(tmp_path: Path) -> None:
    from scripts.ed25519_compat import SigningKey

    corpus = tmp_path / "corpus.json"
    output = tmp_path / "thresholds.signed.json"
    corpus.write_text(json.dumps(corpus_payload()), encoding="utf-8")
    key = SigningKey.generate()
    env = dict(os.environ)
    env["DOKKOMPLEKT_THRESHOLD_PRIVATE_KEY_B64"] = base64.b64encode(bytes(key)).decode("ascii")
    result = run(
        "scripts/calibrate_thresholds.py",
        str(corpus),
        "--domain",
        "hr",
        "--output",
        str(output),
        "--min-auto-samples",
        "2",
        "--min-review-samples",
        "2",
        "--min-holdout-auto-samples",
        "2",
        "--sign",
        env=env,
    )
    assert result.returncode == 0, result.stderr
    envelope = json.loads(output.read_text(encoding="utf-8"))
    assert envelope["signature_alg"] == "ed25519"
    canonical = json.dumps(
        envelope["payload"], ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    key.verify_key.verify(canonical, base64.b64decode(envelope["signature_b64"], validate=True))
