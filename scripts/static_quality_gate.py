from __future__ import annotations
from pathlib import Path
import json
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_DIR_NAMES = {"__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache", "node_modules", "dist", "target"}
REQUIRED_FILES = [
    "Cargo.toml",
    "src-tauri/src/main.rs",
    "src-tauri/tauri.conf.json",
    "crates/dokkomplekt-core/src/lib.rs",
    "crates/dokkomplekt-docx/src/lib.rs",
    "crates/dokkomplekt-storage/src/lib.rs",
    "crates/dokkomplekt-morph/src/lib.rs",
    "crates/dokkomplekt-refdata/src/lib.rs",
    "V18_P0_IMPLEMENTATION_REPORT.md",
    "V18_SCOPE_MATRIX.md",
    "V18_0_4_PROFESSIONAL_POPUP_REPORT.md",
    "RELEASE_VERIFICATION_V18_0_4.md",
    "V18_0_5_GUIDED_SCANNER_REPORT.md",
    "RELEASE_VERIFICATION_V18_0_5.md",
    "V18_0_6_SCANNER_WATCHER_PRINT_FIX_REPORT.md",
    "RELEASE_VERIFICATION_V18_0_6.md",
    "resources/production_calendar_ru.tsv",
    "tests/test_v18_0_4_professional_popup_contracts.py",
    "tests/test_v18_0_5_guided_word_scanner_contracts.py",
    "tests/test_v18_0_6_scanner_watcher_print_contracts.py",
    "docs/TZ_Dokkomplekt_v18.md",
    "docs/LEGACY_MIGRATION_INVENTORY.json",
    "scripts/check_legacy_migration_inventory.py",
    "crates/dokkomplekt-core/data/medical_diary_match_aliases.ru.json",
    "crates/dokkomplekt-core/src/core/parser.rs",
    "crates/dokkomplekt-core/src/core/template_detector.rs",
    "crates/dokkomplekt-core/src/core/field_extractor.rs",
    "crates/dokkomplekt-core/src/core/workflow_contract.rs",
    "crates/dokkomplekt-core/src/core/document_generator.rs",
    "crates/dokkomplekt-core/src/core/validation.rs",
    "crates/dokkomplekt-core/src/core/storage.rs",
    "crates/dokkomplekt-core/src/universal_pipeline.rs",
    "crates/dokkomplekt-core/src/domains/medical.rs",
    "crates/dokkomplekt-core/src/domains/legal.rs",
    "crates/dokkomplekt-core/src/domains/hr.rs",
    "crates/dokkomplekt-core/src/domains/education.rs",
    "crates/dokkomplekt-core/src/domains/accounting.rs",
    "content-packs/catalog.json",
    "src/data/starterPacks.ts",
    "V18_1_3_PILOT_EXPANSION_REPORT.md",
    "RELEASE_VERIFICATION_V18_1_3.md",
    "V18_2_0_COMPONENT_DELIVERY_REPORT.md",
    "RELEASE_VERIFICATION_V18_2_0.md",
    "RELEASE_VERIFICATION_V18_2_1.md",
    "V18_2_1_RESUME_QUEUE_MODULARIZATION_REPORT.md",
    "V18_2_2_ADVERSARIAL_HARDENING_REPORT.md",
    "RELEASE_VERIFICATION_V18_2_2.md",
    "tests/test_v18_2_0_component_delivery_contracts.py",
    "tests/test_v18_2_1_resume_queue_modularization_contracts.py",
    "tests/test_v18_2_2_adversarial_hardening_contracts.py",
    "src-tauri/src/semantic_runtime.rs",
    "tests/windows/verify_reboot_evidence.ps1",
    "src-tauri/src/resume_engine.rs",
    "src-tauri/src/central_queue.rs",
    "src-tauri/src/subsystems/watcher_commands.rs",
    "tests/test_v18_1_3_pilot_expansion_contracts.py",
    "docs/pilot-security-pack/README.md",
    "docs/KEDO_AND_SIGNATURE_INTEGRATION.md",
    "crates/dokkomplekt-core/src/domains/custom.rs",
    "crates/dokkomplekt-core/src/product_access.rs",
    "crates/dokkomplekt-license-core/src/lib.rs",
    "resources/icd10_f.tsv",
    "src/App.tsx",
    "src/lib/api.ts",
    "src/lib/types.ts",
    "src/lib/api.test.ts",
    "src/lib/api.contract.test.ts",
    "scripts/assert_release_ready.py",
    "scripts/generate_update_manifest.py",
    "scripts/build_source_archive.py",
    "SECURITY.md",
    "CONTRIBUTING.md",
    ".env.example",
    "requirements-dev.txt",
    "V18_0_7_SECURITY_UPDATE_RELEASE_REPORT.md",
    "RELEASE_VERIFICATION_V18_0_7.md",
    "tests/test_v18_0_7_security_update_popup_license_contracts.py",
    "V18_0_8_ZERO_TOUCH_HARDENING_REPORT.md",
    "RELEASE_VERIFICATION_V18_0_8.md",
    "tests/test_v18_0_8_zero_touch_hardening_contracts.py",
    "src-tauri/src/workspace_hygiene.rs",
    "scripts/assert_offline_runtime_ready.py",
    "scripts/probe_offline_runtime.py",
    "scripts/write_rustsec_evidence.py",
    "scripts/create_runtime_lock.py",
    "scripts/approve_content_pack.py",
    "content-packs/APPROVAL_WORKFLOW.md",
    "scripts/queue_mtls_service.py",
    "sidecars/runtime-catalog.example.json",
    "tests/test_v18_3_0_hardening_contracts.py",
    "scripts/check_reference_data_freshness.py",
    "docs/ARCHITECTURE_CANONICAL_PIPELINE.md",
    "V18_1_1_OPERATIONAL_HARDENING_REPORT.md",
    "RELEASE_VERIFICATION_V18_1_1.md",
    "tests/test_v18_1_1_operational_hardening_contracts.py",
    "tests/test_v18_1_1_offline_runtime.py",
    "crates/dokkomplekt-core/src/source_classification.rs",
    "src-tauri/src/reference_data_update.rs",
    "scripts/audit_rust_production_panics.py",
    "scripts/create_offline_runtime_bundle.py",
    "scripts/verify_starter_content_packs.py",
    "scripts/verify_offline_runtime_bundle.py",
    "scripts/sign_windows_release.ps1",
    "scripts/write_windows_release_evidence.ps1",
    "tests/windows/windows_hardware_e2e.ps1",
    "tests/fixtures/docx/complex_realistic_template.docx",
    "tests/fixtures/docx/corpus-manifest.json",
    "tests/fixtures/docx/visual-golden.json",
    "scripts/generate_docx_golden_corpus.py",
    "scripts/verify_docx_visual_goldens.py",
    "tests/test_v18_1_2_grounding_distributed_release_contracts.py",
    "tests/test_v18_1_2_offline_runtime_bundle.py",
    "tests/test_v18_1_2_rust_panic_audit.py",
    "V18_1_2_GROUNDING_DISTRIBUTED_RELEASE_REPORT.md",
    "RELEASE_VERIFICATION_V18_1_2.md",
    "V18_3_0_SEMANTIC_AUTOMATION_REPORT.md",
    "RELEASE_VERIFICATION_V18_3_0.md",
    "scripts/run_python_contracts_sharded.py",
    "crates/dokkomplekt-core/src/corpus_recorder.rs",
    "crates/dokkomplekt-core/src/print_triage.rs",
    "src-tauri/src/threshold_calibration.rs",
    "src-tauri/src/universal_intake.rs",
    "src-tauri/src/subsystems/business_registry.rs",
    "docs/QUEUE_SERVICE_DEPLOYMENT.md",
    "docs/ROADMAP_SEMANTIC_MAGIC.md",
    "docs/TECH_SPEC_ALL_PHASES.md",
    "tests/test_v18_3_0_ocr_layout_contracts.py",
    "tests/test_v18_3_0_license_database_transport.py",
    "tests/test_v18_3_0_scanned_pdf_fixture.py",
    "scripts/verify_scanned_pdf_fixture.py",
    "tests/fixtures/ocr/scanned_table_image_only.pdf",
    "tests/fixtures/ocr/scanned_table.tesseract.tsv",
    "tests/fixtures/ocr/scanned_table.expected.json",
]
FORBIDDEN_TS_ENGINES = [
    "src/lib/rewriteCore.ts",
    "src/lib/productAccess.ts",
    "src/lib/dataSchemaEngine.ts",
    "src/lib/domainPluginLayer.ts",
    "src/lib/templateIntelligenceEngine.ts",
    "src/lib/workflowScenarioEngine.ts",
    "src/lib/enginePipeline.ts",
    "src/lib/domainProfiles.ts",
    "src/lib/universalBehavior.ts",
    "src/lib/backgroundAgent.ts",
    "src/lib/medicalDocumentDeep.ts",
    "src/lib/icd10F.ts",
    "src/lib/parityComparison.ts",
]


