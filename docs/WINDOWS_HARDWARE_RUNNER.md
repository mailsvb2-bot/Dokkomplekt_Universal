# Windows production runtime + hardware validation

Dokkomplekt production acceptance uses **two physically separate Windows trust domains**. A single Windows host must never both hold production runtime/signing private material and execute Word/printer/reboot acceptance.

The public repository `mailsvb2-bot/Dokkomplekt_Universal` must **not** be registered as a self-hosted runner target. Both role-specific runners are registered only in the private repository `mailsvb2-bot/Dokkomplekt_Hardware_Validation`. The public workflow runs on GitHub-hosted `ubuntu-latest`, dispatches the private workflow, waits for the correlated private run, and contains no self-hosted job.

## Architecture

### Runtime/signing host

Required labels:

`self-hosted`, `Windows`, `X64`, `dokkomplekt-runtime`

Protected environment:

`windows-production-signing`

This is a dedicated Windows x64 build/signing host. It runs the Actions runner as a Windows service under `NT AUTHORITY\NETWORK SERVICE`. It owns the reviewed offline runtime tree, verifies the offline-approved runtime lock, runs Rust/RustSec, stages and probes the runtime, performs OCR validation, creates the signed runtime bundle/SBOM, builds the application and NSIS installer, applies Authenticode, and creates a signed release handoff.

Only this trust domain may receive private signing material:

- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64`;
- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD`;
- `DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64`;
- `DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64`;
- `DOKKOMPLEKT_GATE_PRIVATE_KEY_B64`.

The runtime-lock approval **private** key remains offline. GitHub and both runners receive no copy of it. The runtime runner receives only the approval public key and the already approved `<manifest>.sig`.

The runtime host does not require Word or a printer.

### Hardware evidence host

Required labels:

`self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware`

Protected environment:

`windows-hardware-validation`

This is a **different Windows machine** running the Actions runner only in an interactive user session through an AtLogOn scheduled task. It requires licensed desktop Microsoft Word, WebView2, Visual Studio C++ Build Tools for the current Rust Word hardware test, a dedicated real printer queue, PrintService Operational logging, and persistent reboot storage.

It must not expose `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` and must contain no runtime/signing private-key variables. `scripts/verify_windows_hardware_evidence_host.ps1` fails closed if those values appear in the hardware process.

Hardware/public-verification variables include:

- `DOKKOMPLEKT_TEST_PRINTER`;
- `DOKKOMPLEKT_TEST_DUPLEX`;
- `DOKKOMPLEKT_TEST_TRAY`;
- `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH`;
- `DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT`;
- `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`.

## Runtime root and service ACL boundary

The production runtime is allowed only under this bounded root on the runtime host:

`C:\ProgramData\DokkomplektRuntime`

Recommended layout:

- `C:\ProgramData\DokkomplektRuntime\components\...` — reviewed portable component trees and license files;
- `C:\ProgramData\DokkomplektRuntime\locked\runtime-inventory.json`;
- `C:\ProgramData\DokkomplektRuntime\locked\windows-x86_64-manifest.json`;
- `C:\ProgramData\DokkomplektRuntime\locked\windows-x86_64-manifest.json.sig`.

The manifest, every `source`, every `license_file`, and the distribution inventory must stay inside this root. Symlinks/junctions/reparse points are rejected. `scripts/grant_windows_runtime_service_access.ps1` verifies that bounded set first and only then grants `NT AUTHORITY\NETWORK SERVICE` recursive ReadAndExecute on this one runtime root. It writes `C:\ProgramData\DokkomplektE2E\RUNTIME_SERVICE_ACL.json`.

The runtime bootstrap refuses to continue without matching ACL evidence, and the runtime job re-checks both the bounded root and ACL while actually running as Network Service. This prevents a service-mode runner from becoming operationally correct only for the administrator who configured it.

## Prepare the production runtime

Place all reviewed component roots used by `runtime-kit.json` under `C:\ProgramData\DokkomplektRuntime`. From the exact approved public source checkout, run:

