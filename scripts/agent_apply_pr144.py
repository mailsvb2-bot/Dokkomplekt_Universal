from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/dokkomplekt-core/src/source_parser.rs"
REGISTRY = ROOT / "crates/dokkomplekt-core/src/field_registry.rs"
ALIASES = ROOT / "crates/dokkomplekt-core/src/field_aliases.rs"
DOMAIN = ROOT / "crates/dokkomplekt-core/src/domains/medical.rs"
INVENTORY = ROOT / "docs/LEGACY_MIGRATION_INVENTORY.json"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


source = SOURCE.read_text(encoding="utf-8")
source_anchor = '''        LabelRule {
            field: "medical.profile_status",
            labels: &[
                "Профильный статус при поступлении",
'''
source_insert = '''        LabelRule {
            field: "medical.epidemiology",
            labels: &["Эпидемиологический анамнез", "Wywiad epidemiologiczny"],
            multiline: true,
        },
        LabelRule {
            field: "medical.profile_observation",
            labels: &[
                "Профильное наблюдение",
                "Диспансерное наблюдение",
                "На учёте у психиатров",
                "На учете у психиатров",
            ],
            multiline: false,
        },
        LabelRule {
            field: "medical.disability",
            labels: &["Оформление инвалидности", "Инвалидность"],
            multiline: false,
        },
        LabelRule {
            field: "medical.rvk_referral",
            labels: &["Направление от РВК", "Направление РВК"],
            multiline: false,
        },
''' + source_anchor
source = replace_once(source, source_anchor, source_insert, "source medical profile facts")
SOURCE.write_text(source, encoding="utf-8")

registry = REGISTRY.read_text(encoding="utf-8")
registry_anchor = '''        field(
            "medical.profile_status",
            "Профильный статус",
'''
registry_insert = '''        field(
            "medical.epidemiology",
            "Эпидемиологический анамнез",
            DomainKind::Medical,
            false,
            &[
                "epidemiology",
                "Эпидемиологический анамнез",
                "Wywiad epidemiologiczny",
            ],
        ),
        field(
            "medical.profile_observation",
            "Профильное / диспансерное наблюдение",
            DomainKind::Medical,
            false,
            &[
                "profile_observation",
                "psych_account",
                "medical.psych_account",
                "Профильное наблюдение",
                "Диспансерное наблюдение",
                "На учёте у психиатров",
                "На учете у психиатров",
            ],
        ),
        field(
            "medical.disability",
            "Инвалидность",
            DomainKind::Medical,
            false,
            &["patient.disability", "Оформление инвалидности", "Инвалидность"],
        ),
        field(
            "medical.rvk_referral",
            "Направление от РВК",
            DomainKind::Medical,
            false,
            &["rvk_referral", "Направление от РВК", "Направление РВК"],
        ),
''' + registry_anchor
registry = replace_once(registry, registry_anchor, registry_insert, "registry medical profile facts")
REGISTRY.write_text(registry, encoding="utf-8")

aliases = ALIASES.read_text(encoding="utf-8")
canonical_anchor = '''        "status.objective" | "status.somatic" | "somatic_status" => "medical.somatic_status".into(),
'''
canonical_insert = '''        "profile_observation" | "psych_account" | "medical.psych_account" => {
            "medical.profile_observation".into()
        },
        "rvk_referral" => "medical.rvk_referral".into(),
        "epidemiology" => "medical.epidemiology".into(),
''' + canonical_anchor
aliases = replace_once(aliases, canonical_anchor, canonical_insert, "canonical aliases")

storage_anchor = '''        "medical.somatic_status" => &[
            "medical.somatic_status",
'''
storage_insert = '''        "medical.profile_observation" => &[
            "medical.profile_observation",
            "profile_observation",
            "psych_account",
            "medical.psych_account",
        ],
        "medical.rvk_referral" => &["medical.rvk_referral", "rvk_referral"],
        "medical.epidemiology" => &["medical.epidemiology", "epidemiology"],
''' + storage_anchor
aliases = replace_once(aliases, storage_anchor, storage_insert, "storage aliases")
ALIASES.write_text(aliases, encoding="utf-8")

domain = DOMAIN.read_text(encoding="utf-8")
domain_anchor = '''            FieldExtractionRule {
                field_id: "medical.somatic_status".into(),
                aliases: vec!["Соматический статус".into(), "Объективный статус".into()],
'''
domain_insert = '''            FieldExtractionRule {
                field_id: "medical.epidemiology".into(),
                aliases: vec![
                    "Эпидемиологический анамнез".into(),
                    "Wywiad epidemiologiczny".into(),
                ],
                required: false,
            },
            FieldExtractionRule {
                field_id: "medical.profile_observation".into(),
                aliases: vec![
                    "Профильное наблюдение".into(),
                    "Диспансерное наблюдение".into(),
                    "На учёте у психиатров".into(),
                    "На учете у психиатров".into(),
                ],
                required: false,
            },
            FieldExtractionRule {
                field_id: "medical.disability".into(),
                aliases: vec!["Оформление инвалидности".into(), "Инвалидность".into()],
                required: false,
            },
            FieldExtractionRule {
                field_id: "medical.rvk_referral".into(),
                aliases: vec!["Направление от РВК".into(), "Направление РВК".into()],
                required: false,
            },
''' + domain_anchor
domain = replace_once(domain, domain_anchor, domain_insert, "domain medical profile facts")
DOMAIN.write_text(domain, encoding="utf-8")

inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
donor = next(item for item in inventory["donors"] if item["repository"] == "mailsvb2-bot/Dokkomplekt")
entries = donor["entries"]
existing = {entry["path"] for entry in entries}
new_entries = [
    {
        "path": "medical_parser.py",
        "status": "migrated-domain-profile",
        "targets": [
            "crates/dokkomplekt-core/src/source_parser.rs",
            "crates/dokkomplekt-core/src/field_aliases.rs",
            "crates/dokkomplekt-core/tests/donor_medical_profile_facts.rs",
        ],
        "note": "Donor profile observation, disability, epidemiology and explicit RVK-referral labels are represented in canonical Rust. Bare `РВК` is intentionally excluded because Universal already uses it for commissariat requisites.",
    },
    {
        "path": "medical_models.py",
        "status": "migrated-domain-profile",
        "targets": [
            "crates/dokkomplekt-core/src/field_registry.rs",
            "crates/dokkomplekt-core/src/domains/medical.rs",
            "crates/dokkomplekt-core/tests/donor_medical_profile_facts.rs",
        ],
        "note": "Donor PatientData profile facts are scoped to the Medical domain. Generic bare `disability` remains profession-neutral rather than becoming a global medical alias.",
    },
]
for entry in new_entries:
    if entry["path"] not in existing:
        entries.append(entry)
        existing.add(entry["path"])
entries.sort(key=lambda item: item["path"].lower())
INVENTORY.write_text(json.dumps(inventory, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
