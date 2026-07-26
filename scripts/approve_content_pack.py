#!/usr/bin/env python3
"""Create or verify detached, exact-revision approval evidence for a content pack.

This tool does not declare forms legally correct by itself. It records the named
organisation/reviewer, jurisdiction, legal basis and exact DOCX hashes, then signs
the canonical payload with Ed25519. Production publication can require a trusted
public key and reject draft-only or changed templates.
"""
from __future__ import annotations

import argparse
import base64
import json
import re
import sys
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Any

from ed25519_compat import BadSignatureError, SigningKey, VerifyKey

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from validate_content_pack import validate  # noqa: E402

B64_RE = re.compile(r"^[A-Za-z0-9+/]+={0,2}$")


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def read_b64(path: Path, expected_lengths: set[int], title: str) -> bytes:
    raw = path.read_text("utf-8").strip()
    if not B64_RE.fullmatch(raw):
        raise ValueError(f"{title} must contain one base64 value")
    decoded = base64.b64decode(raw, validate=True)
    if len(decoded) not in expected_lengths:
        raise ValueError(f"{title} has invalid decoded length {len(decoded)}")
    return decoded


def parse_iso_date(value: str, title: str) -> str:
    parsed = date.fromisoformat(value)
    return parsed.isoformat()


def build_payload(args: argparse.Namespace) -> dict[str, Any]:
    pack_root = args.pack.resolve()
    validation = validate(pack_root)
    manifest = json.loads((pack_root / "pack.json").read_text("utf-8"))
    if manifest.get("usage_mode") != "draft_only" or manifest.get("requires_organization_review") is not True:
        raise ValueError("approval source must remain draft_only and require organisation review")
    templates = []
    for slot in manifest["template_slots"]:
        if not slot.get("sha256") or not slot.get("template_path"):
            raise ValueError(f"template {slot.get('document_id')} is not hash-verified")
        templates.append({
            "document_id": slot["document_id"],
            "template_path": slot["template_path"],
            "sha256": slot["sha256"],
        })
    templates.sort(key=lambda item: item["document_id"])
    valid_until = parse_iso_date(args.valid_until, "valid-until") if args.valid_until else None
    payload = {
        "schema": "dokkomplekt.content-pack-approval.v1",
        "pack_id": manifest["pack_id"],
        "pack_version": manifest["version"],
        "source_status": validation["status"],
        "organization": args.organization.strip(),
        "reviewer": args.reviewer.strip(),
        "jurisdiction": args.jurisdiction.strip(),
        "legal_basis": args.legal_basis.strip(),
        "review_scope": args.review_scope.strip(),
        "approved_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
        "valid_until": valid_until,
        "templates": templates,
        "production_assertion": "approved_for_named_organization_and_jurisdiction_only",
    }
    for key in ("organization", "reviewer", "jurisdiction", "legal_basis", "review_scope"):
        if not payload[key] or len(payload[key]) > 2000:
            raise ValueError(f"{key} is required and must be at most 2000 characters")
    return payload


def create(args: argparse.Namespace) -> int:
    seed = read_b64(args.signing_key.resolve(), {32}, "signing key")
    signing_key = SigningKey(seed)
    payload = build_payload(args)
    message = canonical_json(payload)
    signature = signing_key.sign(message).signature
    document = {
        "payload": payload,
        "signature_b64": base64.b64encode(signature).decode("ascii"),
        "public_key_b64": base64.b64encode(bytes(signing_key.verify_key)).decode("ascii"),
    }
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", "utf-8")
    print(f"CONTENT PACK APPROVAL CREATED: {output}")
    return 0


def verify(args: argparse.Namespace) -> int:
    document = json.loads(args.approval.resolve().read_text("utf-8"))
    payload = document.get("payload")
    if not isinstance(payload, dict) or payload.get("schema") != "dokkomplekt.content-pack-approval.v1":
        raise ValueError("approval payload schema is invalid")
    trusted = read_b64(args.trusted_public_key.resolve(), {32}, "trusted public key")
    embedded = base64.b64decode(str(document.get("public_key_b64", "")), validate=True)
    if embedded != trusted:
        raise ValueError("approval public key is not the trusted organization key")
    signature = base64.b64decode(str(document.get("signature_b64", "")), validate=True)
    try:
        VerifyKey(trusted).verify(canonical_json(payload), signature)
    except BadSignatureError as exc:
        raise ValueError("approval signature is invalid") from exc

    pack_root = args.pack.resolve()
    validate(pack_root)
    manifest = json.loads((pack_root / "pack.json").read_text("utf-8"))
    expected = {
        slot["document_id"]: slot["sha256"]
        for slot in manifest["template_slots"]
    }
    approved = {
        item["document_id"]: item["sha256"]
        for item in payload.get("templates", [])
        if isinstance(item, dict)
    }
    if payload.get("pack_id") != manifest.get("pack_id") or payload.get("pack_version") != manifest.get("version"):
        raise ValueError("approval belongs to another pack/version")
    if approved != expected:
        raise ValueError("approval template hash set differs from the current pack")
    valid_until = payload.get("valid_until")
    if valid_until and date.fromisoformat(valid_until) < date.today():
        raise ValueError("approval has expired")
    print(
        "CONTENT PACK APPROVAL VERIFIED: "
        f"pack={payload['pack_id']} jurisdiction={payload['jurisdiction']} organization={payload['organization']}"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    create_parser = subparsers.add_parser("create")
    create_parser.add_argument("--pack", type=Path, required=True)
    create_parser.add_argument("--organization", required=True)
    create_parser.add_argument("--reviewer", required=True)
    create_parser.add_argument("--jurisdiction", required=True)
    create_parser.add_argument("--legal-basis", required=True)
    create_parser.add_argument("--review-scope", required=True)
    create_parser.add_argument("--valid-until")
    create_parser.add_argument("--signing-key", type=Path, required=True, help="file with base64 Ed25519 32-byte seed")
    create_parser.add_argument("--output", type=Path, required=True)
    create_parser.set_defaults(handler=create)

    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--pack", type=Path, required=True)
    verify_parser.add_argument("--approval", type=Path, required=True)
    verify_parser.add_argument("--trusted-public-key", type=Path, required=True, help="file with base64 Ed25519 public key")
    verify_parser.set_defaults(handler=verify)

    args = parser.parse_args()
    return args.handler(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