def fail(message: str) -> None:
    print(message)
    sys.exit(1)

missing = [p for p in REQUIRED_FILES if not (ROOT / p).exists() and not (ROOT / "docs/history" / Path(p).name).exists()]
if missing:
    fail("Missing required files:\n- " + "\n- ".join(missing))

present_ts_engines = [p for p in FORBIDDEN_TS_ENGINES if (ROOT / p).exists()]
if present_ts_engines:
    fail("TypeScript duplicate engines are forbidden; UI must be thin over Tauri commands:\n- " + "\n- ".join(present_ts_engines))

# Prevent the Tauri shell from collapsing back into one unreviewable command monolith.
architecture_files = [ROOT / "src-tauri/src/main.rs"] + sorted((ROOT / "src-tauri/src/subsystems").glob("*.rs"))
for architecture_file in architecture_files:
    line_count = len(architecture_file.read_text(encoding="utf-8").splitlines())
    if line_count >= 3000:
        fail(
            f"Tauri runtime file is too large ({line_count} lines): "
            f"{architecture_file.relative_to(ROOT)}; split responsibility before release"
        )

for rel in ["package.json", "package-lock.json", "src-tauri/tauri.conf.json", "schemas/semantic-case.schema.json"]:
    json.loads((ROOT / rel).read_text(encoding="utf-8"))

