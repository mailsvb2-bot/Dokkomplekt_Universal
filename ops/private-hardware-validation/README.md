# Private hardware validation repository scaffold

This directory is stored in the public source repository as audited, non-secret infrastructure code. The actual hardware runner and production signing secrets live behind the separate private repository:

`mailsvb2-bot/Dokkomplekt_Hardware_Validation`

## Files to install in the private repository

Copy `windows-hardware-e2e.yml` to `.github/workflows/windows-hardware-e2e.yml` on the private repository's protected `main` branch.

No Dokkomplekt application source needs to be copied to the private repository. The private workflow receives an exact public source SHA and clones `mailsvb2-bot/Dokkomplekt_Universal` at that SHA without using private-repository credentials.

## One physical Windows runner

Only the `hardware-evidence` job is self-hosted. Register one interactive Windows runner in the private repository with custom label:

`dokkomplekt-hardware`

It needs licensed Microsoft Word and the dedicated real printer used by the hardware acceptance test. It must not contain production signing keys.

The `signed-runtime-build` job runs on ephemeral GitHub-hosted `windows-latest` in protected environment `windows-production-signing`; no user-owned signing/runtime computer is required.

## Private repository settings

1. Keep repository visibility `private`.
2. Protect `main` against force-push/deletion.
3. Create protected environment `windows-production-signing` for Authenticode/handoff signing secrets and hosted runtime-source variables.
4. Create protected environment `windows-hardware-validation` for the one physical hardware runner's printer/reboot variables only.
5. Never expose signing private keys to `windows-hardware-validation`.
6. Never enable untrusted pull-request workflows that can consume production signing environments or the physical hardware runner.

### `windows-production-signing` runtime inputs

The hosted signing job consumes an immutable reviewed runtime over HTTPS:

- `DOKKOMPLEKT_RUNTIME_BUNDLE_URL`;
- `DOKKOMPLEKT_RUNTIME_BUNDLE_PAYLOAD_URL`;
- `DOKKOMPLEKT_RUNTIME_BUNDLE_SIGNATURE_URL`;
- `DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL`;
- `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
- `DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64`.

The exact runtime payload must verify both the release/runtime Ed25519 signature and a second independently generated offline approval signature before staging.

## Public bridge settings

The public source repository pins the private target in `.github/workflows/windows-hardware-e2e.yml`:

- repository `mailsvb2-bot/Dokkomplekt_Hardware_Validation`;
- workflow `windows-hardware-e2e.yml`.

Environment `windows-hardware-dispatch` therefore needs only secret `DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN`, with the minimum private-repository metadata/Actions access needed to dispatch and read workflow runs. The public dispatcher verifies that the target remains a separate private repository.
