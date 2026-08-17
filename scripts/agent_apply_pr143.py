from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/dokkomplekt-core/src/source_parser.rs"
TEST = ROOT / "crates/dokkomplekt-core/tests/donor_medical_template_noise.rs"
INVENTORY = ROOT / "docs/LEGACY_MIGRATION_INVENTORY.json"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


source = SOURCE.read_text(encoding="utf-8")
source = replace_once(
    source,
    '''    if field == "medical.diagnosis" {\n        return sanitize_medical_diagnosis(value);\n    }\n''',
    '''    if field == "medical.diagnosis" {\n        let cleaned = sanitize_medical_source_value(value)?;\n        return sanitize_medical_diagnosis(&cleaned);\n    }\n    if field.starts_with("medical.")\n        && field != "medical.case_number"\n        && !field.ends_with(".date")\n        && !field.ends_with("_date")\n    {\n        return sanitize_medical_source_value(value);\n    }\n''',
    "normalize medical source value",
)

source = replace_once(
    source,
    '''    for warning in engine_report.warnings {\n        if !report.warnings.contains(&warning) {\n            report.warnings.push(warning);\n        }\n    }\n\n    // Multiple deterministic extractors may identify the same person name with\n''',
    '''    for warning in engine_report.warnings {\n        if !report.warnings.contains(&warning) {\n            report.warnings.push(warning);\n        }\n    }\n\n    // A higher-confidence generic extractor must never be able to restore a\n    // donor template instruction that the medical parser already rejected.\n    // Apply the narrow donor sanitizer once more to the final canonical medical\n    // values after all deterministic extractors have merged.\n    if medical {\n        sanitize_final_medical_values(&mut case, &mut report);\n    }\n\n    // Multiple deterministic extractors may identify the same person name with\n''',
    "post-merge medical sanitizer",
)

helper_anchor = '''fn sanitize_medical_diagnosis(value: &str) -> Option<String> {\n'''
helpers = r'''fn sanitize_medical_source_value(value: &str) -> Option<String> {
    let mut lines = Vec::new();
    for raw_line in value.lines() {
        let mut line = clean_value(raw_line);
        if line.is_empty() || is_medical_template_choice_placeholder(&line) {
            continue;
        }
        if let Some(start) = medical_service_marker_start(&line) {
            line = line[..start]
                .trim()
                .trim_end_matches(['-', '—', '–', ':', ';', ',', '.'])
                .trim()
                .to_string();
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    let cleaned = lines.join("\n");
    (!cleaned.is_empty()).then_some(cleaned)
}

fn is_medical_template_choice_placeholder(value: &str) -> bool {
    let normalized = value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    matches!(
        normalized.as_str(),
        "нужно / не нужно" | "нужен / не нужен" | "состоит / не состоит" | "да / нет"
    )
}

fn medical_service_marker_start(value: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    for marker in ["сюда подставлять", "сюда подставляется", "выбирается в ui"] {
        let Some(marker_end) = find_label_end(value, marker) else {
            continue;
        };
        let Some(marker_start) = label_start_from_end(value, marker, marker_end) else {
            continue;
        };
        best = Some(best.map_or(marker_start, |current| current.min(marker_start)));
    }
    best
}

fn sanitize_final_medical_values(case: &mut SemanticCase, report: &mut ParsedSourceReport) {
    let fields = case
        .values
        .keys()
        .filter(|field| {
            (field.starts_with("medical.")
                && field.as_str() != "medical.case_number"
                && !field.ends_with(".date")
                && !field.ends_with("_date"))
                || matches!(field.as_str(), "classification.primary" | "action.plan")
        })
        .cloned()
        .collect::<Vec<_>>();

    for field in fields {
        let Some(current) = case.get(&field).map(str::to_owned) else {
            continue;
        };
        let sanitized = sanitize_medical_source_value(&current).and_then(|value| {
            if matches!(field.as_str(), "medical.diagnosis" | "classification.primary") {
                sanitize_medical_diagnosis(&value)
            } else {
                Some(value)
            }
        });
        match sanitized {
            Some(cleaned) => {
                if cleaned != current {
                    if let Some(value) = case.values.get_mut(&field) {
                        value.value = cleaned;
                    }
                }
            }
            None => {
                case.values.remove(&field);
                report.filled_fields.retain(|item| item != &field);
                let warning = format!(
                    "Поле «{field}» не принято: обнаружена служебная инструкция шаблона"
                );
                if !report.warnings.contains(&warning) {
                    report.warnings.push(warning);
                }
            }
        }
    }
}

'''
source = replace_once(source, helper_anchor, helpers + helper_anchor, "medical sanitizer helpers")
SOURCE.write_text(source, encoding="utf-8")

