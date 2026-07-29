# Production release bootstrap

A public production release is intentionally blocked until both Windows self-hosted runners are provisioned.

## Runtime/signing runner

Required labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-runtime`.

Configure repository secrets and variables used by `build-installers.yml`: the Authenticode PFX and password, runtime/update signing keys, pinned public keys, real HTTPS component catalog/base URLs, timestamp server, and an absolute runner-owned sidecar manifest path. The manifest must pin every offline component by SHA-256 and signature.

Run locally on the runner before enabling releases:

```powershell
python scripts/release_environment_preflight.py --mode windows-runtime --json-report verification/release/runtime-preflight.json
```

## Hardware runner

Required labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware`. Install licensed Microsoft Word, configure a dedicated test printer and spooler logging, and reserve an absolute persistent path for two-boot watcher evidence.

```powershell
python scripts/release_environment_preflight.py --mode windows-hardware --json-report verification/release/hardware-preflight.json
```

The release workflow remains fail-closed: no EXE is attached to a GitHub release until Authenticode verification, OCR fixture execution, real Word/printing checks, watcher reboot evidence, and Linux bundle checks all pass.
