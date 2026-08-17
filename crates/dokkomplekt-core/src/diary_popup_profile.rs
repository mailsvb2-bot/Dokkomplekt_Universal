//! Medical diary popup façade over the universal popup profile engine.
//!
//! The generic popup machinery stays profession-neutral. This module only adds the
//! donor-compatible diary decisions that are mandatory for the medical diary role.

use crate::{
    popup_profiles, DomainKind, DocumentTemplateSpec, MedicalDocumentRole, PopupFieldConfig,
    PromptAskMode, PromptInputKind, DEFAULT_TREATMENT_CORRECTION,
};
use std::collections::BTreeSet;

pub const DIARY_SICK_LEAVE_EPICRISIS: &str = "medical.diary_sick_leave_epicrisis";
pub const DIARY_TREATMENT_CORRECTION: &str = "medical.diary_treatment_correction";

fn is_medical_diary(category: &DomainKind, role_id: &str) -> bool {
    matches!(category, DomainKind::Medical)
        && matches!(
            MedicalDocumentRole::from_role_id(role_id),
            MedicalDocumentRole::Diary
        )
}

fn donor_sick_leave_config() -> PopupFieldConfig {
    let mut config = PopupFieldConfig::new(
        DIARY_SICK_LEAVE_EPICRISIS,
        "Лечится по больничному листу?",
    );
    config.required = true;
    config.input_kind = PromptInputKind::YesNo;
    config.ask_mode = PromptAskMode::Always;
    config.options = vec!["Нет".into(), "Да".into()];
    config.section = Some("Медицинские данные".into());
    config.help_text = Some(
        "Если да — программа будет писать динамический эпикриз 1 раз в 10 дней.".into(),
    );
    config.order = 230;
    config
}

fn donor_treatment_correction_config() -> PopupFieldConfig {
    let mut config = PopupFieldConfig::new(DIARY_TREATMENT_CORRECTION, "Коррекция лечения");
    config.input_kind = PromptInputKind::LongText;
    config.ask_mode = PromptAskMode::Confirm;
    config.default_value = Some(DEFAULT_TREATMENT_CORRECTION.into());
    config.linked_to = Some(DIARY_SICK_LEAVE_EPICRISIS.into());
    config.section = Some("Медицинские данные".into());
    config.help_text = Some(
        "Введите коррекцию лечения. Если оставить пустым, будет: лекарства принимает согласно назначениям."
            .into(),
    );
    config.order = 240;
    config
}

fn extend_diary_fields(
    mut fields: Vec<PopupFieldConfig>,
    category: &DomainKind,
    role_id: &str,
) -> Vec<PopupFieldConfig> {
    if !is_medical_diary(category, role_id) {
        return fields;
    }
    for required in [donor_sick_leave_config(), donor_treatment_correction_config()] {
        if let Some(existing) = fields
            .iter_mut()
            .find(|field| field.field_id == required.field_id)
        {
            *existing = required;
        } else {
            fields.push(required);
        }
    }
    fields.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then(left.field_id.cmp(&right.field_id))
    });
    fields
}

pub fn default_popup_fields_for_document(document: &DocumentTemplateSpec) -> Vec<PopupFieldConfig> {
    extend_diary_fields(
        popup_profiles::default_popup_fields_for_document(document),
        &document.category,
        &document.role_id,
    )
}

pub fn effective_popup_fields(document: &DocumentTemplateSpec) -> Vec<PopupFieldConfig> {
    extend_diary_fields(
        popup_profiles::effective_popup_fields(document),
        &document.category,
        &document.role_id,
    )
}

pub fn profession_runtime_control_fields(category: &DomainKind, role_id: &str) -> BTreeSet<String> {
    let mut fields = popup_profiles::profession_runtime_control_fields(category, role_id);
    if is_medical_diary(category, role_id) {
        fields.insert(DIARY_SICK_LEAVE_EPICRISIS.into());
        fields.insert(DIARY_TREATMENT_CORRECTION.into());
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diary_document() -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "diaries".into(),
            button_label: "Дневники".into(),
            template_path: "diaries.docx".into(),
            category: DomainKind::Medical,
            role_id: "diaries".into(),
            required_fields: Vec::new(),
            placeholders: Vec::new(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn donor_sick_leave_epicrisis_popup_matches_donor_contract() {
        let fields = effective_popup_fields(&diary_document());
        let sick_leave = fields
            .iter()
            .find(|field| field.field_id == DIARY_SICK_LEAVE_EPICRISIS)
            .unwrap();
        assert!(sick_leave.required);
        assert_eq!(sick_leave.input_kind, PromptInputKind::YesNo);
        assert_eq!(sick_leave.ask_mode, PromptAskMode::Always);
        assert_eq!(sick_leave.options, ["Нет", "Да"]);
        assert!(sick_leave.default_value.is_none());

        let correction = fields
            .iter()
            .find(|field| field.field_id == DIARY_TREATMENT_CORRECTION)
            .unwrap();
        assert!(!correction.required);
        assert_eq!(correction.input_kind, PromptInputKind::LongText);
        assert_eq!(correction.ask_mode, PromptAskMode::Confirm);
        assert_eq!(
            correction.default_value.as_deref(),
            Some(DEFAULT_TREATMENT_CORRECTION)
        );
        assert_eq!(
            correction.linked_to.as_deref(),
            Some(DIARY_SICK_LEAVE_EPICRISIS)
        );
    }

    #[test]
    fn donor_controls_are_medical_diary_scoped() {
        let medical = profession_runtime_control_fields(&DomainKind::Medical, "diaries");
        assert!(medical.contains(DIARY_SICK_LEAVE_EPICRISIS));
        assert!(medical.contains(DIARY_TREATMENT_CORRECTION));
        assert!(profession_runtime_control_fields(&DomainKind::Hr, "diaries")
            .intersection(&medical)
            .next()
            .is_none());
        assert!(!profession_runtime_control_fields(&DomainKind::Medical, "discharge")
            .contains(DIARY_SICK_LEAVE_EPICRISIS));
    }
}
