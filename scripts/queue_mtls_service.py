#!/usr/bin/env python3
"""Fail-closed mTLS queue service for Dokkomplekt multi-machine workers.

Desktop clients never receive PostgreSQL credentials and never connect to a
remote database directly. The service exposes one narrow certificate-bound
lease API and can use:

* SQLite for one queue-service instance in a small office;
* PostgreSQL with certificate-verified TLS for production/HA deployments.

The service stores only source SHA-256, lease ownership and technical status.
No document bytes, extracted values or personal data are accepted by the API.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import ssl
import time
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Lock
from typing import Any, Protocol

MAX_BODY_BYTES = 16 * 1024
MAX_DSN_BYTES = 16 * 1024
SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")
WORKER_RE = re.compile(r"^[^\x00-\x1f\x7f]{1,256}$")


class QueueError(ValueError):
    pass


@dataclass(frozen=True)
class QueueDecision:
    decision: str
    message: str = ""


class QueueBackend(Protocol):
    backend_name: str

    def acquire(
        self,
        source_sha256: str,
        worker_id: str,
        allow_completed_reissue: bool,
        client_identity: str = "",
    ) -> QueueDecision: ...

    def renew(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision: ...

    def complete(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision: ...

    def retryable(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision: ...


class QueueStore:
    """SQLite backend for one queue-service process."""

    backend_name = "sqlite"

    def __init__(self, path: Path, lease_seconds: int = 1800) -> None:
        validate_lease_seconds(lease_seconds)
        self.path = path
        self.lease_seconds = lease_seconds
        self._init_lock = Lock()
        self._initialize()

    def _connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self.path, timeout=15, isolation_level=None)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("PRAGMA synchronous=FULL")
        connection.execute("PRAGMA busy_timeout=15000")
        return connection

    def _initialize(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self._init_lock, self._connect() as connection:
            connection.executescript(
                """
                CREATE TABLE IF NOT EXISTS processing_queue (
                    source_sha256 TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    worker_id TEXT NOT NULL,
                    client_identity TEXT NOT NULL DEFAULT '',
                    lease_until INTEGER NOT NULL,
                    completed_at INTEGER,
                    last_error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_processing_queue_status
                    ON processing_queue(status, lease_until);
                """
            )
            columns = {
                str(row["name"])
                for row in connection.execute("PRAGMA table_info(processing_queue)")
            }
            if "client_identity" not in columns:
                connection.execute(
                    "ALTER TABLE processing_queue ADD COLUMN client_identity TEXT NOT NULL DEFAULT ''"
                )
        if os.name != "nt":
            self.path.parent.chmod(0o700)
            self.path.chmod(0o600)

    def acquire(
        self,
        source_sha256: str,
        worker_id: str,
        allow_completed_reissue: bool,
        client_identity: str = "",
    ) -> QueueDecision:
        source_sha256 = validate_sha256(source_sha256)
        worker_id = validate_worker(worker_id)
        client_identity = validate_client_identity(client_identity)
        now = int(time.time())
        lease_until = now + self.lease_seconds
        with self._connect() as connection:
            connection.execute("BEGIN IMMEDIATE")
            try:
                self._cleanup(connection, now)
                row = connection.execute(
                    "SELECT status, lease_until FROM processing_queue WHERE source_sha256=?",
                    (source_sha256,),
                ).fetchone()
                if row is not None:
                    status = str(row["status"])
                    expired = int(row["lease_until"]) <= now
                    if status == "completed" and not allow_completed_reissue:
                        connection.execute("COMMIT")
                        return QueueDecision("completed")
                    if status == "processing" and not expired:
                        connection.execute("COMMIT")
                        return QueueDecision("busy")
                    connection.execute(
                        """UPDATE processing_queue
                           SET status='processing', worker_id=?, client_identity=?, lease_until=?, updated_at=?, last_error=NULL
                           WHERE source_sha256=?""",
                        (worker_id, client_identity, lease_until, now, source_sha256),
                    )
                else:
                    connection.execute(
                        """INSERT INTO processing_queue(
                               source_sha256,status,worker_id,client_identity,lease_until,created_at,updated_at
                           ) VALUES (?,'processing',?,?,?,?,?)""",
                        (source_sha256, worker_id, client_identity, lease_until, now, now),
                    )
                connection.execute("COMMIT")
                return QueueDecision("acquired")
            except Exception:
                connection.execute("ROLLBACK")
                raise

    def renew(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision:
        return self._owned_update(
            source_sha256,
            worker_id,
            client_identity,
            "UPDATE processing_queue SET lease_until=?,updated_at=? "
            "WHERE source_sha256=? AND worker_id=? AND client_identity=? AND status='processing'",
            lambda now: (now + self.lease_seconds, now),
            "Lease центральной очереди потерян; публикация остановлена.",
        )

    def complete(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision:
        return self._owned_update(
            source_sha256,
            worker_id,
            client_identity,
            "UPDATE processing_queue SET status='completed',completed_at=?,lease_until=?,updated_at=? "
            "WHERE source_sha256=? AND worker_id=? AND client_identity=? AND status='processing'",
            lambda now: (now, now, now),
            "Lease центральной очереди потерян до завершения.",
        )

    def retryable(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision:
        return self._owned_update(
            source_sha256,
            worker_id,
            client_identity,
            "UPDATE processing_queue SET status='retryable',lease_until=?,updated_at=?,"
            "last_error='worker exited before completion' "
            "WHERE source_sha256=? AND worker_id=? AND client_identity=? AND status='processing'",
            lambda now: (now, now),
            "Lease уже передан другому worker; retryable не записан.",
        )

    def _owned_update(
        self,
        source_sha256: str,
        worker_id: str,
        client_identity: str,
        sql: str,
        prefix: Any,
        missing_message: str,
    ) -> QueueDecision:
        source_sha256 = validate_sha256(source_sha256)
        worker_id = validate_worker(worker_id)
        client_identity = validate_client_identity(client_identity)
        now = int(time.time())
        values = (*prefix(now), source_sha256, worker_id, client_identity)
        with self._connect() as connection:
            cursor = connection.execute(sql, values)
            if cursor.rowcount != 1:
                raise QueueError(missing_message)
        return QueueDecision("ok")

    @staticmethod
    def _cleanup(connection: sqlite3.Connection, now: int) -> None:
        connection.execute(
            "DELETE FROM processing_queue WHERE status='completed' AND completed_at < ?",
            (now - 90 * 86400,),
        )
        connection.execute(
            "DELETE FROM processing_queue WHERE status='retryable' AND updated_at < ?",
            (now - 30 * 86400,),
        )


class PostgresQueueStore:
    """PostgreSQL backend used only behind the mTLS service boundary."""

    backend_name = "postgresql_tls"

    def __init__(self, dsn: str, lease_seconds: int = 1800) -> None:
        validate_lease_seconds(lease_seconds)
        try:
            import psycopg  # type: ignore[import-not-found]
            from psycopg.conninfo import conninfo_to_dict, make_conninfo  # type: ignore[import-not-found]
        except ImportError as exc:
            raise QueueError(
                "PostgreSQL backend requires psycopg 3; install requirements-queue-server.txt"
            ) from exc
        raw = dsn.strip()
        if not raw or len(raw.encode("utf-8")) > MAX_DSN_BYTES:
            raise QueueError("PostgreSQL DSN is empty or too large")
        settings = conninfo_to_dict(raw)
        validate_postgres_ssl_settings(settings)
        self._psycopg = psycopg
        self.dsn = make_conninfo(raw, connect_timeout=settings.get("connect_timeout", "5"))
        self.lease_seconds = lease_seconds
        self._initialize()

    def _connect(self) -> Any:
        return self._psycopg.connect(self.dsn)

    def _initialize(self) -> None:
        with self._connect() as connection:
            with connection.cursor() as cursor:
                cursor.execute(
                    """
                    CREATE TABLE IF NOT EXISTS processing_queue (
                        source_sha256 TEXT PRIMARY KEY,
                        status TEXT NOT NULL,
                        worker_id TEXT NOT NULL,
                        client_identity TEXT NOT NULL DEFAULT '',
                        lease_until BIGINT NOT NULL,
                        completed_at BIGINT,
                        last_error TEXT,
                        created_at BIGINT NOT NULL,
                        updated_at BIGINT NOT NULL
                    )
                    """
                )
                cursor.execute(
                    "CREATE INDEX IF NOT EXISTS idx_processing_queue_status "
                    "ON processing_queue(status, lease_until)"
                )

    def acquire(
        self,
        source_sha256: str,
        worker_id: str,
        allow_completed_reissue: bool,
        client_identity: str = "",
    ) -> QueueDecision:
        source_sha256 = validate_sha256(source_sha256)
        worker_id = validate_worker(worker_id)
        client_identity = validate_client_identity(client_identity)
        now = int(time.time())
        lease_until = now + self.lease_seconds
        with self._connect() as connection:
            with connection.transaction():
                with connection.cursor() as cursor:
                    self._cleanup(cursor, now)
                    cursor.execute(
                        "SELECT status, lease_until FROM processing_queue "
                        "WHERE source_sha256=%s FOR UPDATE",
                        (source_sha256,),
                    )
                    row = cursor.fetchone()
                    if row is not None:
                        status, current_lease = str(row[0]), int(row[1])
                        if status == "completed" and not allow_completed_reissue:
                            return QueueDecision("completed")
                        if status == "processing" and current_lease > now:
                            return QueueDecision("busy")
                        cursor.execute(
                            "UPDATE processing_queue SET status='processing',worker_id=%s,"
                            "client_identity=%s,lease_until=%s,updated_at=%s,last_error=NULL "
                            "WHERE source_sha256=%s",
                            (worker_id, client_identity, lease_until, now, source_sha256),
                        )
                    else:
                        cursor.execute(
                            "INSERT INTO processing_queue(source_sha256,status,worker_id,"
                            "client_identity,lease_until,created_at,updated_at) "
                            "VALUES (%s,'processing',%s,%s,%s,%s,%s)",
                            (
                                source_sha256,
                                worker_id,
                                client_identity,
                                lease_until,
                                now,
                                now,
                            ),
                        )
        return QueueDecision("acquired")

    def renew(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision:
        now = int(time.time())
        return self._owned_update(
            "UPDATE processing_queue SET lease_until=%s,updated_at=%s "
            "WHERE source_sha256=%s AND worker_id=%s AND client_identity=%s AND status='processing'",
            (now + self.lease_seconds, now),
            source_sha256,
            worker_id,
            client_identity,
            "Lease центральной очереди потерян; публикация остановлена.",
        )

    def complete(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision:
        now = int(time.time())
        return self._owned_update(
            "UPDATE processing_queue SET status='completed',completed_at=%s,lease_until=%s,updated_at=%s "
            "WHERE source_sha256=%s AND worker_id=%s AND client_identity=%s AND status='processing'",
            (now, now, now),
            source_sha256,
            worker_id,
            client_identity,
            "Lease центральной очереди потерян до завершения.",
        )

    def retryable(
        self, source_sha256: str, worker_id: str, client_identity: str = ""
    ) -> QueueDecision:
        now = int(time.time())
        return self._owned_update(
            "UPDATE processing_queue SET status='retryable',lease_until=%s,updated_at=%s,"
            "last_error='worker exited before completion' "
            "WHERE source_sha256=%s AND worker_id=%s AND client_identity=%s AND status='processing'",
            (now, now),
            source_sha256,
            worker_id,
            client_identity,
            "Lease уже передан другому worker; retryable не записан.",
        )

    def _owned_update(
        self,
        sql: str,
        prefix: tuple[Any, ...],
        source_sha256: str,
        worker_id: str,
        client_identity: str,
        missing_message: str,
    ) -> QueueDecision:
        source_sha256 = validate_sha256(source_sha256)
        worker_id = validate_worker(worker_id)
        client_identity = validate_client_identity(client_identity)
        with self._connect() as connection:
            with connection.cursor() as cursor:
                cursor.execute(
                    sql,
                    (*prefix, source_sha256, worker_id, client_identity),
                )
                if cursor.rowcount != 1:
                    raise QueueError(missing_message)
        return QueueDecision("ok")

    @staticmethod
    def _cleanup(cursor: Any, now: int) -> None:
        cursor.execute(
            "DELETE FROM processing_queue WHERE status='completed' AND completed_at < %s",
            (now - 90 * 86400,),
        )
        cursor.execute(
            "DELETE FROM processing_queue WHERE status='retryable' AND updated_at < %s",
            (now - 30 * 86400,),
        )


class QueueHttpServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address: tuple[str, int], store: QueueBackend) -> None:
        super().__init__(address, QueueHandler)
        self.store = store


class QueueHandler(BaseHTTPRequestHandler):
    server_version = "DokkomplektQueue/2"
    protocol_version = "HTTP/1.1"

    @property
    def store(self) -> QueueBackend:
        server = self.server
        if not isinstance(server, QueueHttpServer):
            raise RuntimeError("unexpected server type")
        return server.store

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/v1/health":
            self._write(200, {"decision": "ok", "backend": self.store.backend_name})
        else:
            self._write(404, {"decision": "error", "message": "route not found"})

    def do_POST(self) -> None:  # noqa: N802
        try:
            payload = self._read_json()
            source_sha256 = payload.get("source_sha256")
            worker_id = payload.get("worker_id")
            client_identity = self._client_identity()
            if self.path == "/v1/queue/acquire":
                allowed = {"source_sha256", "worker_id", "allow_completed_reissue"}
                if set(payload) - allowed:
                    raise QueueError("request contains unknown fields")
                allow_completed_reissue = payload.get("allow_completed_reissue", False)
                if not isinstance(allow_completed_reissue, bool):
                    raise QueueError("allow_completed_reissue must be boolean")
                decision = self.store.acquire(
                    source_sha256,
                    worker_id,
                    allow_completed_reissue,
                    client_identity,
                )
            elif self.path == "/v1/queue/renew":
                self._require_lease_fields(payload)
                decision = self.store.renew(source_sha256, worker_id, client_identity)
            elif self.path == "/v1/queue/complete":
                self._require_lease_fields(payload)
                decision = self.store.complete(source_sha256, worker_id, client_identity)
            elif self.path == "/v1/queue/retryable":
                self._require_lease_fields(payload)
                decision = self.store.retryable(source_sha256, worker_id, client_identity)
            else:
                self._write(404, {"decision": "error", "message": "route not found"})
                return
            body = {"decision": decision.decision}
            if decision.message:
                body["message"] = decision.message
            self._write(200, body)
        except QueueError as exc:
            self._write(409, {"decision": "error", "message": str(exc)})
        except (json.JSONDecodeError, TypeError, UnicodeDecodeError) as exc:
            self._write(400, {"decision": "error", "message": f"invalid request: {exc}"})
        except Exception:
            # Do not leak paths, SQL, credentials or certificate details.
            self._write(500, {"decision": "error", "message": "internal queue error"})

    @staticmethod
    def _require_lease_fields(payload: dict[str, Any]) -> None:
        if set(payload) != {"source_sha256", "worker_id"}:
            raise QueueError("lease request must contain exactly source_sha256 and worker_id")

    def _client_identity(self) -> str:
        if not isinstance(self.connection, ssl.SSLSocket):
            raise QueueError("mTLS client identity is unavailable")
        certificate = self.connection.getpeercert(binary_form=True)
        if not certificate:
            raise QueueError("mTLS client certificate is unavailable")
        return hashlib.sha256(certificate).hexdigest()

    def _read_json(self) -> dict[str, Any]:
        self.connection.settimeout(20)
        self.close_connection = True
        raw_length = self.headers.get("Content-Length", "")
        if not raw_length.isdigit():
            raise QueueError("Content-Length is required")
        length = int(raw_length)
        if length <= 0 or length > MAX_BODY_BYTES:
            raise QueueError("request body size is invalid")
        content_type = self.headers.get_content_type()
        if content_type != "application/json":
            raise QueueError("Content-Type must be application/json")
        body = self.rfile.read(length)
        payload = json.loads(body.decode("utf-8"))
        if not isinstance(payload, dict):
            raise QueueError("JSON body must be an object")
        return payload

    def _write(self, status: int, payload: dict[str, Any]) -> None:
        body = (json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n").encode(
            "utf-8"
        )
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format_string: str, *args: Any) -> None:
        peer = self.connection.getpeercert() if isinstance(self.connection, ssl.SSLSocket) else None
        subject = peer.get("subject", ()) if peer else ()
        fingerprint = hashlib.sha256(repr(subject).encode("utf-8")).hexdigest()[:12]
        print(f"queue peer={fingerprint} {format_string % args}")


def validate_sha256(value: Any) -> str:
    if not isinstance(value, str) or not SHA256_RE.fullmatch(value):
        raise QueueError("source_sha256 must be a 64-character hexadecimal SHA-256")
    return value.lower()


def validate_worker(value: Any) -> str:
    if not isinstance(value, str) or not WORKER_RE.fullmatch(value):
        raise QueueError("worker_id is invalid")
    return value


def validate_client_identity(value: Any) -> str:
    if not isinstance(value, str) or (value and not SHA256_RE.fullmatch(value)):
        raise QueueError("client certificate identity is invalid")
    return value.lower()


def validate_lease_seconds(value: int) -> None:
    if value < 60 or value > 24 * 3600:
        raise QueueError("lease_seconds must be between 60 and 86400")


def validate_postgres_ssl_settings(settings: dict[str, str]) -> None:
    sslmode = settings.get("sslmode", "").strip().lower()
    if sslmode not in {"verify-ca", "verify-full"}:
        raise QueueError(
            "PostgreSQL queue backend requires sslmode=verify-full or sslmode=verify-ca"
        )
    root = settings.get("sslrootcert", "").strip()
    if not root:
        raise QueueError("PostgreSQL queue backend requires sslrootcert")
    if root != "system" and not Path(root).expanduser().is_file():
        raise QueueError("PostgreSQL sslrootcert file does not exist")
    cert = settings.get("sslcert", "").strip()
    key = settings.get("sslkey", "").strip()
    if bool(cert) != bool(key):
        raise QueueError("PostgreSQL sslcert and sslkey must be configured together")
    if cert and not Path(cert).expanduser().is_file():
        raise QueueError("PostgreSQL sslcert file does not exist")
    if key and not Path(key).expanduser().is_file():
        raise QueueError("PostgreSQL sslkey file does not exist")
    if key and os.name != "nt" and Path(key).stat().st_mode & 0o077:
        raise QueueError("PostgreSQL sslkey must not be readable by group/others")


def require_file(path: Path, title: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise QueueError(f"{title} not found: {resolved}")
    return resolved


def read_secret_file(path: Path, title: str) -> str:
    resolved = require_file(path, title)
    if resolved.stat().st_size <= 0 or resolved.stat().st_size > MAX_DSN_BYTES:
        raise QueueError(f"{title} has invalid size")
    if os.name != "nt" and resolved.stat().st_mode & 0o077:
        raise QueueError(f"{title} must not be readable by group/others")
    value = resolved.read_text("utf-8").strip()
    if not value:
        raise QueueError(f"{title} is empty")
    return value


def build_tls_context(cert: Path, key: Path, ca: Path) -> ssl.SSLContext:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.verify_mode = ssl.CERT_REQUIRED
    context.check_hostname = False
    context.load_cert_chain(str(cert), str(key))
    context.load_verify_locations(cafile=str(ca))
    context.options |= ssl.OP_NO_COMPRESSION
    return context


def build_store(args: argparse.Namespace) -> QueueBackend:
    if args.database is not None:
        return QueueStore(args.database.expanduser().resolve(), args.lease_seconds)
    if args.postgres_dsn_file is not None:
        dsn = read_secret_file(args.postgres_dsn_file, "PostgreSQL DSN file")
        return PostgresQueueStore(dsn, args.lease_seconds)
    if args.postgres_dsn_env is not None:
        dsn = os.environ.get(args.postgres_dsn_env, "").strip()
        if not dsn:
            raise QueueError(
                f"environment variable {args.postgres_dsn_env} is empty or missing"
            )
        return PostgresQueueStore(dsn, args.lease_seconds)
    raise QueueError("one queue backend must be configured")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=9443)
    backend = parser.add_mutually_exclusive_group(required=True)
    backend.add_argument("--database", type=Path)
    backend.add_argument("--postgres-dsn-file", type=Path)
    backend.add_argument(
        "--postgres-dsn-env",
        metavar="ENV_NAME",
        help="read the PostgreSQL TLS DSN from this environment variable",
    )
    parser.add_argument("--server-cert", type=Path, required=True)
    parser.add_argument("--server-key", type=Path, required=True)
    parser.add_argument("--client-ca", type=Path, required=True)
    parser.add_argument("--lease-seconds", type=int, default=1800)
    args = parser.parse_args()

    cert = require_file(args.server_cert, "server certificate")
    key = require_file(args.server_key, "server private key")
    ca = require_file(args.client_ca, "client CA certificate")
    store = build_store(args)
    server = QueueHttpServer((args.host, args.port), store)
    server.socket = build_tls_context(cert, key, ca).wrap_socket(server.socket, server_side=True)
    print(
        f"Dokkomplekt mTLS queue listening on https://{args.host}:{args.port} "
        f"backend={store.backend_name}"
    )
    try:
        server.serve_forever(poll_interval=0.5)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
