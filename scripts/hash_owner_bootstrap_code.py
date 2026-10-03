"""Print a domain-separated SHA-256 digest for the owner bootstrap code.

The plaintext code is read with getpass and never written to disk or echoed.
Store only the resulting digest in DOKKOMPLEKT_OWNER_BOOTSTRAP_CODE_SHA256.
"""
from __future__ import annotations

from getpass import getpass
import hashlib

DOMAIN = b"dokkomplekt-owner-bootstrap-v1\0"


def digest(code: str) -> str:
    value = code.strip()
    if not value:
        raise ValueError("owner bootstrap code must not be empty")
    return hashlib.sha256(DOMAIN + value.encode("utf-8")).hexdigest()


def main() -> None:
    first = getpass("Owner bootstrap code: ")
    second = getpass("Repeat owner bootstrap code: ")
    if first != second:
        raise SystemExit("codes do not match")
    print(digest(first))


if __name__ == "__main__":
    main()
