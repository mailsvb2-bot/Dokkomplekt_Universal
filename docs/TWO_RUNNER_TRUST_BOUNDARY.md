# Two-runner production trust boundary

Dokkomplekt production Windows acceptance uses two physically separate self-hosted trust domains in the private `mailsvb2-bot/Dokkomplekt_Hardware_Validation` repository.

## Runtime/signing runner

Labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-runtime`.

Environment: `windows-production-signing`.

This runner owns the approved offline runtime tree and runtime lock. It receives production signing credentials, verifies the offline runtime-lock approval before staging, executes the Rust/RustSec release gate, stages and probes the runtime, performs image-only PDF OCR, builds and Authenticode-signs the application and NSIS installer, creates the signed runtime bundle and then creates a signed handoff manifest covering every transferred byte.

Microsoft Word, printer and reboot acceptance are not responsibilities of this runner.

Audited registration entrypoint:

```powershell
.\scripts\register_windows_runtime_runner.ps1 `
  -SidecarManifestPath 'C:\DokkomplektRuntime\locked\windows-x86_64-manifest.json' `
  -InstallPrerequisites
```

## Hardware evidence runner

Labels: `self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware`.

Environment: `windows-hardware-validation`.

This runner receives no production signing/private-key secrets and no runner-owned runtime manifest. It has the representative interactive Windows desktop, licensed Microsoft Word, WebView2, a dedicated real printer queue, PrintService Operational logging and persistent reboot state.

Audited registration entrypoint:

```powershell
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -InstallPrerequisites
```

The hardware entrypoint intentionally has no `SidecarManifestPath` parameter.

Before any Word/printer/reboot action the hardware runner downloads the runtime runner handoff artifact and verifies:

- Ed25519 signature of `SIGNED_HANDOFF.json`;
- exact `release_sha` and `request_id` binding;
- exact path, size and SHA-256 for every handoff payload;
- absence of missing, unexpected or reparse/symlink payloads;
- Authenticode signatures of the application and NSIS installer;
- the signed offline runtime bundle using the trusted runtime public key;
- producer and consumer Windows host fingerprints differ.

The hardware host preflight also fails closed if `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` or any known production signing/private-key environment variable is exposed to the hardware process.

## Registration security

Both role-specific entrypoints prompt for the short-lived GitHub runner registration token with `SecureString`. The shared audited bootstrap `scripts/bootstrap_private_windows_runner.ps1`:

- refuses the public `Dokkomplekt_Universal` repository;
- permits registration only to `mailsvb2-bot/Dokkomplekt_Hardware_Validation`;
- downloads the official Windows x64 Actions runner and verifies its published SHA-256 digest;
- uses distinct roots `C:\actions-runner-runtime` and `C:\actions-runner-hardware`;
- starts each runner through an interactive `AtLogOn` scheduled task, not a Windows service.

## Handoff

The only release payload crossing the trust boundary is the GitHub Actions artifact named:

`Dokkomplekt-Windows-Signed-Handoff-<release_sha>-<request_id>`

GitHub artifact transport is not treated as the trust anchor. The cryptographic manifest and hardware-side independent verification are the trust boundary.

## Acceptance invariant

A production hardware verdict is valid only when the `hardware-evidence` job has `needs: signed-runtime-build` and the two jobs execute on distinct runner labels and distinct protected environments. The hardware job must contain no production signing secret references and no runner-owned runtime manifest.
