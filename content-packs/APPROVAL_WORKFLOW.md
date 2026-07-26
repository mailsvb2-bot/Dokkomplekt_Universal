# Tier‑1 approval workflow

The bundled HR, legal and accounting packs intentionally remain `draft_only`.
No software author can truthfully convert them into legally approved forms for
all organisations and jurisdictions without a named professional review.
Version 18.3.0 closes the production path instead of changing that label:

1. A legal/HR/accounting owner reviews every DOCX for one named organisation and jurisdiction.
2. `scripts/approve_content_pack.py create` records the legal basis, scope, reviewer, validity period and every exact template SHA‑256.
3. The evidence is signed by the organisation’s Ed25519 key.
4. `scripts/approve_content_pack.py verify` requires the pinned public key and fails if one byte of a template or pack version changed.
5. In the desktop app, the organisation separately approves the exact active template revision. Automatic printing remains blocked until that local approval exists and confidence triage passes.

Example:

```bash
python scripts/approve_content_pack.py create \
  --pack content-packs/tier1-legal-ru \
  --organization "ООО Пример" \
  --reviewer "Иванов И.И., руководитель юридической службы" \
  --jurisdiction "Российская Федерация / договорная работа организации" \
  --legal-basis "Локальные формы и применимые нормы по состоянию на дату ревью" \
  --review-scope "Все 4 формы пакета; реквизиты и маршруты согласования" \
  --valid-until 2027-12-31 \
  --signing-key organization-approval-seed.b64 \
  --output tier1-legal-ru.approval.json

python scripts/approve_content_pack.py verify \
  --pack content-packs/tier1-legal-ru \
  --approval tier1-legal-ru.approval.json \
  --trusted-public-key organization-approval-public.b64
```

An approval is deliberately scoped to the named organisation and jurisdiction.
It is not a universal legal warranty, and it expires or becomes invalid whenever
a template hash or pack version changes.
