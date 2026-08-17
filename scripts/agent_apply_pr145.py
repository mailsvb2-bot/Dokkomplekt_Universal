from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEMANTICS = ROOT / "crates/dokkomplekt-core/src/domains/medical_semantics.rs"
REGISTRY = ROOT / "crates/dokkomplekt-core/src/field_registry.rs"
ALIASES = ROOT / "crates/dokkomplekt-core/src/field_aliases.rs"
PLAN = ROOT / "crates/dokkomplekt-core/src/domains/medical_document_plan.rs"
COMMANDS = ROOT / "src-tauri/src/subsystems/document_commands.rs"
INVENTORY = ROOT / "docs/LEGACY_MIGRATION_INVENTORY.json"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


semantics = SEMANTICS.read_text(encoding="utf-8")
semantics = replace_once(
    semantics,
    "use crate::SemanticCase;\n",
    "use crate::{SemanticCase, SemanticValue, ValueSource};\nuse chrono::{Duration, NaiveDate};\n",
    "medical semantics imports",
)
semantics = replace_once(
    semantics,
    'pub const SICK_LEAVE_VK_POSITION: &str = "medical.sick_leave_vk.position";\n',
    'pub const SICK_LEAVE_VK_POSITION: &str = "medical.sick_leave_vk.position";\n\npub const MEDICAL_EXPERT_ANAMNESIS: &str = "medical.expert_anamnesis";\npub const MEDICAL_SICK_LEAVE_NEEDED: &str = "medical.sick_leave_needed";\n',
    "expert constants",
)
old_render_tail = '''    for (scoped_id, legacy_id) in role_scoped_bindings(role_id) {
        if let Some(mut value) = case.values.get(*scoped_id).cloned() {
            value.field_id = (*legacy_id).to_string();
            scoped_case.values.insert((*legacy_id).to_string(), value);
        }
        if case.skipped_fields.contains(*scoped_id) {
            scoped_case.skipped_fields.insert((*legacy_id).to_string());
        }
    }
    scoped_case
}

pub fn title_for_role_scoped_field'''
new_render_tail = '''    for (scoped_id, legacy_id) in role_scoped_bindings(role_id) {
        if let Some(mut value) = case.values.get(*scoped_id).cloned() {
            value.field_id = (*legacy_id).to_string();
            scoped_case.values.insert((*legacy_id).to_string(), value);
        }
        if case.skipped_fields.contains(*scoped_id) {
            scoped_case.skipped_fields.insert((*legacy_id).to_string());
        }
    }

    let canonical_role = crate::domains::medical::canonical_medical_role(role_id);
    if matches!(canonical_role.as_str(), "primary" | "discharge") {
        // Never reuse a stale expert paragraph from the source document. Build an
        // ephemeral render-only value from the current case and the current role.
        scoped_case.values.remove(MEDICAL_EXPERT_ANAMNESIS);
        scoped_case.skipped_fields.remove(MEDICAL_EXPERT_ANAMNESIS);
        if let Some(expert) = build_expert_anamnesis(&scoped_case, &canonical_role) {
            scoped_case.values.insert(
                MEDICAL_EXPERT_ANAMNESIS.to_string(),
                SemanticValue::new(
                    MEDICAL_EXPERT_ANAMNESIS,
                    expert,
                    ValueSource::SafeDefault,
                    1.0,
                ),
            );
        }
    }
    scoped_case
}

pub fn set_medical_sick_leave_choice(case: &mut SemanticCase, enabled: bool) {
    let value = if enabled { "Да" } else { "Нет" };
    case.values.insert(
        MEDICAL_SICK_LEAVE_NEEDED.to_string(),
        SemanticValue::new(
            MEDICAL_SICK_LEAVE_NEEDED,
            value,
            ValueSource::UserConfirmed,
            1.0,
        ),
    );
    case.skipped_fields.remove(MEDICAL_SICK_LEAVE_NEEDED);
}

fn build_expert_anamnesis(case: &SemanticCase, role_id: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(work) = expert_work_sentence(case) {
        parts.push(work);
    }

    if role_id == "discharge" {
        let sick_needed = case
            .get(MEDICAL_SICK_LEAVE_NEEDED)
            .and_then(normalize_yes_no)
            .or_else(|| case.get("medical.sick_leave_number").map(|_| true));
        match sick_needed {
            Some(true) => parts.push(discharge_sick_leave_sentence(case)),
            Some(false) => parts.push("В выдаче ЛН не нуждается.".to_string()),
            None => {}
        }
    }

    (!parts.is_empty()).then(|| parts.join(" "))
}

fn expert_work_sentence(case: &SemanticCase) -> Option<String> {
    let workplace = case
        .get("medical.workplace")
        .or_else(|| case.get("subject.organization"))
        .map(clean_expert_component)
        .filter(|value| !value.is_empty());
    let position = case
        .get("medical.position")
        .or_else(|| case.get("subject.position"))
        .map(clean_expert_component)
        .filter(|value| !value.is_empty());
    match (workplace, position) {
        (Some(workplace), Some(position)) => Some(format!(
            "Работает в {workplace}, в должности {position}."
        )),
        (Some(workplace), None) => Some(format!("Работает в {workplace}.")),
        (None, Some(position)) => Some(format!("Работает, должность: {position}.")),
        (None, None) => None,
    }
}

fn discharge_sick_leave_sentence(case: &SemanticCase) -> String {
    let number = case
        .get("medical.sick_leave_number")
        .map(clean_expert_component)
        .filter(|value| !value.is_empty());
    let mut line = match number {
        Some(number) => format!("Больничный лист № {number}."),
        None => "Больничный лист.".to_string(),
    };

    let start = case
        .get("medical.admission_date")
        .or_else(|| case.get("medical.sick_leave_from"))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let finish = case
        .get("medical.discharge_date")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let (Some(start), Some(finish)) = (start, finish) {
        if let (Some(start_date), Some(finish_date)) =
            (parse_medical_date(start), parse_medical_date(finish))
        {
            if finish_date >= start_date {
                let days = (finish_date - start_date).num_days() + 1;
                line.push_str(&format!(
                    " Срок лечения с {start} по {finish}, {days} {}.",
                    russian_day_word(days)
                ));
            } else {
                line.push_str(&format!(" Срок лечения с {start} по {finish}."));
            }
        } else {
            line.push_str(&format!(" Срок лечения с {start} по {finish}."));
        }
    } else if let Some(start) = case
        .get("medical.sick_leave_from")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        line.push_str(&format!(" Больничный лист открыт с {start}."));
    }

    if let Some(finish_date) = finish.and_then(parse_medical_date) {
        let return_to_work = finish_date + Duration::days(1);
        line.push_str(&format!(
            " К труду с {}.",
            return_to_work.format("%d.%m.%Y")
        ));
    }
    line
}

fn clean_expert_component(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(['.', ',', ';', ':'])
        .trim()
        .to_string()
}

fn parse_medical_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%d.%m.%Y").ok()
}

fn normalize_yes_no(value: &str) -> Option<bool> {
    let normalized = value.trim().to_lowercase().replace('ё', "е");
    match normalized.as_str() {
        "да" | "д" | "yes" | "y" | "1" | "+" | "нужен" | "нужна" | "нужно" => {
            Some(true)
        }
        "нет" | "н" | "no" | "n" | "0" | "-" | "не нужен" | "не нужна" | "не нужно" => {
            Some(false)
        }
        _ => None,
    }
}

fn russian_day_word(days: i64) -> &'static str {
    let last_two = days.rem_euclid(100);
    if (11..=14).contains(&last_two) {
        return "дней";
    }
    match days.rem_euclid(10) {
        1 => "день",
        2..=4 => "дня",
        _ => "дней",
    }
}

pub fn title_for_role_scoped_field'''
semantics = replace_once(semantics, old_render_tail, new_render_tail, "expert render semantics")
SEMANTICS.write_text(semantics, encoding="utf-8")