expected_version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
package_version = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
tauri_version = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]
if not expected_version or package_version != expected_version or tauri_version != expected_version:
    fail(
        "Version mismatch: "
        f"VERSION={expected_version!r}, package.json={package_version!r}, tauri.conf.json={tauri_version!r}"
    )
for cargo_manifest in [ROOT / "crates" / name / "Cargo.toml" for name in [
    "dokkomplekt-core", "dokkomplekt-docx", "dokkomplekt-storage", "dokkomplekt-morph",
    "dokkomplekt-refdata", "dokkomplekt-license-core", "dokkomplekt-license-server",
    "dokkomplekt-license-python",
]] + [ROOT / "src-tauri" / "Cargo.toml"]:
    manifest = cargo_manifest.read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', manifest, flags=re.MULTILINE)
    if not match or match.group(1) != expected_version:
        fail(f"Version mismatch in {cargo_manifest.relative_to(ROOT)}: expected {expected_version}")
python_binding_manifest = ROOT / "crates/dokkomplekt-license-python/pyproject.toml"
python_binding_text = python_binding_manifest.read_text(encoding="utf-8")
python_binding_version = re.search(r'^version\s*=\s*"([^"]+)"', python_binding_text, flags=re.MULTILINE)
if not python_binding_version or python_binding_version.group(1) != expected_version:
    fail(f"Version mismatch in {python_binding_manifest.relative_to(ROOT)}: expected {expected_version}")

if "--clean-source" in sys.argv:
    forbidden = []
    for path in ROOT.rglob("*"):
        if path.is_dir() and path.name in FORBIDDEN_DIR_NAMES:
            forbidden.append(str(path.relative_to(ROOT)))
    if forbidden:
        fail("Forbidden generated directories in clean source tree:\n" + "\n".join(forbidden[:80]))

