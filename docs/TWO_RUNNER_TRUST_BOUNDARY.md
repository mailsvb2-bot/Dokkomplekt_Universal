# Hosted-signing / hardware production trust boundary

Dokkomplekt Windows production acceptance uses **two trust domains**: one ephemeral hosted signing/build domain and one private hardware-validation domain. The hardware-validation domain contains **two independent live Windows instances** so platform evidence is not conflated.

The first domain is an ephemeral GitHub-hosted Windows signing/build job. The second domain is a private two-row live matrix:
- Windows 10 x64 runner `dokkomplekt-win10-live`;
- Windows 11 x64 runner `dokkomplekt-win11-live`.

Both hardware rows receive the same exact signed handoff, but execute in separate interactive Windows sessions with separate local state and evidence.

## Hosted runtime/signing domain

Runner: GitHub-hosted `windows-latest`.

Environment: `windows-production-signing`.

The hosted job receives production signing credentials only through protected step-level secrets. It has no persistent runner-owned runtime tree, no `dokkomplekt-runtime` self-hosted label and no `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH`.

Runtime composition is fixed before CI by an immutable signed offline bundle. The hosted job downloads the bundle and its exact signing payload from protected public-HTTPS variables, then verifies:

- release/runtime Ed25519 signature against `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
- a second independent offline approval signature against `DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64`;
- bundle SHA-256 and size;
- SBOM hash;
- exact ZIP file set, safe paths and no symlink entries;
- reviewed complete-portable-tree inventory and provenance/license metadata.

Only after those checks may the hosted job stage executables, run production runtime/OCR/parity gates, build and Authenticode-sign the application and NSIS installer, and create `SIGNED_HANDOFF.json`.

The offline approval private key is not stored in GitHub Actions. Therefore production signing credentials alone cannot silently approve a different runtime composition.

## Hardware evidence runner

Labels:
- `self-hosted`, `Windows`, `X64`, `dokkomplekt-win10-live`;
- `self-hosted`, `Windows`, `X64`, `dokkomplekt-win11-live`.

Environment: `windows-hardware-validation`.

Each live runner receives **no production signing/private-key secrets** and no runner-owned runtime manifest. Each provides an independent interactive Windows desktop, licensed Microsoft Word, WebView2, a dedicated real printer queue, PrintService Operational logging and persistent reboot state. A platform failure does not cancel the other row because the matrix is configured with `fail-fast: false`.

Audited registration entrypoint:

```powershell
# Windows 10 x64 instance
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -WindowsVersion 10 `
  -InstallPrerequisites

# Windows 11 x64 instance
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -WindowsVersion 11 `
  -InstallPrerequisites
```

The hardware runner must remain interactive and starts through an `AtLogOn` scheduled task; Windows service/Session 0 execution is forbidden for Word/printer/visible-GUI evidence.

Before any Word/printer/reboot action it verifies:

- Ed25519 signature of `SIGNED_HANDOFF.json`;
- exact `release_sha` and `request_id` binding;
- exact path, size and SHA-256 for every handoff payload;
- absence of missing, unexpected or reparse/symlink payloads;
- Authenticode signatures of application and NSIS installer;
- signed offline runtime bundle using the trusted runtime public key;
- producer/consumer host identity separation.

The hardware host preflight fails closed if `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` or any known production signing/private-key environment variable is exposed to the hardware process.

## Handoff

The only release payload crossing from hosted signing into physical hardware validation is the GitHub Actions artifact:

`Dokkomplekt-Windows-Signed-Handoff-<release_sha>-<request_id>`

GitHub artifact transport itself is not the trust anchor. The signed manifest and independent hardware-side verification provide the boundary.

## Acceptance invariant

A production hardware verdict is valid only when private `prepare` builds/signs the canonical handoff on GitHub-hosted Windows under `windows-production-signing`, both Windows 10 and Windows 11 prepare rows consume that same handoff, private `verify` re-downloads that exact handoff by `prepare_run_id` instead of rebuilding it, both live rows complete independently under `windows-hardware-validation`, neither hardware row has production signing secret references, and the public publisher later re-verifies and publishes those exact handoff bytes.

The legacy `dokkomplekt-runtime` service scripts remain only for backward compatibility and regression coverage. They are not required by current release/hardware validation.
