# Production release bootstrap

A public production release is intentionally blocked until both Windows self-hosted runners are provisioned.

## Runtime/signing runner

Required labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-runtime`.

Configure repository secrets and variables used by `build-installers.yml`:

- compile-time public trust anchors: `DOKKOMPLEKT_GATE_PUBKEY_B64`, `DOKKOMPLEKT_LICENSE_PUBKEY_B64`, `DOKKOMPLEKT_UPDATE_PUBKEY_B64`, `DOKKOMPLEKT_THRESHOLD_PUBKEY_B64`, `DOKKOMPLEKT_REFDATA_PUBKEY_B64` and `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
- real HTTPS endpoints: `DOKKOMPLEKT_UPDATE_MANIFEST_URL`, `DOKKOMPLEKT_REFDATA_URL`, `DOKKOMPLEKT_COMPONENTS_CATALOG_URL` and `DOKKOMPLEKT_COMPONENTS_BASE_URL`;
- private signing material: the Authenticode PFX/password, runtime-signing key, update-signing key and gate-signing key;
- timestamp server and an absolute runner-owned sidecar manifest path.

The manifest must pin every offline component by SHA-256 and signature. Public trust anchors and URLs are present during compilation. Private keys are scoped only to the exact signing/preflight step that needs them; they are not exposed to checkout, dependency installation, tests or ordinary build steps. Every third-party GitHub Action is pinned by a full commit SHA.

Run the public production-build preflight before any platform build:

```powershell
python scripts/release_environment_preflight.py --mode production-build --json-report verification/release/production-build-preflight.json
```

Run locally on the signing runner before enabling Windows releases:

```powershell
python scripts/release_environment_preflight.py --mode windows-runtime --json-report verification/release/runtime-preflight.json
```

## Hardware runner

Required labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware`. Install licensed Microsoft Word, configure a dedicated test printer and spooler logging, and reserve an absolute persistent path for two-boot watcher evidence.

```powershell
python scripts/release_environment_preflight.py --mode windows-hardware --json-report verification/release/hardware-preflight.json
```

The release workflow remains fail-closed: no EXE is attached to a GitHub release until Authenticode verification, OCR fixture execution, real Word/printing checks, watcher reboot evidence, and Linux bundle checks all pass.
