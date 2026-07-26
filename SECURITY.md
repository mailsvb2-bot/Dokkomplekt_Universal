# Security Policy

## Supported releases

Security fixes are accepted for the current release line. Installers must be built only after the mandatory Rust, TypeScript, Python, browser and platform packaging gates pass.

## Reporting a vulnerability

Do not disclose suspected vulnerabilities in public issues. Send a private report to the project owner with:

- affected version and operating system;
- reproducible steps and expected/actual behaviour;
- impact assessment;
- logs or a minimal proof of concept with personal and medical data removed.

Never include real patient, employee, customer, payment or license secrets.

## Security boundaries

- License issuer private keys, payment secrets, callback secrets and issue-token secrets belong only on the server.
- The desktop application contains public verification keys only.
- Update manifests use a separate Ed25519 key from product licensing.
- Update manifest URL and public key are compile-time trust anchors and are not accepted from UI or persisted user state.
- Update downloads are HTTPS-only, redirect-free, credential-free, fragment-free and reject localhost/private/service IP addresses. Signed size and SHA-256 are verified before a package is published into the local verified-update cache.
- Production license server startup is fail-closed when required secrets or PostgreSQL are missing.

## Dependency and release handling

Run `npm audit`, the complete Rust gate and platform installer contracts before publishing. Do not release artifacts built from a dirty tree or without `SOURCE_MANIFEST_SHA256.txt` matching the final archive contents.
