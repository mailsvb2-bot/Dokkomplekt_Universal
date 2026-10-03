# Shared licensing profile for MedicalDiaryAutofill

The existing Dokkomplekt license server can serve MedicalDiaryAutofill as a
separate product instance. The Windows app must never receive the Ed25519
private issuer key, provider credentials, callback secret, recovery secret, or
license-issue control secret.

Recommended production environment for the MedicalDiaryAutofill instance:

- `DOKKOMPLEKT_LICENSE_PRODUCT_ID=diary_filler`
- `DOKKOMPLEKT_LICENSE_PRODUCT_TITLE=MedicalDiaryAutofill`
- `DOKKOMPLEKT_DEFAULT_LICENSE_DAYS=31`
- `DOKKOMPLEKT_OWNER_LICENSE_DAYS=36500`
- `DOKKOMPLEKT_OWNER_BOOTSTRAP_CODE_SHA256=<domain-separated digest>`
- `DOKKOMPLEKT_PAYMENT_PROVIDER=yookassa` or `sbp`
- normal production PostgreSQL, issuer-key, YooKassa and callback/recovery
  secrets from the existing server deployment contract.

Generate the owner-code digest interactively:

```
python scripts/hash_owner_bootstrap_code.py
```

Paid clients create an order, keep the returned `order_access_token`, wait for
verified payment, activate their machine, then request the signed license with
`Authorization: Bearer <order_access_token>`. The legacy server-only
`issue_token` path remains available for backward compatibility but is not
required by MedicalDiaryAutofill.

Owner access uses `POST /api/owner/license`. The bootstrap code is checked
server-side only; the application binary does not contain the plaintext code.
The returned VIP license is Ed25519-signed, product-scoped, machine-bound and
marked with signed metadata `role=owner_superadmin`, `access=unlimited`.
