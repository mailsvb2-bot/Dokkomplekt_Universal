"""Small Ed25519 compatibility layer backed by ``cryptography``.

The project previously depended on both PyNaCl and cryptography for the same
primitive.  This module exposes only the tiny API surface used by release tools
so source and packaged environments share one audited implementation.
"""
from __future__ import annotations

from dataclasses import dataclass

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)


class BadSignatureError(ValueError):
    """Raised when an Ed25519 signature is invalid."""


@dataclass(frozen=True)
class SignedMessage:
    """Minimal result compatible with the project's former PyNaCl usage."""

    message: bytes
    signature: bytes

    def __bytes__(self) -> bytes:
        return self.signature + self.message


class VerifyKey:
    def __init__(self, public_key: bytes):
        raw = bytes(public_key)
        if len(raw) != 32:
            raise ValueError("Ed25519 public key must be exactly 32 bytes")
        self._raw = raw
        self._key = Ed25519PublicKey.from_public_bytes(raw)

    def __bytes__(self) -> bytes:
        return self._raw

    def verify(self, message: bytes, signature: bytes | None = None) -> bytes:
        payload = bytes(message)
        if signature is None:
            if len(payload) < 64:
                raise BadSignatureError("signed message is shorter than an Ed25519 signature")
            signature, payload = payload[:64], payload[64:]
        try:
            self._key.verify(bytes(signature), payload)
        except (InvalidSignature, ValueError) as exc:
            raise BadSignatureError("Ed25519 signature verification failed") from exc
        return payload


class SigningKey:
    def __init__(self, seed: bytes):
        raw = bytes(seed)
        if len(raw) != 32:
            raise ValueError("Ed25519 private seed must be exactly 32 bytes")
        self._seed = raw
        self._key = Ed25519PrivateKey.from_private_bytes(raw)
        public = self._key.public_key().public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw,
        )
        self.verify_key = VerifyKey(public)

    @classmethod
    def generate(cls) -> "SigningKey":
        key = Ed25519PrivateKey.generate()
        seed = key.private_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PrivateFormat.Raw,
            encryption_algorithm=serialization.NoEncryption(),
        )
        return cls(seed)

    def __bytes__(self) -> bytes:
        return self._seed

    def sign(self, message: bytes) -> SignedMessage:
        payload = bytes(message)
        return SignedMessage(message=payload, signature=self._key.sign(payload))
