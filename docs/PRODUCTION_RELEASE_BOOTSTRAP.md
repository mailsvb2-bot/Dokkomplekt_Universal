# Production release bootstrap

A public production release is intentionally fail-closed until two independent domains are ready: the protected GitHub-hosted signing environment and the single physical Windows hardware-evidence runner. There is no production `dokkomplekt-runtime` self-hosted signing runner.

## 1. Protected hosted build/signing domain

`Build Signed Offline Installers` uses `windows-latest` inside the protected GitHub environment `windows-production-signing`. The environment must provide the real production trust anchors and immutable HTTPS delivery endpoints used by `build-installers.yml`:

- compile-time Ed25519 public keys: `DOKKOMPLEKT_GATE_PUBKEY_B64`, `DOKKOMPLEKT_LICENSE_PUBKEY_B64`, `DOKKOMPLEKT_UPDATE_PUBKEY_B64`, `DOKKOMPLEKT_THRESHOLD_PUBKEY_B64`, `DOKKOMPLEKT_REFDATA_PUBKEY_B64`;
- application delivery endpoints: `DOKKOMPLEKT_UPDATE_MANIFEST_URL`, `DOKKOMPLEKT_REFDATA_URL`, `DOKKOMPLEKT_COMPONENTS_CATALOG_URL`, `DOKKOMPLEKT_COMPONENTS_BASE_URL`;
- approved core-runtime verification: `DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64`, `DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64`, `DOKKOMPLEKT_RUNTIME_BUNDLE_URL`, `DOKKOMPLEKT_RUNTIME_BUNDLE_PAYLOAD_URL`, `DOKKOMPLEKT_RUNTIME_BUNDLE_SIGNATURE_URL`, `DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL`;
- Authenticode policy: `DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND=certificate-store`, `DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT`, `DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER`, `DOKKOMPLEKT_TIMESTAMP_SERVER`;
- protected signing secrets used only by the steps that require them: `DOKKOMPLEKT_GATE_PRIVATE_KEY_B64`, `DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64`, `DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64`.

The production Authenticode private key must remain non-exportable in an approved HSM/KSP/CSP provider. The legacy PFX backend in `scripts/sign_windows_release.ps1` is retained only for non-production compatibility and is rejected when `DOKKOMPLEKT_RELEASE_MODE=production`.

The repository does not manufacture or upload an Authenticode private key. A reviewed provider-specific provisioning/authentication integration must make the approved certificate available to the ephemeral runner as `Cert:\CurrentUser\My` before signing. The workflow verifies the actual certificate, provider and non-exportability with `scripts/sign_windows_release.ps1 -VerifyCertificateOnly` before it downloads the runtime or performs an expensive production build. Merely setting a thumbprint/provider variable is not sufficient.

The hosted environment is checked in layers:

```text
scripts/release_environment_preflight.py --mode production-build
scripts/verify_windows_hosted_signing_runner.py
scripts/sign_windows_release.ps1 -VerifyCertificateOnly
```

The first validates public compile-time anchors/endpoints, the second validates the GitHub-hosted trust boundary and signing policy, and the third proves that the real certificate/private-key provider is actually usable. The retired `windows-runtime` preflight mode is intentionally fail-closed so old two-runner/PFX automation cannot silently become a second production path.

The approved offline runtime is the bounded `core` profile: Tesseract, Poppler, LibreOffice, SumatraPDF and 7-Zip. The semantic runtime/model is not embedded in the stock core installer and remains a separately governed optional component. Runtime bytes must be immutable, supply-chain locked and bound to both the runtime signature and independent offline-composition approval.

## 2. Physical Windows hardware-evidence domain

The only required self-hosted Windows role is `dokkomplekt-hardware` in the private hardware-validation repository. It must be an interactive Windows host with licensed Microsoft Word, a real dedicated printer queue, PrintService logging and persistent storage for prepare -> real restart/logon -> verify evidence.

The hardware runner must not receive production signing private keys or Authenticode provisioning material. Its configuration is limited to hardware/public-verification inputs such as `DOKKOMPLEKT_TEST_PRINTER`, `DOKKOMPLEKT_REBOOT_EVIDENCE_PATH`, `DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT` and the public runtime verification key required by the signed handoff.

Its local environment contract remains:

```powershell
python scripts/release_environment_preflight.py --mode windows-hardware --json-report verification/release/hardware-preflight.json
```

The production release remains blocked until the hosted signer produces a valid signed handoff and the physical runner proves real DOCX/Word/print, PrintService evidence, a real Windows restart/logon with watcher exactly-once behavior, and final uninstall/evidence binding to the exact release SHA/request ID. Hosted/mock tests never substitute for those physical acceptance facts.
