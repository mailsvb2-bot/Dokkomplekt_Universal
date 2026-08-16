from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/LEGACY_MIGRATION_INVENTORY.json"
ALLOWED = {"migrated-core", "migrated-domain-profile", "superseded", "not-needed", "missing"}
EXPECTED_DONORS = {
    "mailsvb2-bot/Dokkomplekt": "b4bd25de24e5fd7c5c3374bd9928ce87fa5fdcbd",
    "mailsvb2-bot/diary-filler": "cee7d863e21fdf5c9d9a4d8d88732e9a10819ec7",
}
REQUIRED_SENTINELS = {
    "mailsvb2-bot/Dokkomplekt": {
        "actions_creation_foldering.py",
        "actions_required_fields_popup.py",
        "actions_diary_flow.py",
        "desktop_intake_agent.py",
        "universal_scanner.py",
    },
    "mailsvb2-bot/diary-filler": {
        "dialog_document_details.py",
        "diary_template_selection.py",
        "diary_text_selection.py",
        "diary_writer_entries.py",
        "dnd_mixin.py",
        "smoke_combined_part04_medical_generation.py",
    },
}


def fail(message: str) -> None:
    raise SystemExit(f"LEGACY MIGRATION INVENTORY FAILED: {message}")


def main() -> None:
    if not INVENTORY.is_file():
        fail("docs/LEGACY_MIGRATION_INVENTORY.json is missing")
    data = json.loads(INVENTORY.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1:
        fail("unsupported schema_version")
    if set(data.get("policy", {}).get("allowed_statuses", [])) != ALLOWED:
        fail("allowed status contract drifted")
    donors = {item.get("repository"): item for item in data.get("donors", [])}
    if set(donors) != set(EXPECTED_DONORS):
        fail(f"expected exactly donors {sorted(EXPECTED_DONORS)}")
    for repository, expected_commit in EXPECTED_DONORS.items():
        donor = donors[repository]
        if donor.get("commit") != expected_commit:
            fail(f"{repository} is not pinned to audited commit {expected_commit}")
        entries = donor.get("entries")
        if not isinstance(entries, list) or not entries:
            fail(f"{repository} has no migration entries")
        seen: set[str] = set()
        for entry in entries:
            path = str(entry.get("path", "")).strip()
            status = entry.get("status")
            targets = entry.get("targets", [])
            note = str(entry.get("note", "")).strip()
            if not path or path in seen:
                fail(f"{repository} has empty or duplicate path {path!r}")
            seen.add(path)
            if status not in ALLOWED:
                fail(f"{repository}:{path} has invalid status {status!r}")
            if status == "missing":
                fail(f"{repository}:{path} is still marked missing")
            if not note:
                fail(f"{repository}:{path} has no audit note")
            if status in {"migrated-core", "migrated-domain-profile", "superseded"}:
                if not targets:
                    fail(f"{repository}:{path} has no canonical target")
                for target in targets:
                    if not (ROOT / target).exists():
                        fail(f"{repository}:{path} points to absent target {target}")
            elif targets:
                fail(f"{repository}:{path} status {status} must not claim targets")
        missing_sentinels = REQUIRED_SENTINELS[repository] - seen
        if missing_sentinels:
            fail(f"{repository} lost required donor contracts: {sorted(missing_sentinels)}")
    print("LEGACY MIGRATION INVENTORY OK")


if __name__ == "__main__":
    main()
