# Windows runtime-lock offline approval

The runner-owned production runtime manifest is a security-sensitive input. SHA-256 inside the manifest detects file drift, but SHA-256 alone does not prove that the manifest itself was approved by a trusted release reviewer.

Production hardware validation therefore requires an independent detached **Ed25519 approval signature** over the exact bytes of:

`C:\DokkomplektRuntime\locked\windows-x86_64-manifest.json`

The approval private key must never be copied to the self-hosted hardware runner and must not be stored as a GitHub Actions secret. The hardware runner receives only the signed manifest, its detached `.sig`, and a pinned trusted public key from the protected `windows-production-signing` environment.

## 1. Build and verify the runtime kit on the runner

From the exact approved public source checkout:

```powershell
.\scripts\prepare_windows_production_runtime.ps1 `
  -SpecPath 'C:\DokkomplektRuntime\runtime-kit.json' `
  -OutputDir 'C:\DokkomplektRuntime\locked'
```

This command deliberately removes a stale `.sig` if the manifest is rebuilt. A signature is valid for one exact manifest byte sequence only.

Record the printed manifest SHA-256 and transfer the exact manifest to the separate approval workstation by an approved channel.

## 2. Sign on a separate trusted approval workstation

The workstation needs the same audited source script and an Ed25519 private key that never exists on the hardware runner.

```powershell
python scripts\windows_runtime_lock_approval.py sign `
  'C:\Approval\windows-x86_64-manifest.json' `
  --private-key 'X:\offline-keys\dokkomplekt-runtime-lock-approval-private.pem' `
  --signature 'C:\Approval\windows-x86_64-manifest.json.sig' `
  --metadata 'C:\Approval\windows-x86_64-manifest.json.approval.json' `
  --reviewer 'release-reviewer'
```

The command signs the exact manifest bytes with Ed25519 and emits a raw 64-byte detached signature plus approval metadata containing the manifest SHA-256 and approval public-key ID.

Only the `.sig` and optional approval metadata are returned to the hardware runner. Never return or expose the approval private key.

## 3. Pin the approval public key in the private GitHub environment

In private repository `mailsvb2-bot/Dokkomplekt_Hardware_Validation`, protected environment `windows-production-signing`, configure non-secret variable:

`DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64`

Its value is the Base64 encoding of the trusted Ed25519 **public PEM** matching the offline approval private key.

There is intentionally no `DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PRIVATE_*` GitHub secret. The private approval key is outside GitHub and outside the self-hosted runner trust boundary.

## 4. Place the signature beside the manifest

The hardware runner must contain:

```text
C:\DokkomplektRuntime\locked\windows-x86_64-manifest.json
C:\DokkomplektRuntime\locked\windows-x86_64-manifest.json.sig
```

The private hardware workflow verifies the signature using the pinned public key **before** it executes `prepare_sidecars.py`. Missing signature, invalid Base64 public key, wrong Ed25519 key, changed manifest bytes or invalid signature stops the workflow before runtime staging.

Verification evidence is written to:

`verification/release/RUNTIME_LOCK_APPROVAL.json`

and is uploaded with the other immutable hardware evidence.

## 5. This is separate from runtime-bundle signing

The offline approval signature protects the **input lock before staging**.

Later in the protected hardware workflow, Dokkomplekt independently:

- verifies Authenticode on staged PE sidecars;
- probes the staged runtime and OCR fixture;
- creates a deterministic offline runtime ZIP/SBOM;
- signs that output bundle with the production runtime signing key and verifies it against `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`;
- signs the application binary and NSIS installer with Authenticode.

Both boundaries are required: offline approval prevents an already-compromised runner from silently replacing the reviewed runtime input, while release/runtime signing protects the produced deliverables.
