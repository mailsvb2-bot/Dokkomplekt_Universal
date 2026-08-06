#!/usr/bin/env python3
"""Fail-closed validator for optional Dokkomplekt content packs."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

try:
    from scripts._release_policy import validate_relative_runtime_path
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path

FIELD_RE = re.compile(r"^[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+$")
FIELD_TOKEN_RE = re.compile(r"[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+")
PLACEHOLDER_RE = re.compile(r"{{\s*([^{}]+?)\s*}}")
ID_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{1,79}$")
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
STATUSES = {"workflow_skeleton", "starter", "pilot", "approved"}
DOMAINS = {"hr", "legal", "accounting", "medical", "education", "custom"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_file(root: Path, value: str) -> Path:
    relative = Path(validate_relative_runtime_path(value, "template path"))
    resolved = (root / relative).resolve()
    if root.resolve() not in resolved.parents:
        raise ValueError(f"template escapes pack root: {value!r}")
    if resolved.is_symlink():
        raise ValueError(f"template symlinks are forbidden: {value!r}")
    return resolved



def extract_docx_field_references(path: Path) -> set[str]:
    """Read every Word story and return semantic field ids used by placeholders.

    Text runs are concatenated in XML order so placeholders split by Word across
    multiple `<w:t>` nodes are still detected. Directives and functions are
    harmless because only dotted semantic ids match FIELD_TOKEN_RE.
    """
    fields: set[str] = set()
    try:
        with zipfile.ZipFile(path) as archive:
            stories = sorted(
                name for name in archive.namelist()
                if name.startswith("word/") and name.endswith(".xml")
            )
            for name in stories:
                root = ET.fromstring(archive.read(name))
                text = "".join(
                    node.text or ""
                    for node in root.iter()
                    if node.tag.rsplit("}", 1)[-1] in {"t", "instrText"}
                )
                for expression in PLACEHOLDER_RE.findall(text):
                    fields.update(FIELD_TOKEN_RE.findall(expression.lower()))
    except (zipfile.BadZipFile, ET.ParseError) as exc:
        raise ValueError(f"invalid DOCX container {path.name}: {exc}") from exc
    return fields

def validate(pack_root: Path) -> dict:
    manifest_path = pack_root / "pack.json"
    data = json.loads(manifest_path.read_text("utf-8"))
    if data.get("schema") != 1:
        raise ValueError("schema must be 1")
    if not ID_RE.fullmatch(str(data.get("pack_id", ""))):
        raise ValueError("invalid pack_id")
    if data.get("status") not in STATUSES:
        raise ValueError("invalid status")
    if data.get("domain") not in DOMAINS:
        raise ValueError("invalid domain")
    slots = data.get("template_slots")
    workflows = data.get("workflows")
    if not isinstance(slots, list) or not slots:
        raise ValueError("template_slots must be non-empty")
    if not isinstance(workflows, list) or not workflows:
        raise ValueError("workflows must be non-empty")
    slot_ids: set[str] = set()
    verified = 0
    for index, slot in enumerate(slots):
        document_id = str(slot.get("document_id", ""))
        if not ID_RE.fullmatch(document_id) or document_id in slot_ids:
            raise ValueError(f"invalid or duplicate template_slots[{index}].document_id")
        slot_ids.add(document_id)
        fields = slot.get("required_fields")
        references = slot.get("referenced_fields")
        if (
            not isinstance(fields, list)
            or len(fields) != len(set(fields))
            or any(not FIELD_RE.fullmatch(str(field)) for field in fields)
        ):
            raise ValueError(f"invalid required_fields in {document_id}")
        if (
            not isinstance(references, list)
            or len(references) != len(set(references))
            or any(not FIELD_RE.fullmatch(str(field)) for field in references)
        ):
            raise ValueError(f"invalid referenced_fields in {document_id}")
        template_path = slot.get("template_path")
        expected = slot.get("sha256")
        if template_path is None and expected is None:
            continue
        if not isinstance(template_path, str) or not isinstance(expected, str) or not SHA_RE.fullmatch(expected):
            raise ValueError(f"{document_id}: template_path and lowercase sha256 must be set together")
        file_path = safe_file(pack_root, template_path)
        if file_path.suffix.lower() not in {".docx", ".docm"} or not file_path.is_file():
            raise ValueError(f"{document_id}: template file missing or unsupported")
        actual = sha256_file(file_path)
        if actual != expected:
            raise ValueError(f"{document_id}: SHA-256 mismatch")
        actual_fields = extract_docx_field_references(file_path)
        declared_fields = set(map(str, references))
        required_fields = set(map(str, fields))
        if declared_fields != actual_fields:
            missing = sorted(actual_fields - declared_fields)
            stale = sorted(declared_fields - actual_fields)
            raise ValueError(
                f"{document_id}: referenced_fields mismatch; missing={missing}, stale={stale}"
            )
        if not required_fields.issubset(actual_fields):
            stale_required = sorted(required_fields - actual_fields)
            raise ValueError(
                f"{document_id}: required_fields are absent from DOCX: {stale_required}"
            )
        if data["status"] in {"starter", "pilot", "approved"} and required_fields != actual_fields:
            optional = sorted(actual_fields - required_fields)
            raise ValueError(
                f"{document_id}: populated strict template has unclassified placeholders: {optional}"
            )
        verified += 1
    for workflow in workflows:
        docs = workflow.get("documents")
        if not isinstance(docs, list) or not docs or any(doc not in slot_ids for doc in docs):
            raise ValueError(f"workflow {workflow.get('workflow_id')!r} references unknown documents")
    if data["status"] in {"starter", "pilot", "approved"} and verified != len(slots):
        raise ValueError(f"status {data['status']} requires every template slot to be populated and hash-verified")
    if data["status"] == "starter":
        if data.get("usage_mode") != "draft_only" or data.get("requires_organization_review") is not True:
            raise ValueError("starter pack must be draft_only and require organization review")
    if data["status"] == "approved" and not data.get("publisher", {}).get("reviewed_by"):
        raise ValueError("approved pack requires at least one named reviewer")
    return {"pack_id": data["pack_id"], "status": data["status"], "slots": len(slots), "verified_templates": verified}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("pack_root", type=Path)
    args = parser.parse_args()
    result = validate(args.pack_root.resolve())
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
