from __future__ import annotations

import pytest

from scripts.ed25519_compat import BadSignatureError, SigningKey, VerifyKey


def test_seed_round_trip_and_deterministic_public_key() -> None:
    seed = bytes(range(32))
    first = SigningKey(seed)
    second = SigningKey(seed)
    assert bytes(first) == seed
    assert bytes(first.verify_key) == bytes(second.verify_key)
    assert len(bytes(first.verify_key)) == 32


def test_signature_verifies_and_returns_original_message() -> None:
    key = SigningKey.generate()
    message = b"dokkomplekt-release-attestation"
    signed = key.sign(message)
    assert len(signed.signature) == 64
    assert key.verify_key.verify(message, signed.signature) == message


def test_combined_signed_message_compatibility() -> None:
    key = SigningKey.generate()
    message = b"combined-message"
    signed = key.sign(message)
    assert key.verify_key.verify(bytes(signed)) == message


def test_tampered_message_and_signature_fail_closed() -> None:
    key = SigningKey.generate()
    signed = key.sign(b"original")
    with pytest.raises(BadSignatureError):
        key.verify_key.verify(b"tampered", signed.signature)
    corrupted = bytearray(signed.signature)
    corrupted[-1] ^= 0x01
    with pytest.raises(BadSignatureError):
        key.verify_key.verify(b"original", bytes(corrupted))


def test_invalid_key_lengths_are_rejected() -> None:
    with pytest.raises(ValueError):
        SigningKey(b"short")
    with pytest.raises(ValueError):
        VerifyKey(b"short")
