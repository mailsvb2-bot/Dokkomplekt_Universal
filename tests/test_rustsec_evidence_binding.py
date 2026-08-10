from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))


def load(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def test_rustsec_evidence_uses_audited_pin_report_not_cached_checkout(tmp_path: Path, monkeypatch) -> None:
    module = load(SCRIPTS / "write_rustsec_evidence.py", "write_rustsec_evidence_binding")
    report = tmp_path / "audit.json"
    pin = tmp_path / "pin.json"
    evidence = tmp_path / "evidence.json"
    lock = tmp_path / "Cargo.lock"
    report.write_text('{"vulnerabilities": {}}', encoding="utf-8")
    lock.write_text("lock", encoding="utf-8")
    commit = "a" * 40
    repository = "https://github.com/RustSec/advisory-db"
    pin.write_text(json.dumps({"repository": repository, "commit": commit}), encoding="utf-8")

    monkeypatch.setattr(module, "RAW_REPORT", report)
    monkeypatch.setattr(module, "PIN_REPORT", pin)
    monkeypatch.setattr(module, "EVIDENCE", evidence)
    monkeypatch.setattr(module, "ROOT", tmp_path)
    monkeypatch.setattr(module, "command", lambda *args: "source" if "source_fingerprint.py" in args else "cargo-audit 0.22.2")

    assert module.main() == 0
    payload = json.loads(evidence.read_text(encoding="utf-8"))
    assert payload["advisory_database_commit"] == commit
    assert payload["advisory_database_origin"] == repository
    assert payload["advisory_database_pin_report_sha256"] == digest(pin)


def test_rustsec_evidence_rejects_invalid_audited_pin(tmp_path: Path) -> None:
    module = load(SCRIPTS / "write_rustsec_evidence.py", "write_rustsec_evidence_invalid_pin")
    pin = tmp_path / "pin.json"
    pin.write_text(json.dumps({"repository": "https://github.com/RustSec/advisory-db", "commit": "short"}), encoding="utf-8")
    with pytest.raises(RuntimeError, match="full lowercase Git SHA"):
        module.load_audited_pin(pin)


def test_signed_attestation_fails_closed_if_audited_pin_changes(tmp_path: Path, monkeypatch) -> None:
    module = load(SCRIPTS / "write_cargo_gate_attestation.py", "write_cargo_gate_attestation_binding")
    gate = tmp_path / ".cargo-gate"
    gate.mkdir()
    cargo_lock = tmp_path / "Cargo.lock"
    cargo_lock.write_text("lock", encoding="utf-8")

    rustsec_report = gate / "RUSTSEC_AUDIT.json"
    rustsec_report.write_text('{"vulnerabilities": {}}', encoding="utf-8")
    rustsec_pin = gate / "RUSTSEC_DB_PIN.json"
    rustsec_pin.write_text(json.dumps({"repository": "https://github.com/RustSec/advisory-db", "commit": "a" * 40}), encoding="utf-8")
    rustsec = gate / "RUSTSEC_EVIDENCE.json"
    rustsec.write_text(json.dumps({
        "result": "passed",
        "source_sha256": "source",
        "cargo_lock_sha256": digest(cargo_lock),
        "audit_report_sha256": digest(rustsec_report),
        "advisory_database_pin_report_sha256": digest(rustsec_pin),
        "advisory_database_commit": "a" * 40,
        "advisory_database_origin": "https://github.com/RustSec/advisory-db",
        "cargo_audit_version": "cargo-audit 0.22.2",
    }), encoding="utf-8")

    commercial_lock = gate / "COMMERCIAL_CRATES_Cargo.lock"
    commercial_lock.write_text("commercial-lock", encoding="utf-8")
    commercial_audit = gate / "COMMERCIAL_CRATES_RUSTSEC_AUDIT.json"
    commercial_audit.write_text("{}", encoding="utf-8")
    commercial = gate / "COMMERCIAL_CRATES_EVIDENCE.json"
    commercial.write_text(json.dumps({
        "result": "passed",
        "source_sha256": "source",
        "generated_lock_sha256": digest(commercial_lock),
        "audit_report_sha256": digest(commercial_audit),
    }), encoding="utf-8")

    monkeypatch.setattr(module, "ROOT", tmp_path)
    monkeypatch.setattr(module, "OUT", gate / "CARGO_GATE_ATTESTATION.json")
    monkeypatch.setattr(module, "SIG", gate / "CARGO_GATE_ATTESTATION.sig")
    monkeypatch.setattr(module, "RUSTSEC", rustsec)
    monkeypatch.setattr(module, "RUSTSEC_REPORT", rustsec_report)
    monkeypatch.setattr(module, "RUSTSEC_PIN", rustsec_pin)
    monkeypatch.setattr(module, "COMMERCIAL", commercial)
    monkeypatch.setattr(module, "COMMERCIAL_LOCK", commercial_lock)
    monkeypatch.setattr(module, "COMMERCIAL_AUDIT", commercial_audit)
    monkeypatch.setattr(module, "command", lambda *args: "source" if "source_fingerprint.py" in args else "tool-version")
    monkeypatch.setenv("DOKKOMPLEKT_GATE_PRIVATE_KEY_B64", base64.b64encode(b"x" * 32).decode("ascii"))

    # Tamper the exact pin after evidence creation. Signing must fail rather than
    # silently binding to some unrelated cached advisory-db checkout.
    rustsec_pin.write_text(json.dumps({"repository": "https://github.com/RustSec/advisory-db", "commit": "b" * 40}), encoding="utf-8")
    with pytest.raises(SystemExit, match="pin report changed"):
        module.main()
