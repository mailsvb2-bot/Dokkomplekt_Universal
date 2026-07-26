from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "crates" / "dokkomplekt-license-server" / "src" / "config.rs"
POSTGRES = ROOT / "crates" / "dokkomplekt-license-server" / "src" / "storage" / "postgres.rs"
ENV = ROOT / ".env.example"


def test_license_server_rejects_remote_notls_before_connect() -> None:
    config = CONFIG.read_text(encoding="utf-8")
    postgres = POSTGRES.read_text(encoding="utf-8")
    assert "validate_database_transport(database_url, strict_runtime)?" in config
    assert "DatabaseEndpoint::Remote(host)" in config
    assert "remote PostgreSQL host" in config
    assert "crate::config::validate_database_transport(" in postgres
    assert postgres.index("validate_database_transport") < postgres.index("Client::connect")


def test_production_database_example_uses_unix_socket() -> None:
    env = ENV.read_text(encoding="utf-8")
    assert "DATABASE_URL=postgresql:///dokkomplekt?host=/var/run/postgresql" in env
    assert "DATABASE_URL=postgresql://user:password@db.example.com" not in env


def test_development_loopback_is_explicitly_separate_from_production() -> None:
    config = CONFIG.read_text(encoding="utf-8")
    assert "DatabaseEndpoint::Loopback if !strict_runtime => Ok(())" in config
    assert "production license server requires PostgreSQL through a local Unix-domain socket" in config
