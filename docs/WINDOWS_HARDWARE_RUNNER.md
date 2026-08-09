# Windows Hardware Runner

Dokkomplekt production hardware acceptance must run on a dedicated interactive Windows host with real Microsoft Word, a real printer queue, signed binaries, a complete offline runtime and a genuine reboot. It is intentionally separate from ordinary GitHub-hosted CI.

## Security boundary: never attach the production runner to the public source repo

`mailsvb2-bot/Dokkomplekt_Universal` is public. The physical/self-hosted runner must **not** be registered in that repository. A persistent self-hosted runner attached to a public repository creates an unacceptable attack surface from public/fork workflows.

Use the dedicated **private** repository:

`mailsvb2-bot/Dokkomplekt_Hardware_Validation`

Architecture:

1. public `Dokkomplekt_Universal` contains source and the hosted `Windows Hardware E2E` dispatcher;
2. the dispatcher is pinned to `mailsvb2-bot/Dokkomplekt_Hardware_Validation` and first proves through the GitHub API that the target is private;
3. the private repository contains the real self-hosted workflow from `ops/private-hardware-validation/windows-hardware-e2e.yml`;
4. the Windows runner is registered only to that private repository;
5. Authenticode/runtime/update/gate private keys live only in the private repository environment `windows-production-signing`;
6. the private workflow anonymously checks out the public source repository at the exact approved 40-character SHA, proves that SHA is on public `main`, and then executes the hardware contour;
7. the public dispatcher waits for the correlated private workflow run and fails unless its conclusion is `success`.

The public repository therefore never directly schedules untrusted repository jobs on the production Windows host and never stores production signing secrets.

## Private validation repository bootstrap

The private repository is:

`mailsvb2-bot/Dokkomplekt_Hardware_Validation`

Its protected `main` branch must contain:

`.github/workflows/windows-hardware-e2e.yml`

from the audited scaffold `ops/private-hardware-validation/windows-hardware-e2e.yml`. Keep that repository private.

The private workflow accepts four inputs: `source_repository`, `release_sha`, `reboot_phase`, and a correlation `request_id`. Its `run-name` includes the request id so the public hosted dispatcher can bind its verdict to exactly the run it created.

## Public dispatcher environment

In public `Dokkomplekt_Universal`, use the protected environment:

`windows-hardware-dispatch`

The target is pinned directly in `.github/workflows/windows-hardware-e2e.yml`:

- `DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY=mailsvb2-bot/Dokkomplekt_Hardware_Validation`;
- `DOKKOMPLEKT_HARDWARE_VALIDATION_WORKFLOW=windows-hardware-e2e.yml`.

No GitHub Actions variable is required for either value.

Set environment secret:

- `DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN` — a narrowly scoped fine-grained credential restricted to `Dokkomplekt_Hardware_Validation`, with Actions write permission so it can dispatch the private workflow and read the resulting run status. Do not give it Contents write, Administration, Packages, Issues, Pull requests or signing-secret permissions.

The public `Windows Hardware E2E` workflow runs only on GitHub-hosted `ubuntu-latest`, requires `release_sha == github.sha` on public `main`, verifies the pinned target repository reports `private=true`, dispatches the private workflow and waits for the exact correlation id. It contains no `runs-on: self-hosted` job.

## Required Windows host

Use a dedicated Windows 11 Pro x64 physical machine, or a Windows 11 Pro x64 VM only if the printer and desktop/Word behavior are genuinely representative of the acceptance claim. The account must be a dedicated Windows user with administrator rights on this test host.

Keep that user logged in while jobs execute. Do **not** install the Actions runner as a Windows service. The test requires visible GUI evidence and Word COM; service/Session 0 execution is rejected by the host preflight.

Required host components:

- licensed and activated desktop Microsoft Word for the runner user;
- Microsoft Edge WebView2 Runtime;
- a dedicated real printer queue, not Microsoft Print to PDF/XPS/OneNote/Fax;
- Git for Windows;
- PowerShell 7;
- Visual Studio Build Tools with the C++ VCTools workload;
- OpenSSL available to the interactive runner;
- outbound HTTPS to GitHub, Rust, npm and the production Dokkomplekt component/reference/update endpoints;
- runner-owned production sidecar tree and manifest;
- enough disk for Rust/Tauri builds, LibreOffice, OCR assets, the semantic model and release bundles.

Required labels:

`self-hosted`, `Windows`, `X64`, `dokkomplekt-hardware-e2e`

## Register the runner — only in the private repository

In the **private validation repository**, open **Settings → Actions → Runners → New self-hosted runner** and obtain a fresh short-lived registration token.

Preferred path: download/run `bootstrap-hardware-runner.ps1` from the private repository in an elevated interactive PowerShell window. It downloads the audited registration/bootstrap scripts from a pinned public source SHA and registers only to `Dokkomplekt_Hardware_Validation`.

Direct source-checkout path:

```powershell
.\scripts\register_windows_hardware_runner.ps1 `
  -RepositoryUrl 'https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation' `
  -PrinterName 'YOUR_REAL_PRINTER_QUEUE' `
  -SidecarManifestPath 'C:\DokkomplektRuntime\windows-x86_64-manifest.json' `
  -InstallPrerequisites
```

