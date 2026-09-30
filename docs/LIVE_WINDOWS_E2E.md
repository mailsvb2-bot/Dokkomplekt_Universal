# Production Live Windows User E2E

The canonical live-user contour runs on one persistent, dedicated **Windows 11 x64** machine registered only in the private `mailsvb2-bot/Dokkomplekt_Hardware_Validation` repository.

It is intentionally different from browser E2E with mocked Tauri IPC. Live evidence is accepted only from the signed offline NSIS installed into a real interactive Windows user session after the cryptographically signed handoff has been verified.

## Lanes

- `installed-baseline`: first run, native file dialogs, template creation, persistence, restart, invalid/corrupt/oversized sources, folders, licensing and installed preview.
- `installed-e1`: real cross-domain generation, medical diaries, ICD-10, Scanner, preflight, versioning, template transfer, bundle semantics and privacy read-back.
- `hardware-live`: Word COM, installed PDF conversion, real printer queue/Event 307, watcher registration, offline runtime and silent uninstall.
- `reboot-live`: genuine Windows reboot/logon, watcher recovery and exactly-once post-reboot case completion.
- `update-live`: previous-version to current-version signed update, recovery and rollback. This lane remains fail-closed until a real executor produces evidence.

The source of truth is `verification/e2e/LIVE_USER_SCENARIOS.json`. Every FPR-01 through FPR-23 is mandatory. A new user-visible capability must be added to the registry together with a live executor and an evidence marker.

## Runner provisioning

The dedicated Windows account must keep the runner in an interactive logon session and must expose a local hardware configuration value:

`DOKKOMPLEKT_PREVIOUS_SIGNED_INSTALLER=C:\path\to\previous-production-signed\Dokkomplekt-setup.exe`

The referenced installer must be a real older production release with Valid Authenticode and a version lower than the candidate under test. The host preflight fails closed if this file is absent or unsigned.

## Execution order

1. Hosted Windows builds/signs the exact release and creates `SIGNED_HANDOFF.json`.
2. The physical runner verifies the handoff and Authenticode/runtime signatures.
3. `tests/windows/windows_live_user_e2e.ps1` runs the installed baseline and E1 suites against the signed **offline** NSIS.
4. `tests/windows/windows_live_update_e2e.ps1` installs the previous signed production release and drives the real Settings → Check updates → Install and restart flow to the candidate version.
5. The same physical runner executes Word/PDF/printer/watcher hardware proof.
6. Prepare phase writes persistent reboot state.
7. After a real Windows restart and interactive logon, verify phase validates watcher recovery/exactly-once and final cleanup.
8. Evidence is bound to the exact release SHA and uploaded.

A green unit test, mocked browser flow, unsigned preview installer, or GitHub-hosted Windows run cannot substitute for this contour.
