# Windows Hardware Runner

This is the production hardware acceptance host for Dokkomplekt Universal. It is intentionally different from the ordinary GitHub-hosted Windows packaging runner: it must prove real Microsoft Word automation, a real printer/spooler completion event, visible GUI behavior, signed binaries, a complete offline runtime, and watcher behavior across a real Windows reboot.

## Required host

Use a dedicated Windows 11 Pro x64 machine or dedicated Windows 11 Pro x64 VM with an interactive desktop session and exclusive use for Dokkomplekt release evidence.

The runner account must be a dedicated Windows user with administrator rights for the test host. Keep this user logged in while hardware jobs execute. Do **not** install the GitHub Actions runner as a Windows service: Word COM and the visible-GUI evidence require an interactive user session, while Windows services execute in a noninteractive service session.

Required host components:

- licensed desktop Microsoft Word, activated for the dedicated runner user;
- Microsoft Edge WebView2 Runtime;
- a dedicated real printer queue (not Microsoft Print to PDF/XPS/OneNote/Fax);
- Git for Windows;
- PowerShell 7;
- Visual Studio 2022 Build Tools with `Microsoft.VisualStudio.Workload.VCTools`;
- outbound HTTPS access to GitHub, Rust and npm infrastructure, plus the production Dokkomplekt component/reference/update endpoints;
- runner-owned production sidecar tree and manifest;
- enough free disk space for Rust/Tauri builds, the semantic model and the offline runtime bundle.

The repository hardware workflow requires the runner labels:

`self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware-e2e`

## Runner-owned sidecar manifest

Do not use the source placeholder `src-tauri/resources/tools/windows-x86_64/sidecar-status.json` as production input. Production uses an absolute path owned by the runner, for example:

`C:\DokkomplektRuntime\windows-x86_64-manifest.json`

The manifest must satisfy `scripts/release_environment_preflight.py`:

- `schema: 1`;
- `target: windows-x86_64`;
- `supply_chain_locked: true`;
- SHA-256 for every file and license notice;
- version, source URL and license metadata for every entry;
- complete portable-tree inventory and distribution review;
- Tesseract including required language data, Poppler, LibreOffice, SumatraPDF, 7-Zip, llama.cpp server and an approved GGUF semantic model; include any additional runtime entries required by the current product contour.

The runner manifest points only to local, reviewed files. `scripts/prepare_sidecars.py` deliberately does not download production binaries.

## Register the GitHub runner

In GitHub open repository **Settings → Actions → Runners → New self-hosted runner** and obtain a fresh repository registration token. The token is short-lived; use it immediately and never store it in the repository or in a script.

Open an **elevated interactive PowerShell** window while logged in as the dedicated runner user. From a checkout of this repository run:

```powershell
.\scripts\register_windows_hardware_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -SidecarManifestPath 'C:\DokkomplektRuntime\windows-x86_64-manifest.json' `
  -InstallPrerequisites
```

The registration entrypoint prompts for the GitHub token as `SecureString`; the plaintext token is not typed into the PowerShell command history. The bootstrap verifies the downloaded GitHub runner ZIP against the SHA-256 digest published in the GitHub release asset metadata, configures label `dokkomplekt-hardware-e2e`, and creates an **interactive AtLogOn scheduled task** instead of a Windows service.

The bootstrap writes host evidence to:

`C:\ProgramData\DokkomplektE2E\HARDWARE_RUNNER_BOOTSTRAP.json`

After registration, verify the repository Settings page shows the runner online and carrying the required custom label.

## `windows-production-signing` environment

Create or use the protected GitHub environment named `windows-production-signing`. Restrict deployment access to protected `main` according to the repository release policy. Do not expose signing secrets to pull-request jobs.

Environment variables required by the current workflow:

- `DOKKOMPLEKT_TEST_PRINTER` — exact dedicated printer queue name;
- `DOKKOMPLEKT_TEST_DUPLEX` — test duplex setting used by the print contract;
- `DOKKOMPLEKT_TEST_TRAY` — test tray setting used by the print contract;
- `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH` — use a persistent absolute path, recommended `C:\ProgramData\DokkomplektE2E\WINDOWS_REBOOT_E2E_RAW.json`;
- `DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT` — absolute path to a stable real source fixture on the runner, outside temporary runner work directories;
- `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH` — absolute runner-owned production manifest path;
- `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
- `DOKKOMPLEKT_GATE_PUBKEY_B64`;
- `DOKKOMPLEKT_LICENSE_PUBKEY_B64`;
- `DOKKOMPLEKT_UPDATE_PUBKEY_B64`;
- `DOKKOMPLEKT_THRESHOLD_PUBKEY_B64`;
- `DOKKOMPLEKT_REFDATA_PUBKEY_B64`;
- `DOKKOMPLEKT_UPDATE_MANIFEST_URL`;
- `DOKKOMPLEKT_REFDATA_URL`;
- `DOKKOMPLEKT_COMPONENTS_CATALOG_URL`;
- `DOKKOMPLEKT_COMPONENTS_BASE_URL`;
- `DOKKOMPLEKT_TIMESTAMP_SERVER`;
- `DOKKOMPLEKT_SIGNING_SCRIPT_SHA256` — SHA-256 of the audited `scripts/sign_windows_release.ps1` from the approved release tree.

