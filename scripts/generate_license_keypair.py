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

import argparse
import base64
import json
import sys
from pathlib import Path

try:
    from ed25519_compat import SigningKey
except ImportError:  # pragma: no cover
    print("cryptography is required: pip install cryptography", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--json-output",
        type=Path,
        help="Write the generated keypair to a JSON file instead of stdout.",
    )
    args = parser.parse_args()

    signing_key = SigningKey.generate()
    public_b64 = base64.b64encode(bytes(signing_key.verify_key)).decode()
    private_b64 = base64.b64encode(bytes(signing_key)).decode()

    if args.json_output is not None:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(
            json.dumps(
                {
                    "public_key_b64": public_b64,
                    "private_key_b64": private_b64,
                },
                ensure_ascii=False,
            ),
            encoding="utf-8",
        )
        return 0

    print("DOKKOMPLEKT_LICENSE_PUBKEY_B64 (bake into the desktop build):")
    print(f"  {public_b64}")
    print()
    print("Issuer PRIVATE seed (license server only — keep secret):")
    print(f"  {private_b64}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
