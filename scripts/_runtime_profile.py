"""Canonical Windows offline-runtime profile policy.

The stock NSIS installer is bounded to the document-processing runtime. The
semantic runtime/model remains a separately verified optional component so a
large GGUF never makes normal document installation depend on semantic payloads.
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
})
SEMANTIC_TOOLS = frozenset({"llama_cpp", "semantic_model"})
RUNTIME_TOOLS = CORE_TOOLS | SEMANTIC_TOOLS


def normalize_profile(value: object, *, semantic_model_required: object | None = None) -> str:
    """Return a validated profile while preserving legacy full payloads.

    Legacy signed runtime payloads did not carry ``runtime_profile`` but did
    require ``semantic_model_required=true``. They therefore map only to
    ``full``. A non-semantic/core signed payload must opt in explicitly.
    """
    if isinstance(value, str) and value.strip():
        profile = value.strip().lower()
        if profile not in PROFILES:
            raise ValueError(f"unsupported runtime profile: {profile!r}")
    elif semantic_model_required is True:
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
    return normalize_profile(
        profile, semantic_model_required=(profile == FULL_PROFILE)
    ) == FULL_PROFILE


def profile_tools(profile: str) -> frozenset[str]:
    normalized = normalize_profile(
        profile, semantic_model_required=(profile == FULL_PROFILE)
    )
    return RUNTIME_TOOLS if normalized == FULL_PROFILE else CORE_TOOLS


def include_tool(profile: str, tool: object) -> bool:
    name = str(tool).strip().lower()
    if not name:
        raise ValueError("runtime file is missing its tool identifier")
    if name not in RUNTIME_TOOLS:
        raise ValueError(f"unsupported external Windows runtime component: {name}")
    return name in profile_tools(profile)


def validate_profile_file_set(profile: str, files: list[dict[str, Any]]) -> None:
    normalized = normalize_profile(
        profile, semantic_model_required=(profile == FULL_PROFILE)
    )
    tools = {str(item.get("tool", "")).strip().lower() for item in files}
    if "" in tools:
        raise ValueError("runtime file is missing its tool identifier")
    expected = set(profile_tools(normalized))
    missing = sorted(expected - tools)
    extra = sorted(tools - expected)
    if missing or extra:
        raise ValueError(
            f"runtime profile {normalized!r} component set mismatch: "
            f"missing={missing}; extra={extra}"
        )
