# Windows production runtime + hardware validation

Dokkomplekt production acceptance uses **two physically separate Windows trust domains**. A single self-hosted machine must never both hold production signing/runtime private material and execute Word/printer/reboot acceptance.

The public source repository `mailsvb2-bot/Dokkomplekt_Universal` must **not** be registered as a self-hosted runner target. Both Windows runners belong only to the private repository `mailsvb2-bot/Dokkomplekt_Hardware_Validation`; the public workflow only dispatches and waits from GitHub-hosted `ubuntu-latest`.

## Trust-domain architecture

### 1. Runtime/signing domain

Runner labels:

`self-hosted`, `Windows`, `X64`, `dokkomplekt-runtime`

Protected environment:

`windows-production-signing`

This runner owns the approved production runtime tree and the exact runner-owned manifest referenced by `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH`. It verifies the offline Ed25519 approval of that lock **before** staging, executes the Rust/RustSec release gate, stages/probes/OCR-tests the offline runtime, creates the signed runtime bundle, builds the application and NSIS installer, applies Authenticode signatures and creates build evidence.

Only this trust domain may receive:

- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64`;
- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD`;
- `DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64`;
- `DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64`;
- `DOKKOMPLEKT_GATE_PRIVATE_KEY_B64`.

The runtime-lock approval **private** key remains offline and is not stored in GitHub or on either runner. The runtime runner receives only its approval public key and the already approved `<manifest>.sig`.

The runtime runner does **not** need Microsoft Word or a printer. Production jobs execute as a **Windows service in Session 0 under Network Service SID `S-1-5-20`**, not in the interactive desktop session used for hardware acceptance.

### Fixed runtime filesystem boundary

Production runtime storage is fixed to:

`C:\ProgramData\DokkomplektRuntime`

The production manifest, its `.sig`, every manifest `source`, every `license_file`, and the reviewed distribution inventory must remain under this direct non-reparse root. A production runtime manifest outside this root is rejected.

Before runtime runner registration, `scripts/grant_windows_runtime_service_access.ps1` verifies that every referenced file is bounded to this root and rejects symlinks/junctions/reparse points. Only after that proof does it grant Windows Network Service SID `S-1-5-20` recursive `ReadAndExecute` on this one root. Evidence is written to:

`C:\ProgramData\DokkomplektE2E\RUNTIME_SERVICE_ACL.json`

The shared runner bootstrap refuses to create the runtime service without matching ACL evidence. During the real private runtime job, `scripts/release_environment_preflight.py --mode windows-runtime` independently proves:

- current Windows SID is `S-1-5-20`;
- current process session is Session 0;
- the manifest and all runtime source/license/inventory files remain under the fixed root;
- those files are readable by the actual service process;
- ACL evidence binds the same root, manifest and SID.

Audited runtime registration entrypoint:

```powershell
.\scripts\register_windows_runtime_runner.ps1 `
  -SidecarManifestPath 'C:\ProgramData\DokkomplektRuntime\locked\windows-x86_64-manifest.json' `
  -RuntimeRoot 'C:\ProgramData\DokkomplektRuntime' `
  -InstallPrerequisites
