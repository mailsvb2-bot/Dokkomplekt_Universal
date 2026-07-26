from __future__ import annotations

import importlib.util
import sqlite3
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load_queue_module():
    module_path = ROOT / "scripts" / "queue_mtls_service.py"
    spec = importlib.util.spec_from_file_location("dokkomplekt_queue_exact_once", module_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_two_workers_crash_recovery_publishes_exactly_once() -> None:
    module = load_queue_module()
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        database = root / "queue.sqlite3"
        publication = root / "published-kit.json"
        store = module.QueueStore(database, lease_seconds=60)
        digest = "d" * 64
        identities = {"worker-a": "a" * 64, "worker-b": "b" * 64}

        def acquire(worker: str) -> tuple[str, str]:
            return worker, store.acquire(digest, worker, False, identities[worker]).decision

        with ThreadPoolExecutor(max_workers=2) as pool:
            decisions = dict(pool.map(acquire, identities))
        assert sorted(decisions.values()) == ["acquired", "busy"]
        winner = next(worker for worker, decision in decisions.items() if decision == "acquired")
        recovery = next(worker for worker in identities if worker != winner)

        # Simulate a worker disappearing without a retryable/complete call. The
        # service clock would expire this lease in production; forcing the DB
        # timestamp avoids a minute-long unit test while exercising the same path.
        with sqlite3.connect(database) as connection:
            connection.execute(
                "UPDATE processing_queue SET lease_until=0 WHERE source_sha256=? AND worker_id=?",
                (digest, winner),
            )
        assert store.acquire(digest, recovery, False, identities[recovery]).decision == "acquired"

        publication.write_text('{"case":"one","worker":"recovery"}', encoding="utf-8")
        assert store.complete(digest, recovery, identities[recovery]).decision == "ok"
        try:
            store.complete(digest, winner, identities[winner])
        except module.QueueError:
            pass
        else:
            raise AssertionError("expired worker must not complete or republish the case")

        assert store.acquire(digest, "worker-c", False, "c" * 64).decision == "completed"
        assert publication.read_text(encoding="utf-8").count('"case"') == 1
        with sqlite3.connect(database) as connection:
            row = connection.execute(
                "SELECT status, worker_id, completed_at FROM processing_queue WHERE source_sha256=?",
                (digest,),
            ).fetchone()
        assert row is not None
        assert row[0] == "completed"
        assert row[1] == recovery
        assert row[2] is not None
