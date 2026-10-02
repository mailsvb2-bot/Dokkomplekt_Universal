# Production Live Windows User E2E

The canonical live-user contour runs as **two independent persistent jobs** in the private `mailsvb2-bot/Dokkomplekt_Hardware_Validation` repository:

- **Windows 10 x64** on runner label `dokkomplekt-win10-live`;
- **Windows 11 x64** on runner label `dokkomplekt-win11-live`.

The jobs use separate Windows instances and interactive sessions. `fail-fast: false` keeps one platform running even if the other fails. Release acceptance requires both rows to pass against the same exact signed handoff.

It is intentionally different from browser E2E with mocked Tauri IPC. Live evidence is accepted only from the signed offline NSIS installed into a real interactive Windows user session after the cryptographically signed handoff has been verified.

## Lanes

- `installed-baseline`: first run, native file dialogs, template creation, persistence, restart, invalid/corrupt/oversized sources, folders, licensing and installed preview.
- `installed-e1`: real cross-domain generation, medical diaries, ICD-10, Scanner, preflight, versioning, template transfer, bundle semantics and privacy read-back.
- `hardware-live`: Word COM, installed PDF conversion, real printer queue/Event 307, watcher registration, offline runtime and silent uninstall.
- `reboot-live`: genuine Windows reboot/logon, watcher recovery and exactly-once post-reboot case completion.
- `update-live`: previous-version to current-version signed update, recovery and rollback. This lane remains fail-closed until a real executor produces evidence.

The source of truth is `verification/e2e/LIVE_USER_SCENARIOS.json`. Every FPR-01 through FPR-23 is mandatory. A new user-visible capability must be added to the registry together with a live executor and an evidence marker.

## Runner provisioning

Each dedicated Windows account must keep its runner in an interactive logon session and must expose a local hardware configuration value:

`DOKKOMPLEKT_PREVIOUS_SIGNED_INSTALLER=C:\path\to\previous-production-signed\Dokkomplekt-setup.exe`

The referenced installer must be a real older production release with Valid Authenticode and a version lower than the candidate under test. The host preflight fails closed if this file is absent or unsigned.

## Execution order

1. Hosted Windows builds/signs the exact release and creates `SIGNED_HANDOFF.json`.
2. Windows 10 and Windows 11 runners independently verify the handoff and Authenticode/runtime signatures.
3. Each row runs `tests/windows/windows_live_user_e2e.ps1` against the signed **offline** NSIS.
4. Each row runs `tests/windows/windows_live_update_e2e.ps1` from a previous signed production release through the real Settings → Check updates → Install and restart flow.
5. Each row independently executes Word/PDF/printer/watcher hardware proof.
6. Prepare phase writes platform-local persistent reboot state.
7. After real Windows restarts and interactive logons on both runners, verify phase validates watcher recovery/exactly-once and final cleanup independently.
8. Evidence is bound to the exact release SHA and uploaded as platform-specific artifacts (`win10` and `win11`).

A green unit test, mocked browser flow, unsigned preview installer, or GitHub-hosted Windows run cannot substitute for this contour.
