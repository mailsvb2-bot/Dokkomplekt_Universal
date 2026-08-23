from __future__ import annotations

import importlib.util
import json
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import time
import unittest
from http.client import HTTPSConnection
from pathlib import Path

from source_helpers import ROOT, project_text as text


class V1830HardeningContracts(unittest.TestCase):
    def test_desktop_queue_has_no_cleartext_postgres_path(self) -> None:
        queue = text("src-tauri/src/central_queue.rs")
        cargo = text("src-tauri/Cargo.toml")
        self.assertNotIn("NoTls", queue)
        self.assertNotIn("postgres::", queue)
        self.assertNotIn("postgres.workspace = true", cargo)
        self.assertIn("reject_legacy_database_transport", queue)
        self.assertIn("DOKKOMPLEKT_QUEUE_MTLS_URL", queue)
        self.assertIn(".https_only(true)", queue)
        self.assertIn(".identity(identity)", queue)
        self.assertIn("redirect(reqwest::redirect::Policy::none())", queue)

    def test_mtls_queue_service_enforces_client_cert_and_atomic_leases(self) -> None:
        service = text("scripts/queue_mtls_service.py")
        for invariant in [
            "ssl.CERT_REQUIRED",
            "TLSVersion.TLSv1_2",
            "BEGIN IMMEDIATE",
            "PRAGMA journal_mode=WAL",
            "PRAGMA synchronous=FULL",
            "worker_id=? AND client_identity=? AND status='processing'",
            "MAX_BODY_BYTES",
            "client_identity",
            "getpeercert(binary_form=True)",
            "allow_completed_reissue must be boolean",
        ]:
            self.assertIn(invariant, service)

    def test_auto_routing_learning_print_triage_and_parallelism_are_wired(self) -> None:
        routing = text("crates/dokkomplekt-core/src/document_routing.rs")
        triage = text("crates/dokkomplekt-core/src/print_triage.rs")
        desktop = text("src-tauri/src/subsystems/desktop_io.rs")
        runtime = text("src-tauri/src/subsystems/automation_runtime.rs")
        watcher = text("src-tauri/src/subsystems/watcher_commands.rs")
        main = text("src-tauri/src/main.rs")
        for invariant in [
            "recommend_document_bundle",
            "stable_cluster_id",
            "related_document_roles",
            "clear_margin",
            "review_required",
        ]:
            self.assertIn(invariant, routing)
        for invariant in [
            "evaluate_print_triage",
            "auto_print_allowed",
            "unapproved_document_ids",
            "missing_fields",
            "confidence_score",
        ]:
            self.assertIn(invariant, triage)
        self.assertIn("template_revision_approvals_v1", desktop)
        self.assertIn("print-review-queue", desktop)
        self.assertIn("print_review_record_v2:", desktop)
        self.assertIn("encrypted_payload", desktop)
        self.assertIn("LEARNED_SCANNER_RULES_LOCK", desktop)
        self.assertIn("learning_status", main)
        self.assertIn("shadow_observations", main)
        self.assertIn("promoted_at", main)
        self.assertIn("automatic_print_review_queued", runtime)
        self.assertIn("max_parallel_cases", watcher)
        self.assertIn("normalize_parallel_cases", watcher)
        self.assertIn("active.len() < max_parallel_cases", watcher)

    def test_roi_is_measured_and_not_presented_as_observed_human_time(self) -> None:
        storage = text("crates/dokkomplekt-storage/src/lib.rs")
        runtime = text("src-tauri/src/subsystems/automation_runtime.rs")
        ui = text("src/components/AutomationControlCenter.tsx")
        self.assertIn("processing_milliseconds", storage)
        self.assertIn("print_review_queued", storage)
        self.assertIn("automatic_print_approved", storage)
        self.assertIn("intake_roi_measured", runtime)
        self.assertIn("organization_baseline_minus_measured_runtime", runtime)
        self.assertIn("Время автоматической обработки", ui)
        self.assertIn("норма ручной работы − фактическое время обработки", ui)
        self.assertIn("не является фактически сэкономленным временем без замера", ui)

    def test_registry_adapter_is_local_validated_and_user_confirmed(self) -> None:
        registry = text("src-tauri/src/subsystems/business_registry.rs")
        for invariant in [
            'validate_field_value("org.inn"',
            'validate_field_value("org.kpp"',
            'validate_field_value("org.ogrn"',
            "dokkomplekt.1c-counterparty-exchange.v1",
            "set_user_value",
            "fields_confirmed_by_user_action",
            "business_registry_record_applied",
            "business_registry_record_v2:",
            "business_registry_index_v2",
            "BUSINESS_REGISTRY_LOCK",
        ]:
            self.assertIn(invariant, registry)

    def test_tier1_approval_is_signed_and_exact_revision_bound(self) -> None:
        from scripts.ed25519_compat import SigningKey
        import base64
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            key = SigningKey.generate()
            seed = base / "seed.b64"
            public = base / "public.b64"
            approval = base / "approval.json"
            seed.write_text(base64.b64encode(bytes(key)).decode("ascii"), "utf-8")
            public.write_text(base64.b64encode(bytes(key.verify_key)).decode("ascii"), "utf-8")
            command = [
                sys.executable, str(ROOT / "scripts/approve_content_pack.py"), "create",
                "--pack", str(ROOT / "content-packs/tier1-legal-ru"),
                "--organization", "ООО Тест",
                "--reviewer", "Иванов И.И.",
                "--jurisdiction", "Российская Федерация",
                "--legal-basis", "Внутреннее профильное ревью",
                "--review-scope", "Все формы пакета",
                "--valid-until", "2027-12-31",
                "--signing-key", str(seed),
                "--output", str(approval),
            ]
            subprocess.run(command, check=True, cwd=ROOT)
            subprocess.run([
                sys.executable, str(ROOT / "scripts/approve_content_pack.py"), "verify",
                "--pack", str(ROOT / "content-packs/tier1-legal-ru"),
                "--approval", str(approval),
                "--trusted-public-key", str(public),
            ], check=True, cwd=ROOT)
            payload = json.loads(approval.read_text("utf-8"))["payload"]
            self.assertEqual(payload["production_assertion"], "approved_for_named_organization_and_jurisdiction_only")
            self.assertTrue(all(len(item["sha256"]) == 64 for item in payload["templates"]))

    def test_runtime_lock_stages_model_ocr_and_license_evidence(self) -> None:
        target = "test-runtime-lock"
        staged = ROOT / "src-tauri" / "resources" / "tools" / target
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            license_file = base / "LICENSE.txt"
            license_file.write_text("approved test notice", "utf-8")
            specs = [
                ("tesseract", "tesseract.exe", "tesseract/tesseract.exe", True),
                ("tesseract", "rus.traineddata", "tesseract/tessdata/rus.traineddata", False),
                ("tesseract", "eng.traineddata", "tesseract/tessdata/eng.traineddata", False),
                ("poppler", "pdftotext.exe", "poppler/pdftotext.exe", True),
                ("poppler", "pdftoppm.exe", "poppler/pdftoppm.exe", True),
                ("libreoffice", "soffice.exe", "libreoffice/soffice.exe", True),
                ("sumatrapdf", "SumatraPDF.exe", "sumatrapdf/SumatraPDF.exe", True),
                ("7zip", "7z.exe", "7zip/7z.exe", True),
                ("llama_cpp", "llama-server.exe", "llama_cpp/llama-server.exe", True),
                ("semantic_model", "model.gguf", "semantic_model/model.gguf", False),
            ]
            artifacts = []
            for tool, name, runtime_target, executable in specs:
                source = base / name
                source.write_bytes(f"{tool}:{name}".encode())
                artifacts.append({
                    "tool": tool,
                    "source": str(source),
                    "target": runtime_target,
                    "executable": executable,
                    "version": "1.0.0-test",
                    "source_url": "https://downloads.dokkomplekt.ru/reviewed-artifact",
                    "license": "Test-License",
                    "license_file": str(license_file),
                })
            catalog = base / "catalog.json"
            catalog.write_text(json.dumps({"schema": 1, "target": target, "artifacts": artifacts}), "utf-8")
            lock = base / "runtime-lock.json"
            subprocess.run([sys.executable, str(ROOT / "scripts/create_runtime_lock.py"), str(catalog), "--output", str(lock)], check=True, cwd=ROOT)
            subprocess.run([sys.executable, str(ROOT / "scripts/prepare_sidecars.py"), str(lock), "--clean"], check=True, cwd=ROOT)
            subprocess.run([
                sys.executable,
                str(ROOT / "scripts/assert_offline_runtime_ready.py"),
                "--target", target,
                "--require-semantic-model",
                "--require-supply-chain",
            ], check=True, cwd=ROOT)
            status = json.loads((staged / "sidecar-status.json").read_text("utf-8"))
            self.assertTrue(status["supply_chain_locked"])
            self.assertTrue(all("license_path" in item for item in status["files"]))
            self.assertTrue(any((staged / item["license_path"]).is_file() for item in status["files"]))
            self.assertTrue(any(item["tool"] == "7zip" for item in status["files"]))
            self.assertIn("--production", text("BUILD_WINDOWS_INSTALLER.bat"))
            self.assertIn("probe_offline_runtime.py", text("BUILD_WINDOWS_INSTALLER.bat"))
        shutil.rmtree(staged, ignore_errors=True)

    def test_rustsec_release_evidence_is_bound_to_lock_report_and_database_commit(self) -> None:
        evidence = text("scripts/write_rustsec_evidence.py")
        attestation = text("scripts/write_cargo_gate_attestation.py")
        release = text("scripts/assert_release_ready.py")
        shell = text("scripts/prepackage_rust_gate.sh")
        bat = text("scripts/prepackage_rust_gate.bat")
        for invariant in [
            "RUSTSEC_AUDIT.json",
            "RUSTSEC_EVIDENCE.json",
            "advisory_database_commit",
            "advisory_database_dirty",
            "cargo_lock_sha256",
            "audit_report_sha256",
            "cargo_audit_version",
        ]:
            self.assertIn(invariant, evidence)
        self.assertIn("cargo audit --deny warnings --json", shell)
        self.assertIn("cargo audit --deny warnings --json", bat)
        self.assertIn("dokkomplekt.cargo-gate.v3", attestation)
        self.assertIn("rustsec_evidence_sha256", attestation)
        self.assertIn("advisory_database_commit", attestation)
        self.assertIn("RUSTSEC_EVIDENCE.json", release)
        self.assertIn("RUSTSEC_AUDIT.json", release)

    def test_tauri_command_registry_matches_thin_typescript_api(self) -> None:
        import re
        main = text("src-tauri/src/main.rs")
        api = text("src/lib/api.ts")
        handler = re.search(r"generate_handler!\s*\[([^\]]+)\]", main, re.DOTALL)
        self.assertIsNotNone(handler)
        backend = {item.strip() for item in handler.group(1).split(",") if item.strip()}
        api_block = re.search(r"rustCommandNames\s*=\s*\[([^\]]+)\]", api, re.DOTALL)
        self.assertIsNotNone(api_block)
        frontend = set(re.findall(r"['\"]([a-zA-Z0-9_]+)['\"]", api_block.group(1)))
        self.assertEqual(backend, frontend)
        self.assertEqual(len(backend), 116)

    def test_python_contract_runner_is_process_isolated_and_bounded(self) -> None:
        runner = text("scripts/run_python_contracts_sharded.py")
        package = json.loads(text("package.json"))
        for invariant in [
            "start_new_session",
            "CREATE_NEW_PROCESS_GROUP",
            "taskkill",
            "os.killpg",
            "--timeout-seconds",
            "dokkomplekt.python-contract-shards.v1",
            "source_unchanged_during_run",
            "source_fingerprint()",
        ]:
            self.assertIn(invariant, runner)
        self.assertEqual(
            package["scripts"]["test:python"],
            "python scripts/run_python_contracts_sharded.py",
        )

    def test_vendor_patch_artifacts_are_absent(self) -> None:
        leftovers = list((ROOT / "vendor").rglob("*.orig"))
        self.assertEqual(leftovers, [])

    def test_mtls_queue_service_rejects_anonymous_clients_end_to_end(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            ca_key, ca_cert = base / "ca.key", base / "ca.crt"
            server_key, server_csr, server_cert = base / "server.key", base / "server.csr", base / "server.crt"
            client_key, client_csr, client_cert = base / "client.key", base / "client.csr", base / "client.crt"
            server_ext, client_ext = base / "server.ext", base / "client.ext"
            server_ext.write_text(
                "basicConstraints=critical,CA:FALSE\n"
                "keyUsage=critical,digitalSignature,keyEncipherment\n"
                "extendedKeyUsage=serverAuth\n"
                "subjectAltName=IP:127.0.0.1\n",
                "utf-8",
            )
            client_ext.write_text(
                "basicConstraints=critical,CA:FALSE\n"
                "keyUsage=critical,digitalSignature,keyEncipherment\n"
                "extendedKeyUsage=clientAuth\n",
                "utf-8",
            )

            def openssl(*args: str) -> None:
                subprocess.run(["openssl", *args], check=True, cwd=base, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

            openssl(
                "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                "-keyout", str(ca_key), "-out", str(ca_cert),
                "-subj", "/CN=Dokkomplekt Test CA", "-days", "1",
                "-addext", "basicConstraints=critical,CA:TRUE",
                "-addext", "keyUsage=critical,keyCertSign,cRLSign",
                "-addext", "subjectKeyIdentifier=hash",
            )
            openssl("req", "-newkey", "rsa:2048", "-nodes", "-keyout", str(server_key), "-out", str(server_csr), "-subj", "/CN=127.0.0.1")
            openssl("x509", "-req", "-in", str(server_csr), "-CA", str(ca_cert), "-CAkey", str(ca_key), "-CAcreateserial", "-out", str(server_cert), "-days", "1", "-extfile", str(server_ext))
            openssl("req", "-newkey", "rsa:2048", "-nodes", "-keyout", str(client_key), "-out", str(client_csr), "-subj", "/CN=worker-a")
            openssl("x509", "-req", "-in", str(client_csr), "-CA", str(ca_cert), "-CAkey", str(ca_key), "-CAcreateserial", "-out", str(client_cert), "-days", "1", "-extfile", str(client_ext))

            with socket.socket() as probe:
                probe.bind(("127.0.0.1", 0))
                port = probe.getsockname()[1]
            process = subprocess.Popen([
                sys.executable, str(ROOT / "scripts/queue_mtls_service.py"),
                "--host", "127.0.0.1", "--port", str(port),
                "--database", str(base / "queue.sqlite3"),
                "--server-cert", str(server_cert), "--server-key", str(server_key),
                "--client-ca", str(ca_cert), "--lease-seconds", "60",
            ], cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            try:
                client_context = ssl.create_default_context(cafile=str(ca_cert))
                client_context.load_cert_chain(str(client_cert), str(client_key))
                deadline = time.monotonic() + 10
                while True:
                    try:
                        connection = HTTPSConnection("127.0.0.1", port, context=client_context, timeout=1)
                        connection.request("GET", "/v1/health")
                        response = connection.getresponse()
                        self.assertEqual(response.status, 200)
                        self.assertEqual(json.loads(response.read())["decision"], "ok")
                        connection.close()
                        break
                    except OSError:
                        if time.monotonic() >= deadline:
                            self.fail("mTLS queue service did not become ready")
                        time.sleep(0.1)

                anonymous_context = ssl.create_default_context(cafile=str(ca_cert))
                anonymous = HTTPSConnection("127.0.0.1", port, context=anonymous_context, timeout=2)
                with self.assertRaises((ssl.SSLError, OSError)):
                    anonymous.request("GET", "/v1/health")
                    anonymous.getresponse()
                anonymous.close()

                payload = json.dumps({"source_sha256": "b" * 64, "worker_id": "worker-a", "allow_completed_reissue": False})
                connection = HTTPSConnection("127.0.0.1", port, context=client_context, timeout=2)
                connection.request("POST", "/v1/queue/acquire", body=payload, headers={"Content-Type": "application/json"})
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                self.assertEqual(json.loads(response.read())["decision"], "acquired")
                connection.close()
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)

    def test_queue_store_lease_semantics(self) -> None:
        module_path = ROOT / "scripts" / "queue_mtls_service.py"
        spec = importlib.util.spec_from_file_location("dokkomplekt_queue_service", module_path)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        import sys
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)  # type: ignore[union-attr]
        with tempfile.TemporaryDirectory() as tmp:
            store = module.QueueStore(Path(tmp) / "queue.sqlite3", lease_seconds=60)
            digest = "a" * 64
            cert_a = "1" * 64
            cert_b = "2" * 64
            self.assertEqual(store.acquire(digest, "worker-a", False, cert_a).decision, "acquired")
            self.assertEqual(store.acquire(digest, "worker-b", False, cert_b).decision, "busy")
            with self.assertRaises(module.QueueError):
                store.complete(digest, "worker-a", cert_b)
            self.assertEqual(store.renew(digest, "worker-a", cert_a).decision, "ok")
            self.assertEqual(store.complete(digest, "worker-a", cert_a).decision, "ok")
            self.assertEqual(store.acquire(digest, "worker-b", False).decision, "completed")
            self.assertEqual(store.acquire(digest, "worker-b", True).decision, "acquired")
            self.assertEqual(store.retryable(digest, "worker-b").decision, "ok")
            self.assertEqual(store.acquire(digest, "worker-c", False).decision, "acquired")


if __name__ == "__main__":
    unittest.main()