The registration script explicitly refuses `https://github.com/mailsvb2-bot/Dokkomplekt_Universal`. It prompts for the GitHub registration token as `SecureString`, verifies the downloaded GitHub Actions runner ZIP against the SHA-256 digest published with the release, registers label `dokkomplekt-hardware-e2e`, and starts the runner through an interactive **AtLogOn scheduled task**, not a Windows service.

Bootstrap evidence is written to:

`C:\ProgramData\DokkomplektE2E\HARDWARE_RUNNER_BOOTSTRAP.json`

## Runner-owned sidecar manifest

Production input must be an absolute runner-owned manifest such as:

`C:\DokkomplektRuntime\windows-x86_64-manifest.json`

Do not use the source placeholder as production evidence. The production manifest must satisfy `scripts/release_environment_preflight.py` and include:

- `schema: 1`;
- `target: windows-x86_64`;
- `supply_chain_locked: true`;
- SHA-256, version, source URL, license and license notice hashes for every file;
- complete portable-tree inventory and distribution review;
- complete Tesseract language data, Poppler, LibreOffice, SumatraPDF, 7-Zip, msgconvert, llama.cpp server and an approved GGUF semantic model;
- any additional runtime entry required by the current release contour.

`scripts/prepare_sidecars.py` stages only these reviewed local files and intentionally does not fetch production binaries from the internet.

## Private `windows-production-signing` environment

Create this environment **only in the private validation repository**. Restrict it to private protected `main`; using required reviewers for release signing is recommended.

Variables required by the current private workflow:

- `DOKKOMPLEKT_TEST_PRINTER`;
- `DOKKOMPLEKT_TEST_DUPLEX`;
- `DOKKOMPLEKT_TEST_TRAY`;
- `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH` — recommended `C:\ProgramData\DokkomplektE2E\WINDOWS_REBOOT_E2E_RAW.json`;
- `DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT` — stable absolute real source fixture path on the runner;
- `DOKKOMPLEKT_SIDECAR_MANIFEST_PATH`;
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
- `DOKKOMPLEKT_SIGNING_SCRIPT_SHA256` — SHA-256 of the audited `scripts/sign_windows_release.ps1` from the approved public release tree.

Secrets required only in the private environment:

- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64`;
- `DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD`;
- `DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64`;
- `DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64`;
- `DOKKOMPLEKT_GATE_PRIVATE_KEY_B64`.

Use a real Authenticode code-signing certificate. Never commit PFX/private keys or signing keys to either repository.

## Host preflight

Before signing/building, the private workflow calls `scripts/verify_windows_hardware_runner.ps1` from the exact public release SHA. It fails closed if:

- the runner executes as SYSTEM or in Session 0;
- an Actions runner Windows service exists;
- the interactive scheduled task/listener is absent;
- Word COM cannot start;
- the configured printer is virtual/unavailable;
- PrintService Operational logging is unavailable;
- VCTools, WebView2, Git, PowerShell 7 or OpenSSL are missing;
- the runner-owned sidecar manifest is not direct, absolute and supply-chain locked.

## Real reboot — two phases

A real reboot cannot resume the same process, so validation is explicitly two-phase for the same public `release_sha`.

### 1. `prepare`

Run public **Actions → Windows Hardware E2E → Run workflow** on `main` with the exact current `release_sha` and `reboot_phase=prepare`. The hosted public dispatcher invokes the private workflow. The private workflow builds/signs the exact source, creates persistent state under `C:\ProgramData\DokkomplektE2E`, installs the watcher and writes a post-reboot scheduled verifier.

Perform a **real Windows restart**, then log into the same dedicated runner account. The watcher and Actions runner AtLogOn tasks start in the interactive session. The prepared verifier injects the source only after the new boot and writes reboot evidence.

### 2. `verify`

For the same public SHA, run the public bridge again with `reboot_phase=verify`. The private workflow verifies the new boot identity and prepared hashes, executes real Word COM `PrintOut`, requires PrintService Event 307 for the configured printer, checks signed runtime/app/installer, GUI/no-console behavior, exactly-once watcher output and uninstall, archives the raw reboot evidence and removes the persistent prepare installation.

`FULL DOKKOMPLEKT AUTOPILOT` with `scope=production-hardware` uses the public bridge in verification mode, so valid reboot evidence for that exact SHA must already exist.

## Production PASS

A hardware production PASS requires evidence for all of the following:

- exact protected public source SHA;
- signed Rust/RustSec gate;
- complete signed offline runtime and SBOM;
- Authenticode-valid staged PE sidecars, application and NSIS installer;
- image-only PDF OCR;
- installed visible GUI without unexpected console/PowerShell windows;
- licensed Microsoft Word COM `PrintOut`;
- PrintService Event 307 for the dedicated real printer;
- genuine Windows reboot, watcher start after reboot and exactly-once output;
- silent install and clean uninstall;
- hashes binding all evidence to the approved source.

Only then can issue #5 be considered for closure. Hosted `software` Autopilot PASS is intentionally insufficient for these physical claims.
