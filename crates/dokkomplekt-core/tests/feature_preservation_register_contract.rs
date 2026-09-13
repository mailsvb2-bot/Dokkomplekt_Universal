use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

#[test]
fn canon_v2_feature_preservation_register_is_complete_and_honest() {
    let root = repository_root();
    let register_path = root.join("docs/CANON_FEATURE_PRESERVATION_REGISTER.json");
    let raw = fs::read_to_string(&register_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", register_path.display()));
    let register: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid feature preservation register JSON: {error}"));

    assert_eq!(register["schema_version"], 1);
    assert_eq!(register["canon"]["revision"], "2.0");
    assert_eq!(register["baseline"]["stage"], "E0");
    assert_eq!(register["baseline"]["generation_mechanics_changed"], false);

    let baseline_commit = register["baseline"]["commit"]
        .as_str()
        .expect("baseline.commit must be a string");
    assert_eq!(baseline_commit.len(), 40, "baseline.commit must be a full SHA-1");
    assert!(
        baseline_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "baseline.commit must contain only hexadecimal characters"
    );

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
    assert!(allowed_statuses.contains("needs-runtime-proof"));
    assert!(allowed_statuses.contains("known-defect"));
    assert!(allowed_statuses.contains("verified"));

    let features = register["features"]
        .as_array()
        .expect("features must be an array");
    assert_eq!(features.len(), expected.len(), "Canon §3 feature groups changed");

    let mut seen_ids = BTreeSet::new();
    let mut seen_groups = BTreeSet::new();
    let mut all_verified = true;

    for feature in features {
        let id = feature["id"].as_str().expect("feature.id must be a string");
        let group = feature["group"]
            .as_str()
            .expect("feature.group must be a string");
        let status = feature["status"]
            .as_str()
            .expect("feature.status must be a string");

        assert!(seen_ids.insert(id), "duplicate feature id: {id}");
        assert!(seen_groups.insert(group), "duplicate feature group: {group}");
        assert_eq!(expected.get(id).copied(), Some(group), "unexpected Canon feature");
        assert!(allowed_statuses.contains(status), "unsupported status for {id}: {status}");

        let actual_state = feature["actual_state"]
            .as_str()
            .expect("actual_state must be a string");
        assert!(!actual_state.trim().is_empty(), "actual_state is empty for {id}");

        non_empty_array(feature, "entry");
        non_empty_array(feature, "backend");
        non_empty_array(feature, "storage");
        non_empty_array(feature, "result");

        for profile_key in ["before", "after"] {
            let profile = feature[profile_key]
                .as_str()
                .unwrap_or_else(|| panic!("{profile_key} must be a string for {id}"));
            assert!(
                check_profiles.contains_key(profile),
                "unknown check profile `{profile}` for {id}"
            );
        }

        for evidence in non_empty_array(feature, "evidence_paths") {
            let relative = evidence
                .as_str()
                .unwrap_or_else(|| panic!("evidence path must be a string for {id}"));
            let path = root.join(relative);
            assert!(path.exists(), "evidence path for {id} does not exist: {relative}");
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
        }
    }

    let declared_complete = register["baseline"]["e0_complete"]
        .as_bool()
        .expect("baseline.e0_complete must be a boolean");
    assert_eq!(
        declared_complete, all_verified,
        "E0 completion must exactly match runtime-backed verification of every Canon §3 feature"
    );
}
