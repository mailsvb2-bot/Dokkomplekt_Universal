#!/usr/bin/env python3
"""Generate an Ed25519 keypair for Dokkomplekt license issuing.

Usage:
    pip install cryptography
    python scripts/generate_license_keypair.py

Output:
    - PUBLIC key (base64, 32 bytes): bake into the desktop build by exporting
      DOKKOMPLEKT_LICENSE_PUBKEY_B64=<public key> before `npm run tauri:build`.
    - PRIVATE key (base64, 32-byte seed): configure it ONLY on the license
      server (issuer). Never commit it, never ship it inside the app.

The key compiled into unofficial builds has no surviving private half, so
license verification in such builds fails closed by design.
"""
from __future__ import annotations

import base64
import sys

try:
    from ed25519_compat import SigningKey
except ImportError:  # pragma: no cover
    print("cryptography is required: pip install cryptography", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    signing_key = SigningKey.generate()
    public_b64 = base64.b64encode(bytes(signing_key.verify_key)).decode()
    private_b64 = base64.b64encode(bytes(signing_key)).decode()
    print("DOKKOMPLEKT_LICENSE_PUBKEY_B64 (bake into the desktop build):")
    print(f"  {public_b64}")
    print()
    print("Issuer PRIVATE seed (license server only — keep secret):")
    print(f"  {private_b64}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
