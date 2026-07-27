# Source and release provenance policy

## Two independent manifests

`SOURCE_MANIFEST_SHA256.txt` covers authored, redistributable source files in the deterministic source archive. It deliberately excludes mutable build output and CI evidence under `verification/` and `build-evidence/`.

CI logs, installer smoke logs and binary bundles are release evidence, not source. They are retained as GitHub Actions artifacts and indexed by immutable SHA-256 digests in the versioned evidence record under this directory.

Mixing ignored `*.log` files into the source manifest made the manifest depend on the machine that built the archive. That design is forbidden from the 18.4.3 provenance repair onward.

## Mandatory gate

Every pull request and every push to `main` runs `Source Provenance`. The gate independently regenerates the source manifest, uploads the generated candidate and a machine-readable mismatch report, and fails unless the checked-in manifest matches the source tree exactly.

`tests/test_source_manifest_integrity.py` repeats the same contract inside the Python regression wall so the check cannot silently disappear from CI.

## Historical limitation

The repository was imported after much of the 18.x development had already happened outside Git. The original commit-by-commit history before the import does not exist in this repository and must not be invented retroactively.

Available historical source archives may be indexed by archive SHA-256 and described as reconstructed snapshots, but they are not represented as original development commits. Future releases must preserve normal Git history, source-manifest integrity and CI artifact digests from the moment each change enters the repository.