```

`RuntimeRoot` is a fail-closed policy assertion; another production path is rejected.

### 2. Hardware evidence domain

Runner labels:

`self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware`

Protected environment:

`windows-hardware-validation`

This machine is the representative **interactive** Windows host. It requires licensed desktop Microsoft Word, WebView2, Visual Studio C++ tools used by the current Rust Word hardware test, a dedicated real printer queue, PrintService Operational logging and persistent reboot storage.

The hardware environment must contain **no signing/private-key secrets** and must not expose `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH`. `scripts/verify_windows_hardware_evidence_host.ps1` fails closed if a runtime manifest variable or any known signing secret is exposed to the hardware process.

Hardware-side variables are limited to hardware/public verification data such as:

- `DOKKOMPLEKT_TEST_PRINTER`;
- `DOKKOMPLEKT_TEST_DUPLEX`;
- `DOKKOMPLEKT_TEST_TRAY`;
- `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH`;
- `DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT`;
- `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`.

Audited hardware registration entrypoint:

```powershell
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -InstallPrerequisites
```

The hardware runner remains an interactive `AtLogOn` scheduled task. Windows service/Session 0 execution is forbidden for Word COM, printer and visible-GUI evidence.

## Cryptographically signed handoff

The only release payload that crosses from runtime/signing to hardware is the Actions artifact:

`Dokkomplekt-Windows-Signed-Handoff-<release_sha>-<request_id>`

Before upload the runtime runner builds `SIGNED_HANDOFF.json`, containing the exact relative path, byte size and SHA-256 of every application, installer, runtime, Rust gate and build-evidence payload. `SIGNED_HANDOFF.json.sig` is an Ed25519 signature created in the runtime signing domain.

The hardware job downloads that artifact and, **before any Word/printer/reboot execution**:

1. verifies `SIGNED_HANDOFF.json.sig` against the trusted runtime public key;
2. binds it to the exact public `release_sha` and dispatch `request_id`;
3. rejects missing, extra, symlink/reparse, size-changed or SHA-256-changed files;
4. restores and verifies signed gate/source-identity evidence;
5. independently verifies Authenticode on the application and NSIS installer;
6. independently verifies the signed offline runtime bundle against the trusted runtime public key;
7. rejects a producer/consumer host fingerprint match, so both roles cannot obtain a production verdict on the same Windows host.

GitHub artifact transport is therefore not itself treated as the trust boundary. The cryptographic manifest and independent hardware-side verification are the boundary.

## Public dispatcher boundary

The public workflow `.github/workflows/windows-hardware-e2e.yml` runs only on GitHub-hosted `ubuntu-latest`. Its protected environment is:

`windows-hardware-dispatch`

It pins:

- `DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY=mailsvb2-bot/Dokkomplekt_Hardware_Validation`;
- `DOKKOMPLEKT_HARDWARE_VALIDATION_WORKFLOW=windows-hardware-e2e.yml`.

Required secret:

- `DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN` — narrowly scoped to the private validation repository with Actions access only.

The dispatcher requires the exact protected public main SHA, proves the target repository is private, supplies a correlation UUID, waits for the exact private run and fails unless that run succeeds.

## Runtime lock preparation

The runtime/signing host, not the hardware host, owns the production runtime tree. Place reviewed portable component roots, license notices and the runtime-kit specification under `C:\ProgramData\DokkomplektRuntime`, then run `scripts/prepare_windows_production_runtime.ps1` from the exact approved public source checkout.

The resulting approved manifest is expected at:

`C:\ProgramData\DokkomplektRuntime\locked\windows-x86_64-manifest.json`

Production runtime requirements include complete portable trees and license/source provenance for Tesseract, Poppler, LibreOffice, SumatraPDF, 7-Zip, msgconvert, llama.cpp and the approved GGUF semantic model. The manifest must use `schema=1`, `target=windows-x86_64`, `supply_chain_locked=true`, exact SHA-256 entries and a complete reviewed distribution inventory.

After each rebuild, any old approval `.sig` is invalidated. The new exact manifest must be approved again with the offline runtime-lock approval private key before production staging can proceed.

## Private workflow

The audited scaffold is:

`ops/private-hardware-validation/windows-hardware-e2e.yml`

The private repository `.github/workflows/windows-hardware-e2e.yml` must match the audited scaffold for an approved release.

It contains two jobs in strict order:

1. `signed-runtime-build` on `dokkomplekt-runtime` / `windows-production-signing`;
2. `hardware-evidence`, with `needs: signed-runtime-build`, on `dokkomplekt-hardware` / `windows-hardware-validation`.

Do not place both runner labels on one host. Do not copy signing secrets into `windows-hardware-validation`. Do not copy the runner-owned sidecar tree or manifest onto the hardware machine as part of the validation design.

## Real reboot — two phases

Real reboot validation remains explicitly two-phase for the same public release SHA.

### `prepare`

Run public **Windows Hardware E2E** with the approved `release_sha` and `reboot_phase=prepare`. The service-mode runtime runner creates the signed handoff, then the interactive hardware runner verifies it and installs/prepares the watcher under persistent `C:\ProgramData\DokkomplektE2E` state. The prepared record includes the SHA-256 of `SIGNED_HANDOFF.json`.

Perform an actual Windows restart and log back into the dedicated hardware runner account.

### `verify`

Run the public workflow again for the same SHA with `reboot_phase=verify`. A fresh signed handoff for that source/request is independently verified, then the hardware runner verifies the post-reboot evidence and executes the full physical contour.

`FULL DOKKOMPLEKT AUTOPILOT` with `scope=production-hardware` is allowed to pass only after the required exact-SHA hardware evidence exists.

## Hardware acceptance

Production hardware evidence requires all of the following:

- exact protected public source SHA;
- approved and supply-chain-locked offline runtime on the service-mode runtime runner;
- bounded Network Service runtime ACL evidence;
- signed Rust/RustSec release gate;
- signed runtime bundle and SBOM;
- Authenticode-valid application and NSIS installer;
- signed `SIGNED_HANDOFF.json` with exact file hashes;
- physically distinct runtime/signing and hardware host fingerprints;
- hardware-side independent handoff/runtime/Authenticode verification;
- licensed Word COM `PrintOut`;
- PrintService Event 307 on the dedicated real printer queue;
- visible installed GUI without unexpected console/PowerShell windows;
- genuine Windows reboot;
- watcher startup after reboot and exactly-once output;
- silent install and clean uninstall;
- immutable evidence hashes bound to the approved release.

Issue #5 remains open until those **real external Windows observations** exist. Hosted CI, mocked Tauri IPC, unsigned preview installers or a green software-only Autopilot cannot substitute for this evidence.

## Legacy single-runner scripts

`scripts/register_windows_hardware_runner.ps1`, `scripts/bootstrap_windows_hardware_runner.ps1` and `scripts/verify_windows_hardware_runner.ps1` remain temporarily for compatibility with the previous single-runner setup. They are no longer the production trust-boundary design. The private validation repository must use role-specific runtime and hardware registration so the production signing material and the Word/printer/reboot host remain separate.