# Tighten the regression contract to include generic canonical mirrors as well.
test = TEST.read_text(encoding="utf-8")ntest = test.replace(
    '''    assert_eq!(case.get("medical.icd10"), Some("F20.0"));\n''',
    '''    assert_eq!(\n        case.get("classification.primary"),\n        Some("F20.0 Параноидная шизофрения")\n    );\n    assert_eq!(case.get("medical.icd10"), Some("F20.0"));\n''',
    1,
)
test = test.replace(
    '''    assert_eq!(case.get("medical.treatment"), None);\n''',
    '''    assert_eq!(case.get("medical.treatment"), None);\n    assert_eq!(case.get("action.plan"), None);\n''',
    1,
)
test = test.replace(
    '''    assert_eq!(\n        case.get("medical.treatment"),\n        Some("Нужно продолжить приём рисперидона 4 мг/сут")\n    );\n''',
    '''    assert_eq!(\n        case.get("medical.treatment"),\n        Some("Нужно продолжить приём рисперидона 4 мг/сут")\n    );\n    assert_eq!(\n        case.get("action.plan"),\n        Some("Нужно продолжить приём рисперидона 4 мг/сут")\n    );\n''',
    1,
)
test = test.replace(
    '''    assert_eq!(case.get("medical.recommendations"), None);\n''',
    '''    assert_eq!(case.get("medical.recommendations"), None);\n    assert_eq!(case.get("action.plan"), None);\n''',
    1,
)
TEST.write_text(test, encoding="utf-8")

inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
donor = next(
    item for item in inventory["donors"] if item["repository"] == "mailsvb2-bot/Dokkomplekt"
)
entries = donor["entries"]
existing = {entry["path"] for entry in entries}
new_entries = [
    {
        "path": "medical_parser_blocks.py",
        "status": "migrated-domain-profile",
        "targets": [
            "crates/dokkomplekt-core/src/source_parser.rs",
            "crates/dokkomplekt-core/tests/donor_medical_template_noise.rs",
        ],
        "note": "Donor medical section-boundary behavior is represented by canonical Rust label boundaries and regression tests; no parallel Python parser is shipped.",
    },
    {
        "path": "medical_parser_sanitize.py",
        "status": "migrated-domain-profile",
        "targets": [
            "crates/dokkomplekt-core/src/source_parser.rs",
            "crates/dokkomplekt-core/tests/donor_medical_template_noise.rs",
        ],
        "note": "Template/service instructions are removed from medical source values while legitimate clinician text is preserved.",
    },
    {
        "path": "medical_text_utils.py",
        "status": "migrated-domain-profile",
        "targets": [
            "crates/dokkomplekt-core/src/source_parser.rs",
            "crates/dokkomplekt-core/tests/donor_medical_template_noise.rs",
        ],
        "note": "Donor service markers and exact option placeholders are sanitized in the canonical Rust source parser.",
    },
]
for entry in new_entries:
    if entry["path"] not in existing:
        insert_at = next(
            (i for i, current in enumerate(entries) if current["path"] == "medical_renderer_labs.py"),
            len(entries),
        )
        entries.insert(insert_at, entry)
        existing.add(entry["path"])

INVENTORY.write_text(json.dumps(inventory, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
