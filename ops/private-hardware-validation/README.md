# Private hardware validation repository scaffold

This directory is intentionally stored in the public source repository only as audited, non-secret infrastructure code. The actual self-hosted runner and production signing secrets must live in a separate private repository.

Recommended target repository:

`mailsvb2-bot/Dokkomplekt_Hardware_Validation`

## Files to install in the private repository

Copy `windows-hardware-e2e.yml` to `.github/workflows/windows-hardware-e2e.yml` on the private repository's protected `main` branch.

No Dokkomplekt application source needs to be copied to the private repository. The private workflow receives an exact public source SHA and clones `mailsvb2-bot/Dokkomplekt_Universal` at that SHA without using the private repository token.

## Private repository settings

1. Keep repository visibility `private`.
2. Protect `main` against force-push/deletion.
3. Create environment `windows-production-signing` and store all signing/private-key secrets only there.
4. Register the Windows self-hosted runner only to this private repository with custom label `dokkomplekt-hardware-e2e`.
5. Configure the environment variables and secrets listed in `docs/WINDOWS_HARDWARE_RUNNER.md`.
6. Never enable pull-request workflows in the private validation repository that target the production self-hosted runner.

## Public bridge settings

The public source repository uses environment `windows-hardware-dispatch` with:

- variable `DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY` pointing to this private repository;
- optional variable `DOKKOMPLEKT_HARDWARE_VALIDATION_WORKFLOW=windows-hardware-e2e.yml`;
- secret `DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN` with only the minimum private-repository metadata/Actions access needed to dispatch and read workflow runs.

The public bridge refuses a non-private target before dispatching.