# TypeScript must be a command wrapper, not a second source of truth.
api_text = (ROOT / "src/lib/api.ts").read_text(encoding="utf-8")
for forbidden_logic in ["detectRole", "detectCategory", "FIELD_ALIASES", "planLimits(", "validateAccessCode", "searchIcd10F", "buildDeepDiaryCalendar"]:
    if forbidden_logic in api_text:
        fail(f"src/lib/api.ts contains forbidden business logic marker: {forbidden_logic}")

if "@tauri-apps/api/core" not in api_text or "callRust" not in api_text:
    fail("src/lib/api.ts must be a thin Tauri command wrapper")

# Backend command parity: every command registered in Tauri must be exposed by the thin API.
main_rs_for_commands = (ROOT / "src-tauri/src/main.rs").read_text(encoding="utf-8")
handler_match = re.search(r"generate_handler!\s*\[([^\]]+)\]", main_rs_for_commands, flags=re.DOTALL)
if not handler_match:
    fail("Cannot find tauri::generate_handler![...] command registry in src-tauri/src/main.rs")
backend_commands = {
    item.strip()
    for item in handler_match.group(1).split(',')
    if item.strip() and not item.strip().startswith('//')
}
api_commands_match = re.search(r"rustCommandNames\s*=\s*\[([^\]]+)\]", api_text, flags=re.DOTALL)
if not api_commands_match:
    fail("Cannot find rustCommandNames in src/lib/api.ts")
api_commands = set(re.findall(r"['\"]([a-zA-Z0-9_]+)['\"]", api_commands_match.group(1)))
if backend_commands != api_commands:
    missing = sorted(backend_commands - api_commands)
    extra = sorted(api_commands - backend_commands)
    fail("Tauri command surface mismatch between generate_handler! and rustCommandNames.\n"
         + ("Missing in UI api.ts: " + ", ".join(missing) + "\n" if missing else "")
         + ("Extra in UI api.ts: " + ", ".join(extra) if extra else ""))

required_wrapper_names = [
    "prepareTemplateSetup", "confirmTemplateSetup", "getDiaryPlan", "getOutputPlan",
    "applyScanner", "routeIntake", "saveState", "loadState",
]
for wrapper_name in required_wrapper_names:
    if f"function {wrapper_name}" not in api_text:
        fail(f"Missing thin UI wrapper for backend command flow: {wrapper_name}")

app_text = (ROOT / "src/App.tsx").read_text(encoding="utf-8")
# The profile-specific legacy diary command remains available through the thin
# API for medical compatibility, but the universal UI intentionally exposes the
# domain-neutral record-series engine instead of a fake medical-only button.
required_ui_entry_names = [
    "prepareTemplateSetup", "confirmTemplateSetup", "getRecordSeriesPlan",
    "getOutputPlan", "applyScanner", "saveState", "loadState",
]
for wrapper_name in required_ui_entry_names:
    if wrapper_name not in app_text:
        fail(f"Missing UI entry point for backend command flow: {wrapper_name}")

# The new generic core must stay domain-neutral. Profession-specific terms belong in domains/*.
FORBIDDEN_CORE_TERMS = ["диагноз", "лечение", "выпис", "дневник", "medical.", "patient."]
core_dir = ROOT / "crates/dokkomplekt-core/src/core"
violations = []
for src in core_dir.rglob("*.rs"):
    text = src.read_text(encoding="utf-8").lower()
    for term in FORBIDDEN_CORE_TERMS:
        if term in text:
            violations.append(f"{src.relative_to(ROOT)} contains domain term {term!r}")
if violations:
    fail("Domain-neutral core violation:\n" + "\n".join(violations[:40]))

lib_rs_path = ROOT / "crates/dokkomplekt-core/src/lib.rs"
lib_rs = lib_rs_path.read_text(encoding="utf-8")
for forbidden_export in [
    "pub use functional_port::*",
    "pub use universal_behavior_port::*",
    "pub use data_schema_engine::*",
    "pub use domain_plugin_layer::*",
    "pub use template_intelligence_engine::*",
    "pub use workflow_scenario_engine::*",
    "pub use core::*",
    "pub use domains::*",
]:
    if forbidden_export in lib_rs:
        fail(f"Ambiguous glob re-export forbidden: {forbidden_export}")

