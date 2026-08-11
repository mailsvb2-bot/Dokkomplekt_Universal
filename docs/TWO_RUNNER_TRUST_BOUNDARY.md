# Hosted-signing / hardware production trust boundary

Dokkomplekt Windows production acceptance uses **two trust domains but only one physical self-hosted Windows machine**.

The first domain is an ephemeral GitHub-hosted Windows signing/build job. The second is the private physical `dokkomplekt-hardware` runner used only for Word, printer, watcher and reboot evidence.

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

Labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware`.

Environment: `windows-hardware-validation`.

This is the one physical Windows machine. It receives **no production signing/private-key secrets** and no runner-owned runtime manifest. It provides the representative interactive Windows desktop, licensed Microsoft Word, WebView2, a dedicated real printer queue, PrintService Operational logging and persistent reboot state.

Audited registration entrypoint:

```powershell
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
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

A production hardware verdict is valid only when `hardware-evidence` has `needs: signed-runtime-build`, signing/build executes on GitHub-hosted Windows under `windows-production-signing`, hardware evidence executes on the private `dokkomplekt-hardware` runner under `windows-hardware-validation`, the hardware job has no production signing secret references, and both the signed runtime and the signed handoff verify before hardware execution.

The legacy `dokkomplekt-runtime` service scripts remain only for backward compatibility and regression coverage. They are not required by current release/hardware validation.