```powershell
.\scripts\prepare_windows_production_runtime.ps1 `
  -SpecPath 'C:\ProgramData\DokkomplektRuntime\runtime-kit.json' `
  -OutputDir 'C:\ProgramData\DokkomplektRuntime\locked'
```

The production kit requires complete portable trees and license/source provenance for Tesseract, Poppler, LibreOffice, SumatraPDF, 7-Zip, msgconvert, llama.cpp, and the approved GGUF semantic model. The generated manifest must use `schema=1`, `target=windows-x86_64`, `supply_chain_locked=true`, exact SHA-256 entries, and complete distribution-review inventory.

Each rebuild deletes any stale approval signature. The exact new manifest must then be signed with the **offline** runtime-lock approval key using the documented `windows_runtime_lock_approval.py` process. Only the resulting `.sig` and approval public key reach the runtime host workflow.

## Register runtime/signing runner

In the private repository open **Settings → Actions → Runners → New self-hosted runner** and obtain a fresh time-limited registration token. From an elevated PowerShell window on the runtime host, in the approved source checkout:

```powershell
.\scripts\register_windows_runtime_runner.ps1 `
  -RepositoryUrl 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation' `
  -SidecarManifestPath 'C:\ProgramData\DokkomplektRuntime\locked\windows-x86_64-manifest.json' `
  -RuntimeRoot 'C:\ProgramData\DokkomplektRuntime' `
  -InstallPrerequisites
```

The entrypoint prepares the bounded runtime ACL **before asking for the GitHub token**, then prompts for the token as a `SecureString`. The internal bootstrap refuses a missing or mismatched `RUNTIME_SERVICE_ACL.json`, verifies the GitHub runner ZIP against the release SHA-256 digest, fixes the label to `dokkomplekt-runtime`, and installs the runner as a Windows service.

Runtime bootstrap evidence:

`C:\ProgramData\DokkomplektE2E\RUNTIME_RUNNER_BOOTSTRAP.json`

## Register hardware evidence runner

Use a **different physical/virtual Windows instance with a different Windows MachineGuid**. It must not contain the runtime service runner or signing secrets. In the private repository obtain a separate fresh runner registration token. From an elevated **interactive** PowerShell window on the hardware host:

```powershell
.\scripts\register_windows_hardware_evidence_runner.ps1 `
  -RepositoryUrl 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation' `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -InstallPrerequisites
```

The bootstrap checks Word COM, the real printer, PrintService logging, WebView2 and VCTools; refuses any Actions runner Windows service; refuses known signing/private-key variables and the runtime manifest variable; verifies the downloaded runner ZIP digest; registers only label `dokkomplekt-hardware`; and starts it with an interactive AtLogOn scheduled task.

Hardware bootstrap evidence:

`C:\ProgramData\DokkomplektE2E\HARDWARE_RUNNER_BOOTSTRAP.json`

Keep the dedicated hardware user logged in while jobs execute. Word COM/visible-GUI acceptance must never run in Session 0.

## Cryptographically signed handoff and physical-host proof

The only release payload crossing from runtime/signing to hardware is:

`Dokkomplekt-Windows-Signed-Handoff-<release_sha>-<request_id>`

The runtime job creates `SIGNED_HANDOFF.json` plus `SIGNED_HANDOFF.json.sig`. Schema v2 binds:

- exact public `release_sha`;
- exact dispatch `request_id`;
- runtime producer runner name;
- SHA-256 fingerprint derived from the runtime host Windows MachineGuid;
- every relative payload path, byte size, and SHA-256.

The hardware job computes its own MachineGuid-derived SHA-256 and passes it as the consumer fingerprint. Verification fails if producer and consumer fingerprints are equal. Therefore two labels configured on the **same Windows machine cannot satisfy production hardware acceptance**.

Before Word/printer/reboot execution the hardware host independently verifies:

1. `SIGNED_HANDOFF.json.sig` against the trusted runtime public key;
2. release SHA and request correlation;
3. producer/consumer host separation;
4. exact file set, sizes, and SHA-256 values;
5. Authenticode on application and NSIS installer;
6. the offline runtime bundle signature against the trusted runtime public key.

GitHub artifact transport is therefore not itself trusted as the security boundary.

## Private environments

### `windows-production-signing`

Runtime-only variables include:

- `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` — `C:\ProgramData\DokkomplektRuntime\locked\windows-x86_64-manifest.json`;
- `DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64`;
- `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
- `DOKKOMPLEKT_GATE_PUBKEY_B64`;
- `DOKKOMPLEKT_LICENSE_PUBKEY_B64`;
- `DOKKOMPLEKT_UPDATE_PUBKEY_B64`;
- `DOKKOMPLEKT_THRESHOLD_PUBKEY_B64`;
- `DOKKOMPLEKT_REFDATA_PUBKEY_B64`;
- production update/reference/component URLs;
- `DOKKOMPLEKT_TIMESTAMP_SERVER`;
- `DOKKOMPLEKT_SIGNING_SCRIPT_SHA256`.

