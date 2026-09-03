"""Canonical Windows offline-runtime profile policy.

The stock NSIS installer is intentionally bounded to the document-processing
runtime.  The semantic model is a separately verified optional component: the
currently approved candidate is larger than the stock NSIS data limit and must
never make the core application un-installable.
"""
from __future__ import annotations

from typing import Any

CORE_PROFILE = "core"
FULL_PROFILE = "full"
PROFILES = (CORE_PROFILE, FULL_PROFILE)

CORE_TOOLS = frozenset({
    "tesseract",
    "poppler",
    "libreoffice",
    "sumatrapdf",
    "7zip",
    "msgconvert",
})
SEMANTIC_TOOLS = frozenset({"llama_cpp", "semantic_model"})


def normalize_profile(value: object, *, semantic_model_required: object | None = None) -> str:
    """Return a validated profile, preserving old full payload compatibility.

    Legacy signed production payloads had no ``runtime_profile`` field but were
    required to set ``semantic_model_required=true``.  Those payloads therefore
    map only to ``full``.  A core payload must opt in explicitly.
    """
    if isinstance(value, str) and value.strip():
        profile = value.strip().lower()
        if profile not in PROFILES:
            raise ValueError(f"unsupported runtime profile: {profile!r}")
    else:
        if semantic_model_required is True:
            profile = FULL_PROFILE
        else:
            raise ValueError("runtime_profile is required for a non-semantic runtime payload")
    expected_semantic = profile == FULL_PROFILE
    if semantic_model_required is not None and semantic_model_required is not expected_semantic:
        raise ValueError(
            f"runtime profile {profile!r} requires semantic_model_required={str(expected_semantic).lower()}"
        )
    return profile


def profile_requires_semantic(profile: str) -> bool:
    normalized = normalize_profile(profile, semantic_model_required=(profile == FULL_PROFILE))
    return normalized == FULL_PROFILE


def include_tool(profile: str, tool: object) -> bool:
    normalized = normalize_profile(profile, semantic_model_required=(profile == FULL_PROFILE))
    name = str(tool).strip().lower()
    if not name:
        raise ValueError("runtime file is missing its tool identifier")
    if normalized == FULL_PROFILE:
        return True
    return name not in SEMANTIC_TOOLS


def validate_profile_file_set(profile: str, files: list[dict[str, Any]]) -> None:
    normalized = normalize_profile(profile, semantic_model_required=(profile == FULL_PROFILE))
    tools = {str(item.get("tool", "")).strip().lower() for item in files}
    required = {"tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip"}
    missing = sorted(required - tools)
    if missing:
        raise ValueError(f"runtime profile {normalized!r} is missing core tools: {missing}")
    if normalized == CORE_PROFILE:
        forbidden = sorted(tools & SEMANTIC_TOOLS)
        if forbidden:
            raise ValueError(f"core runtime must not embed semantic tools: {forbidden}")
    else:
        missing_semantic = sorted(SEMANTIC_TOOLS - tools)
        if missing_semantic:
            raise ValueError(f"full runtime is missing semantic tools: {missing_semantic}")
