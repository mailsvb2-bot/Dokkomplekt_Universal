from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path.cwd()


def replace_exact(path: str, old: str, new: str) -> None:
    file_path = ROOT / path
    text = file_path.read_text(encoding="utf-8")
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"guarded replacement source missing: {path}")
    file_path.write_text(text.replace(old, new, 1), encoding="utf-8")


def append_once(path: str, marker: str, addition: str) -> None:
    file_path = ROOT / path
    text = file_path.read_text(encoding="utf-8")
    if marker in text:
        return
    file_path.write_text(text.rstrip() + "\n\n\n" + addition.strip() + "\n", encoding="utf-8")


replace_exact(
    ".env.example",
    """# Desktop build trust anchors (public values only)
DOKKOMPLEKT_LICENSE_PUBKEY_B64=
DOKKOMPLEKT_UPDATE_PUBKEY_B64=
DOKKOMPLEKT_UPDATE_MANIFEST_URL=https://updates.example.com/dokkomplekt/update-manifest.json
DOKKOMPLEKT_COMPONENTS_CATALOG_URL=https://updates.example.com/dokkomplekt/components-catalog.json
DOKKOMPLEKT_REFDATA_URL=https://updates.example.com/dokkomplekt/reference-data/production-calendar-ru.json
DOKKOMPLEKT_REFDATA_PUBKEY_B64=
""",
    """# Desktop build trust anchors (public values only)
DOKKOMPLEKT_GATE_PUBKEY_B64=
DOKKOMPLEKT_LICENSE_PUBKEY_B64=
DOKKOMPLEKT_UPDATE_PUBKEY_B64=
DOKKOMPLEKT_THRESHOLD_PUBKEY_B64=
# Required for a production build. Leave blank until real public HTTPS endpoints
# are configured in repository variables; reserved example domains are rejected.
DOKKOMPLEKT_UPDATE_MANIFEST_URL=
DOKKOMPLEKT_COMPONENTS_CATALOG_URL=
DOKKOMPLEKT_COMPONENTS_BASE_URL=
DOKKOMPLEKT_REFDATA_URL=
DOKKOMPLEKT_REFDATA_PUBKEY_B64=
""",
)
replace_exact(
    ".env.example",
    "DOKKOMPLEKT_QUEUE_MTLS_URL=https://queue.example.internal:9443\n",
    "# Optional. Leave blank unless a real mTLS queue endpoint has been provisioned.\nDOKKOMPLEKT_QUEUE_MTLS_URL=\n",
)
replace_exact(
    ".env.example",
    "DOKKOMPLEKT_LICENSE_PUBLIC_URL=https://licenses.example.com\n",
    "# Required on a deployed production server. A blank value fails closed until a\n# real public HTTPS origin is supplied.\nDOKKOMPLEKT_LICENSE_PUBLIC_URL=\n",
)
replace_exact(
    "components/components-catalog.example.json",
    '"allowed_hosts": ["downloads.example.com"]',
    '"allowed_hosts": ["REPLACE_WITH_PUBLIC_DOWNLOAD_HOST"]',
)
replace_exact(
    "components/components-catalog.example.json",
    "https://downloads.example.com/dokkomplekt/18.3.0/ocr-windows-x86_64.zip",
    "https://REPLACE_WITH_PUBLIC_DOWNLOAD_HOST/dokkomplekt/18.3.0/ocr-windows-x86_64.zip",
)
replace_exact(
    "docs/PRODUCTION_RELEASE_BOOTSTRAP.md",
    "DOKKOMPLEKT_REFDATA_MANIFEST_URL",
    "DOKKOMPLEKT_REFDATA_URL",
)
replace_exact(
    "docs/QUEUE_SERVICE_DEPLOYMENT.md",
    "DOKKOMPLEKT_QUEUE_MTLS_URL=https://queue.example.internal:9443",
    "DOKKOMPLEKT_QUEUE_MTLS_URL=https://queue.<YOUR_REAL_DOMAIN>:9443",
)
append_once(
    "tests/test_component_content_policy.py",
    "def test_example_component_catalog_requires_an_explicit_real_download_host",
    '''
def test_example_component_catalog_requires_an_explicit_real_download_host() -> None:
    example = (ROOT / "components" / "components-catalog.example.json").read_text(
        encoding="utf-8"
    )
    assert "downloads.example.com" not in example
    assert '"allowed_hosts": ["REPLACE_WITH_PUBLIC_DOWNLOAD_HOST"]' in example
    assert "https://REPLACE_WITH_PUBLIC_DOWNLOAD_HOST/" in example
''',
)
append_once(
    "tests/test_release_environment_preflight.py",
    "def test_example_environment_lists_all_public_build_inputs_without_fake_endpoints",
    '''
def test_example_environment_lists_all_public_build_inputs_without_fake_endpoints() -> None:
    env_path = Path(__file__).resolve().parents[1] / ".env.example"
    values = {}
    for raw in env_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value

    for key in public_build_env():
        assert key in values, f"{key} must be documented in .env.example"

    for key in (
        "DOKKOMPLEKT_UPDATE_MANIFEST_URL",
        "DOKKOMPLEKT_COMPONENTS_CATALOG_URL",
        "DOKKOMPLEKT_COMPONENTS_BASE_URL",
        "DOKKOMPLEKT_REFDATA_URL",
        "DOKKOMPLEKT_QUEUE_MTLS_URL",
        "DOKKOMPLEKT_LICENSE_PUBLIC_URL",
    ):
        assert values[key] == "", f"{key} must stay blank until a real endpoint is supplied"

    env = env_path.read_text(encoding="utf-8").lower()
    for forbidden in ("updates.example.com", "licenses.example.com", "queue.example.internal"):
        assert forbidden not in env


def test_release_and_queue_docs_use_current_variable_names_and_explicit_placeholders() -> None:
    root = Path(__file__).resolve().parents[1]
    release = (root / "docs" / "PRODUCTION_RELEASE_BOOTSTRAP.md").read_text(encoding="utf-8")
    queue = (root / "docs" / "QUEUE_SERVICE_DEPLOYMENT.md").read_text(encoding="utf-8")
    assert "DOKKOMPLEKT_REFDATA_MANIFEST_URL" not in release
    assert "DOKKOMPLEKT_REFDATA_URL" in release
    assert "queue.example.internal" not in queue
    assert "https://queue.<YOUR_REAL_DOMAIN>:9443" in queue
''',
)

subprocess.run(
    [
        "git",
        "rm",
        "-f",
        "--ignore-unmatch",
        ".github/patches/config-example-hardening.patch.gz.b64",
        ".github/patches/config-example-hardening.part-00",
        ".github/patches/config-example-hardening.part-01",
        ".github/patches/config-example-hardening.part-02",
        ".github/patches/config-example-hardening.part-03",
        ".github/patches/config-example-hardening.trigger",
    ],
    check=True,
)
