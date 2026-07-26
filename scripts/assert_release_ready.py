from __future__ import annotations
import base64, hashlib, json, os, sys
from pathlib import Path
from ed25519_compat import BadSignatureError, VerifyKey
from source_fingerprint import source_fingerprint

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / 'Cargo.lock'
ATTESTATION = ROOT / '.cargo-gate' / 'CARGO_GATE_ATTESTATION.json'
SIGNATURE = ROOT / '.cargo-gate' / 'CARGO_GATE_ATTESTATION.sig'
RUSTSEC = ROOT / '.cargo-gate' / 'RUSTSEC_EVIDENCE.json'
RUSTSEC_REPORT = ROOT / '.cargo-gate' / 'RUSTSEC_AUDIT.json'
COMMERCIAL = ROOT / '.cargo-gate' / 'COMMERCIAL_CRATES_EVIDENCE.json'
COMMERCIAL_LOCK = ROOT / '.cargo-gate' / 'COMMERCIAL_CRATES_Cargo.lock'
COMMERCIAL_AUDIT = ROOT / '.cargo-gate' / 'COMMERCIAL_CRATES_RUSTSEC_AUDIT.json'
missing = [str(path.relative_to(ROOT)) for path in (LOCK, ATTESTATION, SIGNATURE, RUSTSEC, RUSTSEC_REPORT, COMMERCIAL, COMMERCIAL_LOCK, COMMERCIAL_AUDIT) if not path.exists()]
if missing:
    print('Release packaging is blocked. Missing signed Rust gate artifact(s):')
    for item in missing:
        print(f'- {item}')
    sys.exit(1)

public_b64 = os.environ.get('DOKKOMPLEKT_GATE_PUBKEY_B64', '').strip()
if not public_b64:
    print('Release packaging is blocked: DOKKOMPLEKT_GATE_PUBKEY_B64 is required.')
    sys.exit(1)
try:
    public = base64.b64decode(public_b64, validate=True)
    signature = base64.b64decode(SIGNATURE.read_text('ascii').strip(), validate=True)
    payload = json.loads(ATTESTATION.read_text('utf-8'))
    canonical = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode('utf-8')
    VerifyKey(public).verify(canonical, signature)
except (ValueError, json.JSONDecodeError, BadSignatureError) as error:
    print(f'Release packaging is blocked: invalid Cargo gate signature: {error}')
    sys.exit(1)

actual_source = source_fingerprint()
actual_lock = hashlib.sha256(LOCK.read_bytes()).hexdigest()
if payload.get('schema') != 'dokkomplekt.cargo-gate.v3' or payload.get('result') != 'passed':
    print('Release packaging is blocked: unsupported or unsuccessful Cargo gate attestation.')
    sys.exit(1)

if os.environ.get('GITHUB_ACTIONS', '').lower() == 'true':
    for field in ('repository', 'commit_sha', 'workflow_run_id', 'workflow_run_attempt'):
        if not str(payload.get(field, '')).strip():
            print(f'Release packaging is blocked: signed Cargo gate is missing CI identity field {field}.')
            sys.exit(1)

if payload.get('source_sha256') != actual_source or payload.get('cargo_lock_sha256') != actual_lock:
    print('Release packaging is blocked: sources or Cargo.lock changed after the signed Rust gate.')
    sys.exit(1)
try:
    rustsec = json.loads(RUSTSEC.read_text('utf-8'))
except (OSError, json.JSONDecodeError) as error:
    print(f'Release packaging is blocked: invalid RustSec evidence: {error}')
    sys.exit(1)
if rustsec.get('schema') != 'dokkomplekt.rustsec-evidence.v1' or rustsec.get('result') != 'passed':
    print('Release packaging is blocked: unsuccessful RustSec evidence.')
    sys.exit(1)
if rustsec.get('source_sha256') != actual_source or rustsec.get('cargo_lock_sha256') != actual_lock:
    print('Release packaging is blocked: RustSec evidence belongs to different sources or Cargo.lock.')
    sys.exit(1)
if rustsec.get('audit_command') != 'cargo audit --deny warnings --json':
    print('Release packaging is blocked: RustSec audit command was not fail-closed.')
    sys.exit(1)
if rustsec.get('advisory_database_dirty') is not False:
    print('Release packaging is blocked: RustSec advisory database was dirty.')
    sys.exit(1)
commit = str(rustsec.get('advisory_database_commit', '')).lower()
if len(commit) != 40 or any(char not in '0123456789abcdef' for char in commit):
    print('Release packaging is blocked: RustSec advisory database commit is missing.')
    sys.exit(1)
report_hash = hashlib.sha256(RUSTSEC_REPORT.read_bytes()).hexdigest()
if rustsec.get('audit_report_sha256') != report_hash:
    print('Release packaging is blocked: RustSec JSON report changed after audit.')
    sys.exit(1)
evidence_hash = hashlib.sha256(RUSTSEC.read_bytes()).hexdigest()
if payload.get('rustsec_evidence_sha256') != evidence_hash or payload.get('rustsec_advisory_database_commit') != commit:
    print('Release packaging is blocked: signed Cargo gate does not bind the RustSec evidence.')
    sys.exit(1)
try:
    commercial = json.loads(COMMERCIAL.read_text('utf-8'))
except (OSError, json.JSONDecodeError) as error:
    print(f'Release packaging is blocked: invalid commercial Rust evidence: {error}')
    sys.exit(1)
if commercial.get('schema') != 'dokkomplekt.commercial-rust-gate.v1' or commercial.get('result') != 'passed':
    print('Release packaging is blocked: commercial Rust crates did not pass their mandatory gate.')
    sys.exit(1)
if commercial.get('source_sha256') != actual_source:
    print('Release packaging is blocked: commercial Rust evidence belongs to different sources.')
    sys.exit(1)
commercial_lock_hash = hashlib.sha256(COMMERCIAL_LOCK.read_bytes()).hexdigest()
commercial_audit_hash = hashlib.sha256(COMMERCIAL_AUDIT.read_bytes()).hexdigest()
commercial_evidence_hash = hashlib.sha256(COMMERCIAL.read_bytes()).hexdigest()
if commercial.get('generated_lock_sha256') != commercial_lock_hash or commercial.get('audit_report_sha256') != commercial_audit_hash:
    print('Release packaging is blocked: commercial Rust lock/audit evidence changed after verification.')
    sys.exit(1)
if (payload.get('commercial_crates_evidence_sha256') != commercial_evidence_hash
        or payload.get('commercial_crates_lock_sha256') != commercial_lock_hash
        or payload.get('commercial_crates_audit_sha256') != commercial_audit_hash):
    print('Release packaging is blocked: signed Cargo gate does not bind commercial Rust evidence.')
    sys.exit(1)
expected_sha = os.environ.get('GITHUB_SHA', '').strip()
if expected_sha and payload.get('commit_sha') != expected_sha:
    print('Release packaging is blocked: signed Cargo gate belongs to another commit.')
    sys.exit(1)
print(f"RELEASE READY: signed Rust attestation matches sources ({actual_source[:12]}…).")
