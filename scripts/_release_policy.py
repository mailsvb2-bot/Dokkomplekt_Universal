#!/usr/bin/env python3
"""Shared fail-closed validation for production release URLs and provenance."""
from __future__ import annotations

import ipaddress
import re
from urllib.parse import urlparse

_RESERVED_EXACT = {
    "localhost",
    "localhost.localdomain",
    "example.com",
    "example.net",
    "example.org",
}
_RESERVED_SUFFIXES = (
    ".localhost",
    ".invalid",
    ".test",
    ".example",
    ".local",
)
_DNS_LABEL = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$")


def _normalized_host(raw: str, label: str) -> str:
    try:
        encoded = raw.rstrip(".").encode("idna").decode("ascii").lower()
    except UnicodeError as exc:
        raise ValueError(f"{label}: host is not valid IDNA") from exc
    if not encoded or len(encoded) > 253:
        raise ValueError(f"{label}: host is invalid")
    return encoded


def _reject_non_public_host(host: str, label: str) -> None:
    if host in _RESERVED_EXACT or any(host.endswith(suffix) for suffix in _RESERVED_SUFFIXES):
        raise ValueError(f"{label}: placeholder or local host is forbidden")
    if host.endswith(".example.com") or host.endswith(".example.net") or host.endswith(".example.org"):
        raise ValueError(f"{label}: documentation-only example host is forbidden")
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        labels = host.split(".")
        if len(labels) < 2 or any(not _DNS_LABEL.fullmatch(part) for part in labels):
            raise ValueError(f"{label}: host must be a valid public DNS name")
        return
    if (
        address.is_private
        or address.is_loopback
        or address.is_link_local
        or address.is_multicast
        or address.is_unspecified
        or address.is_reserved
    ):
        raise ValueError(f"{label}: private, local or reserved IP address is forbidden")


def validate_public_https_url(value: str, label: str) -> str:
    """Return the normalized input after static public-HTTPS validation.

    DNS is deliberately not resolved here; runtime download code performs address
    resolution and pinning immediately before network access.
    """
    parsed = urlparse(value)
    if parsed.scheme.lower() != "https" or not parsed.netloc or not parsed.hostname:
        raise ValueError(f"{label}: must be a real public HTTPS URL")
    if parsed.username or parsed.password:
        raise ValueError(f"{label}: credentials are forbidden")
    if parsed.fragment:
        raise ValueError(f"{label}: fragment is forbidden")
    try:
        _ = parsed.port
    except ValueError as exc:
        raise ValueError(f"{label}: port is invalid") from exc
    host = _normalized_host(parsed.hostname, label)
    _reject_non_public_host(host, label)
    return value


def validate_source_reference(value: str, label: str) -> str:
    """Validate immutable artifact provenance as public HTTPS or a non-empty URN."""
    parsed = urlparse(value)
    if parsed.scheme.lower() == "urn":
        if not parsed.path or parsed.query or parsed.fragment:
            raise ValueError(f"{label}: URN must be non-empty and contain no query or fragment")
        return value
    return validate_public_https_url(value, label)
