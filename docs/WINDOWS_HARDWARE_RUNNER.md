# Windows production signing + hardware validation

Dokkomplekt production acceptance uses a **two-row independent live Windows matrix**. Build/signing still runs once on an ephemeral **GitHub-hosted Windows** runner inside the protected `windows-production-signing` environment. The same exact signed handoff is then consumed independently by:

- **Windows 10 x64** interactive runner: `dokkomplekt-win10-live`;
- **Windows 11 x64** interactive runner: `dokkomplekt-win11-live`.

The two live runners must be separate Windows instances (physical machines or isolated VMs) with separate interactive user sessions, runner identities, local state and evidence. One platform failing must not cancel the other (`fail-fast: false`), and release evidence is complete only when both rows pass.

The public source repository `mailsvb2-bot/Dokkomplekt_Universal` must not be registered as a self-hosted runner target. The sole physical runner belongs only to the private repository `mailsvb2-bot/Dokkomplekt_Hardware_Validation`; the public `windows-hardware-dispatch` workflow dispatches and waits from GitHub-hosted Linux.

## Trust-domain architecture

### 1. Ephemeral build/signing domain

Runner:

`windows-latest` (GitHub-hosted)

Protected environment:

`windows-production-signing`

This job has no persistent runner-owned runtime tree and must not receive `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH`. Instead it downloads one immutable pre-reviewed production runtime from public HTTPS locations:

- `DOKKOMPLEKT_RUNTIME_BUNDLE_URL`;
- `DOKKOMPLEKT_RUNTIME_BUNDLE_PAYLOAD_URL`;
- `DOKKOMPLEKT_RUNTIME_BUNDLE_SIGNATURE_URL`;
- `DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL`.

The runtime is accepted only if the exact signing payload verifies under **two independent Ed25519 trust roots**:

1. the release/runtime signature against `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
2. the offline composition approval against `DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64`.

The offline approval private key is never stored in GitHub. `scripts/windows_runtime_bundle_approval.py` signs an exact reviewed runtime payload outside CI. The bundle itself carries the reviewed complete-portable-tree inventory and provenance/license metadata; `scripts/stage_signed_runtime_bundle.py` reconstructs the staged runtime only after both signatures, bundle digest, SBOM, inventory and exact ZIP file set verify.

The hosted job then executes the existing production gates for the bounded `core` profile: document-runtime completeness, runtime/application parity, sidecar Authenticode, OCR fixture, Rust/RustSec gate, Tauri build, application Authenticode, offline NSIS build/signing and installer contract smoke. The semantic GGUF/runtime is governed separately and is not embedded in the stock core installer.

### Windows Authenticode key boundary

Production Windows signing does **not** use an exportable PFX. The signing backend is explicit:

- `DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND=certificate-store`;
- `DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT` identifies the exact code-signing certificate in `Cert:\CurrentUser\My`;
- `DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER` pins the exact approved HSM/KSP/CSP provider;
- `DOKKOMPLEKT_TIMESTAMP_SERVER` selects the reviewed timestamp service.

`scripts/sign_windows_release.ps1` proves that the selected certificate has an RSA private key, that its provider exactly matches the approved provider, rejects known Microsoft software-only providers, rejects an exportable production key, signs the requested artifacts and then verifies both Authenticode validity and the signer thumbprint.

The legacy `pfx` backend exists only for non-production compatibility/testing. It is fail-closed when `DOKKOMPLEKT_RELEASE_MODE=production` and imported PFX keys are never marked `-Exportable`.

The private key for Authenticode therefore remains **non-exportable** in the configured hardware/HSM-backed provider. The CI workflow must provision/authenticate that provider before signing; it must not copy the private key into GitHub Actions variables or secrets. The repository currently contains no vendor-specific HSM/KSP provisioning action, so production remains intentionally blocked until a reviewed provider integration is configured. `scripts/sign_windows_release.ps1 -VerifyCertificateOnly` fails early unless the exact certificate is already present, provider-matched and non-exportable.

Other private values used only in the protected hosted domain include:

- `DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64` for the signed handoff;
- `DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64` where component catalog publishing is required;
- `DOKKOMPLEKT_GATE_PRIVATE_KEY_B64`.

No persistent Windows build/signing computer is required.

## Preparing an approved runtime source

A reviewed runtime tree is still a release artifact, not something CI may invent. Build it from reviewed portable component trees with the existing runtime-kit tooling, create the deterministic offline bundle, sign its `*.signing.json` with the runtime release key, then independently approve those exact payload bytes with an offline approval key:

```powershell
python scripts/create_offline_runtime_bundle.py --target windows-x86_64 --profile core --require-supply-chain --output-dir release-runtime --signing-key <runtime-private.pem> --trusted-public-key <runtime-public.pem> --require-signature
python scripts/windows_runtime_bundle_approval.py sign release-runtime\Dokkomplekt-offline-runtime-windows-x86_64.zip.signing.json --private-key <offline-approval-private.pem> --reviewer <reviewer>
```

Publish the ZIP, signing JSON, runtime signature and offline-approval signature at immutable HTTPS URLs and configure the four variables above. Changing any byte requires a new runtime signature and a new offline approval.

## 2. Hardware evidence domain

Runner labels:

- `self-hosted`, `Windows`, `X64`, `dokkomplekt-win10-live`;
- `self-hosted`, `Windows`, `X64`, `dokkomplekt-win11-live`.

Protected environment:

`windows-hardware-validation`

Each matrix row is a representative interactive user host with licensed desktop Microsoft Word, WebView2, Visual Studio C++ tools used by the current Rust/Word hardware test, a dedicated real printer queue, PrintService Operational logging and persistent reboot storage. Windows 10 and Windows 11 evidence are independent and are uploaded under platform-specific artifact names.

The hardware environment must contain no signing/private-key secrets and must not expose a sidecar manifest. `scripts/verify_windows_hardware_evidence_host.ps1` fails closed if signing material or `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` is exposed.

Hardware-side variables are limited to public verification/hardware data:

- `DOKKOMPLEKT_TEST_PRINTER`;
- `DOKKOMPLEKT_TEST_DUPLEX`;
- `DOKKOMPLEKT_TEST_TRAY`;
- `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH`;
- `DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT`;
- `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`.

Audited registration entrypoint:

```powershell
# Run on the dedicated Windows 10 x64 instance
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -WindowsVersion 10 `
  -InstallPrerequisites

# Run separately on the dedicated Windows 11 x64 instance
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -WindowsVersion 11 `
  -InstallPrerequisites
