use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Value {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid JSON in {}: {error}", path.display()))
}

fn non_empty_array<'a>(feature: &'a Value, key: &str) -> &'a Vec<Value> {
    let value = feature
        .get(key)
        .unwrap_or_else(|| panic!("feature is missing `{key}`"));
    let array = value
        .as_array()
        .unwrap_or_else(|| panic!("feature field `{key}` must be an array"));
    assert!(!array.is_empty(), "feature field `{key}` must not be empty");
    array
}

fn non_empty_string<'a>(value: &'a Value, key: &str, context: &str) -> &'a str {
    let text = value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{context}.{key} must be a string"));
    assert!(!text.trim().is_empty(), "{context}.{key} must not be empty");
    text
}

#[test]
fn canon_v2_feature_preservation_register_is_complete_and_honest() {
    let root = repository_root();
    let register = read_json(&root.join("docs/CANON_FEATURE_PRESERVATION_REGISTER.json"));
    let baseline = read_json(&root.join("verification/canon-v2/E0_BASELINE.json"));

    assert_eq!(register["schema_version"], 2);
    assert_eq!(register["canon"]["revision"], "2.0");
    assert_eq!(register["baseline"]["stage"], "E0");
    assert_eq!(register["baseline"]["generation_mechanics_changed"], false);
    assert_eq!(register["baseline"]["e0_complete"], true);
    assert_eq!(
        register["baseline"]["evidence_record"],
        "verification/canon-v2/E0_BASELINE.json"
    );

    let baseline_commit = non_empty_string(&register["baseline"], "commit", "baseline");
    assert_eq!(
        baseline_commit.len(),
        40,
        "baseline.commit must be a full SHA-1"
    );
    assert!(
        baseline_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "baseline.commit must contain only hexadecimal characters"
    );
    assert_eq!(baseline["source_baseline"]["commit"], baseline_commit);
    assert_eq!(baseline["e0_exit"]["complete"], true);
    assert_eq!(baseline["e0_exit"]["generation_mechanics_changed"], false);
    assert_eq!(baseline["e0_exit"]["feature_groups_inventoried"], 23);
    assert_eq!(
        baseline["resource_baseline"]["runtime_measurements"]["status"],
        "deferred-to-E6"
    );

    for required_path in [
        "verification/KNOWN_LIMITATIONS.md",
        "tests/fixtures/docx/corpus-manifest.json",
        "tests/test_canon_u00_positioning_contract.py",
    ] {
        assert!(
            root.join(required_path).exists(),
            "missing E0 evidence path: {required_path}"
        );
    }

    let expected: BTreeMap<&str, &str> = [
        ("FPR-01", "Основные документы"),
        ("FPR-02", "Дневники"),
        ("FPR-03", "Входные файлы"),
        ("FPR-04", "Распознавание"),
        ("FPR-05", "МКБ-10"),
        ("FPR-06", "Scanner"),
        ("FPR-07", "Preflight"),
        ("FPR-08", "Кнопки"),
        ("FPR-09", "Обучение"),
        ("FPR-10", "Автоматизация"),
        ("FPR-11", "Папки"),
        ("FPR-12", "Настройки"),
        ("FPR-13", "Комплекты"),
        ("FPR-14", "Версии"),
        ("FPR-15", "Перенос шаблонов"),
        ("FPR-16", "Просмотр"),
        ("FPR-17", "Печать и конвертация"),
        ("FPR-18", "Лицензирование"),
        ("FPR-19", "Обновления"),
        ("FPR-20", "Runtime и упаковка"),
        ("FPR-21", "Domain plugins"),
        ("FPR-22", "Audit и privacy"),
        ("FPR-23", "Инфраструктура"),
    ]
    .into_iter()
    .collect();

    let check_profiles = register["check_profiles"]
        .as_object()
        .expect("check_profiles must be an object");
    let allowed_statuses: BTreeSet<&str> = register["allowed_statuses"]
        .as_array()
        .expect("allowed_statuses must be an array")
        .iter()
        .map(|value| value.as_str().expect("status must be a string"))
        .collect();
    for required in ["needs-runtime-proof", "known-defect", "verified"] {
        assert!(
            allowed_statuses.contains(required),
            "missing allowed status: {required}"
        );
    }

    let features = register["features"]
        .as_array()
        .expect("features must be an array");
    assert_eq!(
        features.len(),
        expected.len(),
        "Canon §3 feature groups changed"
    );

    let mut seen_ids = BTreeSet::new();
    let mut seen_groups = BTreeSet::new();
    let mut all_verified = true;

    for feature in features {
        let id = non_empty_string(feature, "id", "feature");
        let group = non_empty_string(feature, "group", id);
        let status = non_empty_string(feature, "status", id);

        assert!(seen_ids.insert(id), "duplicate feature id: {id}");
        assert!(
            seen_groups.insert(group),
            "duplicate feature group: {group}"
        );
        assert_eq!(
            expected.get(id).copied(),
            Some(group),
            "unexpected Canon feature"
        );
        assert!(
            allowed_statuses.contains(status),
            "unsupported status for {id}: {status}"
        );
        non_empty_string(feature, "actual_state", id);

        non_empty_array(feature, "entry");
        non_empty_array(feature, "backend");
        non_empty_array(feature, "storage");
        non_empty_array(feature, "result");

        for profile_key in ["before", "after"] {
            let profile = non_empty_string(feature, profile_key, id);
            assert!(
                check_profiles.contains_key(profile),
                "unknown check profile `{profile}` for {id}"
            );
        }

        for evidence in non_empty_array(feature, "evidence_paths") {
            let relative = evidence
                .as_str()
                .unwrap_or_else(|| panic!("evidence path must be a string for {id}"));
            assert!(
                root.join(relative).exists(),
                "evidence path for {id} does not exist: {relative}"
            );
        }

        let runtime_evidence = feature["runtime_evidence"]
            .as_array()
            .unwrap_or_else(|| panic!("runtime_evidence must be an array for {id}"));
        if status == "verified" {
            assert!(
                !runtime_evidence.is_empty(),
                "{id} cannot be verified from source inventory alone"
            );
        } else {
            all_verified = false;
            non_empty_string(feature, "runtime_gap", id);
        }
    }

    let final_register_closed = register["final_register_closed"]
        .as_bool()
        .expect("final_register_closed must be a boolean");
    assert_eq!(
        final_register_closed, all_verified,
        "final register closure, not E0 completion, must match runtime-backed verification of every Canon §3 feature"
    );
}
