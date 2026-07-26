#!/usr/bin/env python3
from __future__ import annotations
import base64, hashlib, json, os, subprocess, sys
from datetime import datetime, timezone
from pathlib import Path
from ed25519_compat import SigningKey

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.cargo-gate' / 'CARGO_GATE_ATTESTATION.json'
SIG = ROOT / '.cargo-gate' / 'CARGO_GATE_ATTESTATION.sig'
RUSTSEC = ROOT / '.cargo-gate' / 'RUSTSEC_EVIDENCE.json'
RUSTSEC_REPORT = ROOT / '.cargo-gate' / 'RUSTSEC_AUDIT.json'
COMMERCIAL = ROOT / '.cargo-gate' / 'COMMERCIAL_CRATES_EVIDENCE.json'
COMMERCIAL_LOCK = ROOT / '.cargo-gate' / 'COMMERCIAL_CRATES_Cargo.lock'
COMMERCIAL_AUDIT = ROOT / '.cargo-gate' / 'COMMERCIAL_CRATES_RUSTSEC_AUDIT.json'

def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode('utf-8')

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def command(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

def main() -> int:
    raw = os.environ.get('DOKKOMPLEKT_GATE_PRIVATE_KEY_B64', '').strip()
    if not raw:
        raise SystemExit('DOKKOMPLEKT_GATE_PRIVATE_KEY_B64 is required; unsigned gate markers are forbidden')
    seed = base64.b64decode(raw, validate=True)
    if len(seed) != 32:
        raise SystemExit('DOKKOMPLEKT_GATE_PRIVATE_KEY_B64 must contain a 32-byte Ed25519 seed')
    key = SigningKey(seed)
    source = command(sys.executable, 'scripts/source_fingerprint.py')
    required = (RUSTSEC, RUSTSEC_REPORT, COMMERCIAL, COMMERCIAL_LOCK, COMMERCIAL_AUDIT)
    if any(not path.is_file() for path in required):
        missing = ', '.join(str(path.relative_to(ROOT)) for path in required if not path.is_file())
        raise SystemExit(f'Rust and commercial-crate evidence is required before signing the Cargo gate: {missing}')
    rustsec = json.loads(RUSTSEC.read_text('utf-8'))
    if rustsec.get('result') != 'passed' or rustsec.get('source_sha256') != source:
        raise SystemExit('RustSec evidence does not match the tested source tree')
    if rustsec.get('cargo_lock_sha256') != sha(ROOT / 'Cargo.lock'):
        raise SystemExit('RustSec evidence does not match Cargo.lock')
    if rustsec.get('audit_report_sha256') != sha(RUSTSEC_REPORT):
        raise SystemExit('RustSec report changed after the audit')
    commercial = json.loads(COMMERCIAL.read_text('utf-8'))
    if commercial.get('result') != 'passed' or commercial.get('source_sha256') != source:
        raise SystemExit('Commercial Rust evidence does not match the tested source tree')
    if commercial.get('generated_lock_sha256') != sha(COMMERCIAL_LOCK):
        raise SystemExit('Commercial Rust lock evidence changed after the gate')
    if commercial.get('audit_report_sha256') != sha(COMMERCIAL_AUDIT):
        raise SystemExit('Commercial RustSec report changed after the gate')
    payload = {
        'schema': 'dokkomplekt.cargo-gate.v3',
        'result': 'passed',
        'timestamp_utc': datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z'),
        'source_sha256': source,
        'cargo_lock_sha256': sha(ROOT / 'Cargo.lock'),
        'cargo': command('cargo', '--version'),
        'rustc': command('rustc', '--version'),
        'repository': os.environ.get('GITHUB_REPOSITORY', ''),
        'commit_sha': os.environ.get('GITHUB_SHA', ''),
        'workflow_run_id': os.environ.get('GITHUB_RUN_ID', ''),
        'workflow_run_attempt': os.environ.get('GITHUB_RUN_ATTEMPT', ''),
        'runner_os': os.environ.get('RUNNER_OS', os.name),
        'runner_arch': os.environ.get('RUNNER_ARCH', ''),
        'rustsec_evidence_sha256': sha(RUSTSEC),
        'rustsec_advisory_database_commit': rustsec['advisory_database_commit'],
        'cargo_audit_version': rustsec['cargo_audit_version'],
        'commercial_crates_evidence_sha256': sha(COMMERCIAL),
        'commercial_crates_lock_sha256': sha(COMMERCIAL_LOCK),
        'commercial_crates_audit_sha256': sha(COMMERCIAL_AUDIT),
        'checks': [
            'cargo metadata --locked',
            'cargo fmt --all -- --check',
            'cargo check --workspace --all-targets --locked',
            'cargo clippy --workspace --all-targets --locked -- -D warnings',
            'cargo test --workspace --locked',
            'cargo audit --deny warnings --json',
            'RustSec advisory database HEAD and JSON report evidence',
            'excluded commercial license-server/python crates: fmt/check/clippy/test/audit in isolated workspace',
        ],
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    SIG.write_text(base64.b64encode(key.sign(canonical(payload)).signature).decode('ascii') + '\n', encoding='ascii')
    print(OUT)
    print('DOKKOMPLEKT_GATE_PUBKEY_B64=' + base64.b64encode(bytes(key.verify_key)).decode('ascii'))
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