# Catch unresolved explicit re-exports before Cargo does.
module_root = ROOT / "crates/dokkomplekt-core/src"

def module_file(module_path: str) -> Path | None:
    parts = module_path.split("::")
    if parts[0] == "crate":
        parts = parts[1:]
    if parts and parts[0] == "dokkomplekt_core":
        parts = parts[1:]
    if not parts:
        return None
    direct = module_root.joinpath(*parts).with_suffix(".rs")
    if direct.exists():
        return direct
    mod_rs = module_root.joinpath(*parts, "mod.rs")
    if mod_rs.exists():
        return mod_rs
    return None

def exported_symbols(text: str) -> set[str]:
    return set(re.findall(r"^pub\s+(?:struct|enum|fn|const|static|type|trait)\s+([A-Za-z_][A-Za-z0-9_]*)", text, flags=re.MULTILINE))

reexport_errors: list[str] = []
for match in re.finditer(r"pub\s+use\s+([A-Za-z0-9_:]+)::\s*\{([^}]+)\}\s*;", lib_rs, flags=re.DOTALL):
    module, items_raw = match.groups()
    path = module_file(module)
    if path is None:
        reexport_errors.append(f"re-export module not found: {module}")
        continue
    available = exported_symbols(path.read_text(encoding="utf-8"))
    for item in items_raw.split(','):
        clean = item.strip()
        if not clean or clean.startswith('//'):
            continue
        clean = clean.split(" as ")[0].strip().split("::")[-1].strip()
        if clean and clean not in available:
            reexport_errors.append(f"{lib_rs_path.relative_to(ROOT)} re-exports missing symbol {clean} from {path.relative_to(ROOT)}")
if reexport_errors:
    fail("Unresolved Rust re-export(s):\n" + "\n".join(reexport_errors))

cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
if 'rust-version = "1.97.1"' not in cargo_toml:
    fail("MSRV must be explicit: rust-version = \"1.97\"")

medical_rs = (module_root / "domains/medical.rs").read_text(encoding="utf-8")
universal_pipeline_rs = (module_root / "universal_pipeline.rs").read_text(encoding="utf-8")
if "pub fn canonical_medical_role" not in medical_rs:
    fail("Medical role canonicalization is missing from domains/medical.rs")
for role_slug in ["выписной_эпикриз", "дневники", "акт_для_рвк", "комиссионный_осмотр", "вк_на_мсэ"]:
    if role_slug not in medical_rs:
        fail(f"Medical role slug mapping is missing: {role_slug}")
if "canonical_role_for_domain" not in universal_pipeline_rs or "canonical_medical_role(raw_role)" not in universal_pipeline_rs:
    fail("Universal pipeline does not route domain slugs through domain profile canonicalization")
if "extract_required_fields" in universal_pipeline_rs:
    fail("unused extract_required_fields import found in universal_pipeline.rs")

# Donor migration evidence is release-contract data, not a prose checklist.
_inventory_check = subprocess.run(
    [sys.executable, str(ROOT / "scripts/check_legacy_migration_inventory.py")],
    cwd=ROOT,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
)
if _inventory_check.returncode != 0:
    fail(_inventory_check.stdout.strip() or "legacy migration inventory check failed")

# Diagnosis-specific donor aliases are Medical profile data, never universal matching code.
_professional_records = (ROOT / "crates/dokkomplekt-core/src/professional_records.rs").read_text(encoding="utf-8").split("#[cfg(test)]", 1)[0]
_professional_records = "\n".join(line for line in _professional_records.splitlines() if not line.lstrip().startswith("//"))
for _profile_only_token in ["олигофрен", "психопат", "депресс", "резидуаль"]:
    if _profile_only_token in _professional_records:
        fail(f"Medical diary alias leaked from profile data into Rust matcher: {_profile_only_token}")