registry = REGISTRY.read_text(encoding="utf-8")
registry_anchor = '''        field(
            "medical.attending_doctor",
            "Лечащий врач",
'''
registry_insert = '''        field(
            "medical.sick_leave_needed",
            "Нужен больничный лист",
            DomainKind::Medical,
            false,
            &[
                "expert.sick_leave_needed",
                "expert_sick_leave_needed",
                "Нужен больничный",
                "Больничный лист нужен",
            ],
        ),
        field(
            "medical.expert_anamnesis",
            "Экспертный анамнез",
            DomainKind::Medical,
            false,
            &[
                "expert.anamnesis",
                "expert_anamnesis",
                "expertAnamnesis",
                "Экспертный анамнез",
            ],
        ),
''' + registry_anchor
registry = replace_once(registry, registry_anchor, registry_insert, "expert registry fields")
REGISTRY.write_text(registry, encoding="utf-8")

aliases = ALIASES.read_text(encoding="utf-8")
canonical_anchor = '''        "status.objective" | "status.somatic" | "somatic_status" => "medical.somatic_status".into(),
'''
canonical_insert = '''        "expert_work_org" | "expert.work_org" => "medical.workplace".into(),
        "expert_position" | "expert.position" => "medical.position".into(),
        "expert_sick_leave_number" | "expert.sick_leave_number" => {
            "medical.sick_leave_number".into()
        }
        "expert_sick_leave_from" | "expert.sick_leave_from" => "medical.sick_leave_from".into(),
        "expert_sick_leave_needed" | "expert.sick_leave_needed" => {
            "medical.sick_leave_needed".into()
        }
        "expert_anamnesis" | "expert.anamnesis" => "medical.expert_anamnesis".into(),
''' + canonical_anchor
aliases = replace_once(aliases, canonical_anchor, canonical_insert, "expert canonical aliases")
storage_anchor = '''        "medical.somatic_status" => &[
            "medical.somatic_status",
'''
storage_insert = '''        "medical.workplace" => &["medical.workplace", "expert_work_org", "expert.work_org"],
        "medical.position" => &["medical.position", "expert_position", "expert.position"],
        "medical.sick_leave_number" => &[
            "medical.sick_leave_number",
            "expert_sick_leave_number",
            "expert.sick_leave_number",
        ],
        "medical.sick_leave_from" => &[
            "medical.sick_leave_from",
            "expert_sick_leave_from",
            "expert.sick_leave_from",
        ],
        "medical.sick_leave_needed" => &[
            "medical.sick_leave_needed",
            "expert_sick_leave_needed",
            "expert.sick_leave_needed",
        ],
        "medical.expert_anamnesis" => &[
            "medical.expert_anamnesis",
            "expert_anamnesis",
            "expert.anamnesis",
        ],
''' + storage_anchor
aliases = replace_once(aliases, storage_anchor, storage_insert, "expert storage aliases")
ALIASES.write_text(aliases, encoding="utf-8")

