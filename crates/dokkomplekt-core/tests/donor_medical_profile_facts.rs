use dokkomplekt_core::{canonical_storage_field_id, medical_fields, parse_source_text};

#[test]
fn donor_profile_observation_disability_and_rvk_referral_are_preserved() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения\nНа учёте у психиатров: состоит с 2021 года\nИнвалидность: II группа бессрочно\nНаправление от РВК: Нижегородского района";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(
        case.get("medical.profile_observation"),
        Some("состоит с 2021 года")
    );
    assert_eq!(case.get("medical.disability"), Some("II группа бессрочно"));
    assert_eq!(
        case.get("medical.rvk_referral"),
        Some("Нижегородского района")
    );
}

#[test]
fn donor_epidemiology_is_a_separate_multiline_medical_section() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения\nЭпидемиологический анамнез:\nКонтакты с инфекционными больными отрицает.\nПсихический статус: контактен, ориентирован";
    let (case, _) = parse_source_text(text, 2026);

    let epidemiology = case
        .get("medical.epidemiology")
        .expect("epidemiology must be extracted");
    assert!(epidemiology.contains("Контакты с инфекционными больными отрицает"));
    assert_eq!(
        case.get("medical.profile_status"),
        Some("контактен, ориентирован")
    );
    assert!(!epidemiology.contains("Психический статус"));
}

#[test]
fn donor_choice_placeholder_is_not_profile_observation_data() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения\nНа учёте у психиатров: состоит / не состоит";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(case.get("medical.profile_observation"), None);
}

#[test]
fn bare_rvk_label_is_not_misclassified_as_a_referral() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения\nРВК: Автозаводский";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(case.get("medical.rvk_referral"), None);
}

#[test]
fn donor_legacy_field_ids_resolve_without_medicalizing_generic_disability() {
    assert_eq!(
        canonical_storage_field_id("psych_account"),
        "medical.profile_observation"
    );
    assert_eq!(
        canonical_storage_field_id("profile_observation"),
        "medical.profile_observation"
    );
    assert_eq!(canonical_storage_field_id("disability"), "disability");
    assert_eq!(
        canonical_storage_field_id("rvk_referral"),
        "medical.rvk_referral"
    );
    assert_eq!(
        canonical_storage_field_id("epidemiology"),
        "medical.epidemiology"
    );
}

#[test]
fn donor_profile_fact_fields_are_in_the_medical_registry() {
    let fields = medical_fields();
    for field_id in [
        "medical.profile_observation",
        "medical.disability",
        "medical.rvk_referral",
        "medical.epidemiology",
    ] {
        assert!(
            fields.iter().any(|field| field.id == field_id),
            "missing medical field {field_id}"
        );
    }

    let rvk = fields
        .iter()
        .find(|field| field.id == "medical.rvk_referral")
        .expect("RVK referral field must exist");
    assert!(!rvk.aliases.iter().any(|alias| alias == "РВК"));
}