# Universal pipeline results must influence behavior, not be discarded into underscore variables.
for rel in [
    "crates/dokkomplekt-core/src/functional_port.rs",
    "crates/dokkomplekt-core/src/source_parser.rs",
    "crates/dokkomplekt-core/src/workflow_engine.rs",
    "src-tauri/src/main.rs",
]:
    text = (ROOT / rel).read_text(encoding="utf-8")
    for discarded in ["let _universal_pipeline", "let _core_domain_workflow", "let _generic_parsed_source"]:
        if discarded in text:
            fail(f"Universal pipeline result is discarded in {rel}: {discarded}")

# Legacy block parity must live in Rust/Tauri, not TS duplicate engines.
if (ROOT / 'resources/icd10_f.tsv').read_text('utf-8', errors='replace').count('\nF') < 300:
    fail('ICD-10 F catalog is too small; expected detailed F-code rows')
main_rs = (ROOT / 'src-tauri/src/main.rs').read_text('utf-8', errors='replace')
for cmd in ['validate_product_access', 'install_background_watcher', 'icd10_suggest']:
    if cmd not in main_rs:
        fail(f'Tauri shell does not expose Rust command: {cmd}')


# Thin UI contract tests must cover DTO envelopes for every registered Rust command.
api_contract_test = (ROOT / "src/lib/api.contract.test.ts").read_text(encoding="utf-8")
for command_name in sorted(backend_commands):
    if command_name not in api_contract_test:
        fail(f"TS↔Rust DTO contract test does not cover command: {command_name}")


# Release freshness must track authored sources, not Tauri-generated schemas.
import importlib.util
_fingerprint_spec = importlib.util.spec_from_file_location("source_fingerprint", ROOT / "scripts/source_fingerprint.py")
if _fingerprint_spec is None or _fingerprint_spec.loader is None:
    fail("Cannot load source_fingerprint.py")
_fingerprint_module = importlib.util.module_from_spec(_fingerprint_spec)
_fingerprint_spec.loader.exec_module(_fingerprint_module)
_fingerprint_paths = [path.relative_to(ROOT).as_posix() for path in _fingerprint_module.iter_files()]
if any(path.startswith("src-tauri/gen/") for path in _fingerprint_paths):
    fail("Generated Tauri schemas must not participate in the release source fingerprint")

# A release archive must be created only after the real Rust gate has written a marker.
assert_release = (ROOT / "scripts/assert_release_ready.py").read_text(encoding="utf-8")
if "CARGO_GATE_ATTESTATION.json" not in assert_release or "Cargo.lock" not in assert_release or "VerifyKey" not in assert_release:
    fail("scripts/assert_release_ready.py must require a signed Cargo gate attestation bound to Cargo.lock")

# A source-only pass is useful in the Python CI job and in environments where
# Rust is intentionally unavailable. It performs every structural, parity,
# version and release-contract check above, but never masquerades as a Rust
# compile result. Packaging still requires the non-optional Cargo gate below.
if "--release" not in sys.argv:
    rust_file_count = sum(1 for _ in ROOT.rglob("*.rs"))
    print(
        "STATIC SOURCE GATE PASSED: "
        f"version={expected_version}; commands={len(backend_commands)}; "
        f"rust_sources={rust_file_count}; cargo_not_executed=true"
    )
    sys.exit(0)

# Non-optional Rust compile gate. This is intentionally fail-closed.
if not (ROOT / "Cargo.lock").exists():
    fail("Cargo.lock is missing. Generate it with `cargo generate-lockfile` on the Rust 1.97+ build host before release.")
if shutil.which("cargo") is None:
    fail("cargo is not available, so Rust compilation cannot be verified. Quality gate fails closed.")
for command in [
    ["cargo", "metadata", "--locked", "--format-version", "1"],
    ["cargo", "fmt", "--all", "--", "--check"],
    ["cargo", "check", "--workspace", "--all-targets", "--locked"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
    ["cargo", "test", "--workspace", "--locked"],
]:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if completed.returncode != 0:
        if completed.stdout:
            print(completed.stdout, file=sys.stderr)
        fail("Rust compile gate failed: " + " ".join(command))

print("STATIC + RUST COMPILE QUALITY GATE OK")
