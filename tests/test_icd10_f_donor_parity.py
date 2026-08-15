from __future__ import annotations

import hashlib
from pathlib import Path

EXPECTED_DONOR_F_ROW_COUNT = 333
EXPECTED_DONOR_F_SHA256 = "f7e08c2250ca244bbbf656ce080576f4b39e88c2f766e1e6350b7602bd8f9ac0"

def _donor_sourced_rows() -> dict[str, str]:
    rows: dict[str, str] = {}
    path = Path(__file__).resolve().parents[1] / "resources" / "icd10_f.tsv"
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not raw or raw.startswith("#"):
            continue
        parts = raw.split("\t")
        assert len(parts) >= 4, raw
        code, title, _kind, source = parts[:4]
        if source != "icd10_f_data.py" or "-" in code:
            continue
        assert code not in rows, f"duplicate donor-sourced F code: {code}"
        rows[code] = title
    return rows

def test_icd10_f_catalog_is_exact_pinned_donor_mirror() -> None:
    rows = _donor_sourced_rows()
    assert len(rows) == EXPECTED_DONOR_F_ROW_COUNT
    canonical = "".join(f"{code}\t{rows[code]}\n" for code in sorted(rows))
    assert hashlib.sha256(canonical.encode("utf-8")).hexdigest() == EXPECTED_DONOR_F_SHA256