Runtime-only secrets are the five private values listed under the runtime/signing domain above.

### `windows-hardware-validation`

Contains only hardware variables and public verification material. It must contain no signing secrets and no `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH`.

### Public `windows-hardware-dispatch`

The public workflow pins:

- `DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY=mailsvb2-bot/Dokkomplekt_Hardware_Validation`;
- `DOKKOMPLEKT_HARDWARE_VALIDATION_WORKFLOW=windows-hardware-e2e.yml`.

Required secret:

- `DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN` — narrowly scoped to the private validation repository with Actions access only.

## Private workflow order

The audited scaffold is `ops/private-hardware-validation/windows-hardware-e2e.yml`. The private repository workflow must match the audited scaffold.

The order is fixed:

1. `signed-runtime-build` → `dokkomplekt-runtime` / `windows-production-signing`;
2. signed artifact handoff;
3. `hardware-evidence` with `needs: signed-runtime-build` → `dokkomplekt-hardware` / `windows-hardware-validation`.

Do not place both labels on one host. The signed MachineGuid fingerprints enforce this rule at runtime, not only by documentation.

## Real reboot — two phases

### `prepare`

Run public **Windows Hardware E2E** with the approved exact `release_sha` and `reboot_phase=prepare`. The runtime host creates the signed handoff. The hardware host verifies it and prepares persistent state under `C:\ProgramData\DokkomplektE2E`.

Perform a genuine Windows restart and log back into the same dedicated hardware runner account.

### `verify`

Run public **Windows Hardware E2E** again for the same release SHA with `reboot_phase=verify`. The hardware machine verifies the post-reboot evidence, executes real Word COM `PrintOut`, requires PrintService Event 307, verifies watcher exactly-once behavior, GUI/no-console behavior, and clean uninstall.

After exact-SHA reboot evidence exists, run **FULL DOKKOMPLEKT AUTOPILOT** with `scope=production-hardware`.

## Production PASS

Issue #5 remains open until real evidence exists for all of the following:

- exact protected public source SHA;
- approved supply-chain-locked offline runtime on the runtime host;
- bounded Network Service runtime ACL evidence;
- signing/runtime host physically distinct from hardware host;
- signed Rust/RustSec gate;
- signed runtime bundle and SBOM;
- Authenticode-valid application and NSIS installer;
- signed `SIGNED_HANDOFF.json` with exact file hashes and producer fingerprint;
- hardware-side handoff/runtime/Authenticode verification;
- licensed Word COM `PrintOut`;
- PrintService Event 307 on the real printer;
- visible installed GUI without unexpected console/PowerShell windows;
- genuine Windows reboot;
- watcher startup after reboot and exactly-once output;
- silent install and clean uninstall;
- immutable evidence hashes bound to the approved release.

Hosted CI or software-only Autopilot cannot substitute for those external observations.

## Legacy single-runner scripts

`scripts/register_windows_hardware_runner.ps1`, `scripts/bootstrap_windows_hardware_runner.ps1`, and `scripts/verify_windows_hardware_runner.ps1` remain temporarily for compatibility with the previous design. They are **not** the production trust boundary. Production validation uses the role-specific runtime/signing and hardware-evidence runners above.
