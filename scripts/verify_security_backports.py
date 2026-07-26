from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[1]
cargo=(ROOT/'Cargo.toml').read_text()
audit=(ROOT/'.cargo/audit.toml').read_text()
errors=[]
if '[patch.crates-io]' in cargo: errors.append('local crates.io patches are forbidden')
if 'vendor/time' in cargo or 'vendor/plist' in cargo: errors.append('vendored time/plist fork remains')
if 'RUSTSEC-2026-0009' in audit or 'ignore = ["RUSTSEC-2026-0009"]' in audit: errors.append('obsolete RustSec exception remains')
if '>=0.3.47, <0.4' not in cargo: errors.append('upstream time >=0.3.47 is required')
if errors:
 print('\n'.join(errors), file=sys.stderr); raise SystemExit(1)
print('UPSTREAM SECURITY DEPENDENCY POLICY OK')
