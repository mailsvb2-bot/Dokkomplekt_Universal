# Provenance gate trigger

This file is release-evidence metadata and is deliberately excluded from `SOURCE_MANIFEST_SHA256.txt`.

It records that runner-generated manifests were synchronized on PR #3 and PR #4, and that each subsequent full verification was intentionally triggered by a non-bot commit so GitHub Actions could execute without `action_required`.

PR #4 verifies the strict installer launch-liveness contract: a GUI process or Linux process group must remain alive for the complete smoke interval; an early exit with code 0 is a failure.