```

The bootstrap verifies the actual OS build before registration and assigns only the matching platform label. Windows 10 is build < 22000; Windows 11 is build >= 22000.

The hardware runner remains an interactive `AtLogOn` scheduled task. Windows service/Session 0 execution is forbidden for Word COM, printer and visible-GUI evidence.

## Cryptographically signed handoff

The hosted signing job creates:

`Dokkomplekt-Windows-Signed-Handoff-<release_sha>-<request_id>`

`SIGNED_HANDOFF.json` binds the application, installer, approved runtime and build evidence by path, size and SHA-256 and is signed with the runtime signing key. Each Windows 10/11 hardware runner independently verifies that same handoff, application/installer Authenticode and runtime signature before any Word/printer/reboot execution. Hardware never receives signing private keys.

Because the producer is an ephemeral GitHub-hosted Windows runner and the consumer is the self-hosted physical machine, the trust domains and host identities remain separate without requiring a second user-owned PC.

## Public dispatcher boundary

The public workflow `.github/workflows/windows-hardware-e2e.yml` runs only on GitHub-hosted `ubuntu-latest`. It pins `DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY=mailsvb2-bot/Dokkomplekt_Hardware_Validation` and uses `DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN` from `windows-hardware-dispatch` to dispatch the exact protected public SHA.

## Private workflow

The audited scaffold is `ops/private-hardware-validation/windows-hardware-e2e.yml`; the private repository `.github/workflows/windows-hardware-e2e.yml` must match it.

It contains one signed build job followed by a two-row live matrix:

1. `signed-runtime-build` on ephemeral GitHub-hosted Windows / `windows-production-signing`;
2. `hardware-evidence (windows-10)` on `dokkomplekt-win10-live`;
3. `hardware-evidence (windows-11)` on `dokkomplekt-win11-live`.

Both hardware rows consume the same exact signed handoff and run independently with `fail-fast: false`.

## Real reboot — two phases

### `prepare`

Run public **Windows Hardware E2E** with the approved `release_sha` and `reboot_phase=prepare`. Hosted Windows creates the signed handoff; the physical runner verifies it and prepares persistent watcher/reboot state under `C:\ProgramData\DokkomplektE2E`.

Perform a real Windows restart on **both** live runners and log back into each dedicated runner account.

### `verify`

You may run the public workflow again for the same SHA with `reboot_phase=verify`; alternatively, the production `Build Signed Offline Installers` hardware gate automatically resolves the newest successful private `prepare` for that exact SHA and dispatches the matching private `verify` phase with the same request UUID **and prepare run ID**. Verify downloads the signed handoff from that prepare run instead of rebuilding it, so reboot recovery and the final hardware contour exercise the exact same Windows bytes.

The production release workflow itself always runs this bridge on GitHub-hosted Linux under `windows-hardware-dispatch`; it never schedules a self-hosted runner or a second Windows signer in the public repository. After private verify succeeds, the bridge downloads the exact prepare handoff and verify evidence, re-verifies the signed inventory, and those exact files—not a rebuilt equivalent—become the published Windows release. If the required prepare/reboot has not happened yet, the release fails fast rather than remaining queued.

`FULL DOKKOMPLEKT AUTOPILOT` with `scope=production-hardware` may pass only after exact-SHA hardware evidence exists.

## Hardware acceptance

Production hardware evidence still requires exact protected source SHA, independently approved supply-chain-locked runtime, signed Rust/RustSec gate, Authenticode-valid application and NSIS installer, signed handoff, hardware-side independent verification, licensed Word COM `PrintOut`, PrintService Event 307 on a real printer queue, visible installed GUI without unexpected console windows, genuine reboot, watcher recovery/exactly-once output, silent install/clean uninstall and immutable evidence hashes.

Issue #5 remains open until those real external Windows observations exist. Hosted CI, mocked Tauri IPC, unsigned preview installers or a green software-only Autopilot cannot substitute for physical Word/printer/reboot evidence.

## Legacy runtime-runner scripts

`scripts/register_windows_runtime_runner.ps1`, `scripts/grant_windows_runtime_service_access.ps1` and the old runner-owned-manifest service boundary remain for historical compatibility and forensic regression coverage. They are **not required by the current production release or hardware-validation architecture**.