The update/reference/component URLs must be real public HTTPS production endpoints. Placeholder `.invalid` URLs are not acceptable.

Environment secrets required by the workflow:

- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64`;
- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD`;
- `DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64`;
- `DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64`;
- `DOKKOMPLEKT_GATE_PRIVATE_KEY_B64`.

Use a real Authenticode code-signing certificate whose private key is controlled as a production secret. Never commit PFX/private keys or sidecar signing keys to Git.

## Host preflight

Every hardware workflow now runs `scripts/verify_windows_hardware_runner.ps1` before installing toolchains or touching signing secrets. It fails closed if the job is in Session 0/service mode, the interactive runner task is absent, Word COM fails, the printer is virtual/unavailable, PrintService logging is unavailable, Build Tools/WebView2/OpenSSL are missing, or the runner-owned sidecar manifest is not supply-chain locked.

For an explicit local preflight after the runner is registered:

```powershell
.\scripts\verify_windows_hardware_runner.ps1 `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -SidecarManifestPath 'C:\DokkomplektRuntime\windows-x86_64-manifest.json' `
  -RebootEvidencePath 'C:\ProgramData\DokkomplektE2E\WINDOWS_REBOOT_E2E_RAW.json' `
  -OutputPath 'C:\ProgramData\DokkomplektE2E\HARDWARE_RUNNER_HOST.json'
```

## Real reboot: two phases

A real reboot cannot happen in the middle of one ordinary GitHub Actions job and then magically resume the same process. The workflow therefore exposes two explicit phases for the same exact protected `main` SHA.

### Phase 1 — `prepare`

Run **Actions → Windows Hardware E2E → Run workflow** on `main` with:

- `release_sha` = exact 40-character commit SHA to approve;
- `reboot_phase` = `prepare`.

The workflow builds and signs the candidate, installs the hardware-test copy into a persistent directory under `C:\ProgramData\DokkomplektE2E`, installs the watcher, pins app/source/PowerShell hashes, creates the post-reboot scheduled verification task and writes the pending plan. The workflow finishes with a successful prepare artifact rather than treating the intentional reboot boundary as a product failure.

Then perform a **real Windows restart**. After boot, log into the same dedicated runner account. The Dokkomplekt watcher and the GitHub runner AtLogOn task must start in that interactive session. The post-reboot verifier injects the prepared document only after the new boot, waits for exactly-once watcher processing and writes raw evidence to `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH`.

Do not fake the reboot by restarting a process or a service. `verify_reboot_evidence.ps1` binds the evidence to the Windows boot timestamp and the prepared nonce/hashes.

### Phase 2 — `verify`

For the same `release_sha`, run **Windows Hardware E2E** again with `reboot_phase=verify`. The workflow rebuilds the exact source, verifies the raw reboot evidence against the current boot and release source fingerprint, executes Word COM PrintOut, requires PrintService Event 307 for the configured real printer, verifies signed application/installer/runtime, GUI/no-console behavior and uninstall, archives the raw reboot JSON and cleans the persistent prepare installation.

The `FULL DOKKOMPLEKT AUTOPILOT` `production-hardware` scope dispatches the hardware workflow in its normal verification mode. Therefore the reboot evidence for that exact SHA must already have been prepared and produced before using Autopilot for final production acceptance.

## What constitutes production PASS

A production hardware PASS requires the final hardware artifact to contain evidence for, at minimum:

- exact protected source SHA and signed Rust/RustSec gate;
- complete signed offline runtime and SBOM;
- Authenticode-valid staged PE sidecars, application and NSIS installer;
- real image-only PDF OCR;
- visible installed GUI with no unexpected console/PowerShell windows;
- real Microsoft Word COM PrintOut;
- PrintService Event 307 for the dedicated printer;
- a genuine Windows reboot with watcher start after reboot and exactly-once document processing;
- silent install and clean uninstall;
- hashes binding all final evidence.

Only after this is green should issue #5 be considered for closure. A normal hosted `software` Autopilot PASS remains intentionally insufficient for these physical production claims.

## Operational rules

Keep the machine dedicated and boring: do not browse, develop or use it as a daily workstation. Do not leave another copy of the Actions runner installed as a service. Do not let Windows sleep during a release run. Keep Word activated for the runner account and the printer powered/reachable. Rotate the GitHub runner registration only when re-registering the host; production signing secrets remain in the protected GitHub environment, not on disk in the repository checkout.
