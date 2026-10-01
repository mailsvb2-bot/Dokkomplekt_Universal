from __future__ import annotations

import argparse
import hashlib
import os
import tempfile
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path, PurePosixPath

REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
CUSTOM_XML_PREFIX = "customxml/"


def _normalized_target(source_part: str, target: str) -> str:
    target = target.replace("\\", "/")
    if target.startswith("/"):
        return target.lstrip("/").lower()
    base = PurePosixPath(source_part).parent
    parts: list[str] = []
    for item in (base / target).parts:
        if item in ("", "."):
            continue
        if item == "..":
            if parts:
                parts.pop()
            continue
        parts.append(item)
    return "/".join(parts).lower()


def _relationship_source_part(name: str) -> str:
    path = PurePosixPath(name)
    if path.name == ".rels" and str(path.parent) == "_rels":
        return ""
    parent = path.parent
    if parent.name != "_rels" or not path.name.endswith(".rels"):
        return ""
    source_name = path.name[:-5]
    return str(parent.parent / source_name)


def _strip_custom_xml_relationships(name: str, payload: bytes) -> bytes:
    try:
        root = ET.fromstring(payload)
    except ET.ParseError as exc:
        raise ValueError(f"invalid relationship XML {name}: {exc}") from exc
    source_part = _relationship_source_part(name)
    removed = 0
    for child in list(root):
        if child.tag.rsplit("}", 1)[-1] != "Relationship":
            continue
        target = child.attrib.get("Target", "")
        rel_type = child.attrib.get("Type", "").lower()
        if _normalized_target(source_part, target).startswith(CUSTOM_XML_PREFIX) or rel_type.endswith("/customxml"):
            root.remove(child)
            removed += 1
    if not removed:
        return payload
    ET.register_namespace("", REL_NS)
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)


def _strip_custom_xml_content_types(payload: bytes) -> bytes:
    try:
        root = ET.fromstring(payload)
    except ET.ParseError as exc:
        raise ValueError(f"invalid [Content_Types].xml: {exc}") from exc
    removed = 0
    for child in list(root):
        if child.tag.rsplit("}", 1)[-1] != "Override":
            continue
        part = child.attrib.get("PartName", "").lstrip("/").lower()
        if part.startswith(CUSTOM_XML_PREFIX):
            root.remove(child)
            removed += 1
    if not removed:
        return payload
    ET.register_namespace("", CT_NS)
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)


def sanitize_docx_hidden_carriers(source: Path, destination: Path) -> dict[str, object]:
    if source.suffix.lower() not in {".docx", ".docm"}:
        raise ValueError("only DOCX/DOCM packages are supported")
    before_story: dict[str, str] = {}
    removed_parts: list[str] = []
    rewritten_parts: list[str] = []

    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(source, "r") as reader, zipfile.ZipFile(destination, "w") as writer:
        for info in reader.infolist():
            name = info.filename.replace("\\", "/")
            lower = name.lower()
            payload = reader.read(info)

            if lower.startswith(CUSTOM_XML_PREFIX):
                removed_parts.append(name)
                continue

            if lower.startswith("word/") and lower.endswith(".xml") and "/_rels/" not in lower:
                before_story[name] = hashlib.sha256(payload).hexdigest()

            replacement = payload
            if lower == "[content_types].xml":
                replacement = _strip_custom_xml_content_types(payload)
            elif lower.endswith(".rels"):
                replacement = _strip_custom_xml_relationships(name, payload)
            if replacement != payload:
                rewritten_parts.append(name)

            clone = zipfile.ZipInfo(info.filename, info.date_time)
            clone.compress_type = info.compress_type
            clone.comment = info.comment
            clone.extra = info.extra
            clone.create_system = info.create_system
            clone.create_version = info.create_version
            clone.extract_version = info.extract_version
            clone.flag_bits = info.flag_bits
            clone.volume = info.volume
            clone.internal_attr = info.internal_attr
            clone.external_attr = info.external_attr
            writer.writestr(clone, replacement)

    with zipfile.ZipFile(destination, "r") as check:
        names = [item.filename.replace("\\", "/") for item in check.infolist()]
        leaked = [name for name in names if name.lower().startswith(CUSTOM_XML_PREFIX)]
        if leaked:
            raise RuntimeError(f"customXml parts survived sanitization: {leaked}")
        after_story = {
            name: hashlib.sha256(check.read(name)).hexdigest()
            for name in before_story
        }
        if after_story != before_story:
            raise RuntimeError("visible Word story bytes changed during hidden-carrier sanitization")
        for name in names:
            lower = name.lower()
            if lower.endswith(".rels"):
                payload = check.read(name)
                root = ET.fromstring(payload)
                source_part = _relationship_source_part(name)
                for child in root:
                    if child.tag.rsplit("}", 1)[-1] != "Relationship":
                        continue
                    target = child.attrib.get("Target", "")
                    rel_type = child.attrib.get("Type", "").lower()
                    if _normalized_target(source_part, target).startswith(CUSTOM_XML_PREFIX) or rel_type.endswith("/customxml"):
                        raise RuntimeError(f"dangling customXml relationship survived in {name}")
            elif lower == "[content_types].xml":
                root = ET.fromstring(check.read(name))
                for child in root:
                    if child.tag.rsplit("}", 1)[-1] == "Override":
                        part = child.attrib.get("PartName", "").lstrip("/").lower()
                        if part.startswith(CUSTOM_XML_PREFIX):
                            raise RuntimeError("customXml content type survived sanitization")

    return {
        "removed_parts": sorted(removed_parts),
        "rewritten_parts": sorted(rewritten_parts),
        "story_sha256": before_story,
    }


def sanitize_in_place(path: Path) -> dict[str, object]:
    fd, raw_temp = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".sanitized", dir=path.parent)
    os.close(fd)
    temp = Path(raw_temp)
    try:
        report = sanitize_docx_hidden_carriers(path, temp)
        os.replace(temp, path)
        return report
    finally:
        temp.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--in-place", action="store_true")
    args = parser.parse_args()
    if args.in_place == bool(args.output):
        parser.error("choose exactly one of --in-place or --output")
    report = sanitize_in_place(args.source) if args.in_place else sanitize_docx_hidden_carriers(args.source, args.output)
    print(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