plan = PLAN.read_text(encoding="utf-8")
work_optional = '''            optional.extend(["medical.workplace".into(), "medical.position".into()]);
'''
if plan.count(work_optional) != 2:
    raise SystemExit(f"expert plan work fields: expected 2 optional occurrences, got {plan.count(work_optional)}")
plan = plan.replace(
    work_optional,
    '''            required.extend(["medical.workplace".into(), "medical.position".into()]);
''',
)
PLAN.write_text(plan, encoding="utf-8")

commands = COMMANDS.read_text(encoding="utf-8")
apply_block = '''        if result.accepted {
            snapshot.semantic_case = result.semantic_case.clone();
        }
'''
if commands.count(apply_block) != 2:
    raise SystemExit(f"popup persistence: expected 2 apply blocks, got {commands.count(apply_block)}")
single = '''        if result.accepted {
            snapshot.semantic_case = result.semantic_case.clone();
            if doc.category == dokkomplekt_core::DomainKind::Medical {
                dokkomplekt_core::domains::medical_semantics::set_medical_sick_leave_choice(
                    &mut snapshot.semantic_case,
                    req.sick_leave_enabled,
                );
            }
        }
'''
commands = commands.replace(apply_block, single, 1)
batch = '''        if result.accepted {
            snapshot.semantic_case = result.semantic_case.clone();
            if documents
                .iter()
                .any(|document| document.category == dokkomplekt_core::DomainKind::Medical)
            {
                dokkomplekt_core::domains::medical_semantics::set_medical_sick_leave_choice(
                    &mut snapshot.semantic_case,
                    req.sick_leave_enabled,
                );
            }
        }
'''
commands = commands.replace(apply_block, batch, 1)
COMMANDS.write_text(commands, encoding="utf-8")

inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
donor = next(item for item in inventory["donors"] if item["repository"] == "mailsvb2-bot/Dokkomplekt")
entries = donor["entries"]
if not any(entry["path"] == "medical_expert.py" for entry in entries):
    entries.append(
        {
            "path": "medical_expert.py",
            "status": "migrated-domain-profile",
            "targets": [
                "crates/dokkomplekt-core/src/domains/medical_semantics.rs",
                "crates/dokkomplekt-core/src/domains/medical_document_plan.rs",
                "crates/dokkomplekt-core/tests/donor_medical_expert_anamnesis.rs",
                "src-tauri/src/subsystems/document_commands.rs",
            ],
            "note": "Expert anamnesis is derived only in the Medical render case: primary stays short, discharge includes the confirmed sick-leave decision, number, inclusive treatment period and return-to-work date. The UI sick-leave toggle is persisted as a semantic fact instead of being inferred from missing data.",
        }
    )
entries.sort(key=lambda item: item["path"].lower())
INVENTORY.write_text(json.dumps(inventory, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
