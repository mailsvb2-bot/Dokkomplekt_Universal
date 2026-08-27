// Persistent professional profile hydration for render-time SemanticCase clones.

fn merge_persistent_clause_blocks(
    case: &mut SemanticCase,
    persistent: std::collections::BTreeMap<String, String>,
) {
    // Profile-owned `professional.*` blocks live in the clause-block store and
    // that store is their source of truth. Older snapshots may still contain a
    // stale copy in SemanticCase (including an empty diary tombstone); allowing
    // that copy to win makes a newly selected Texts file invisible to rendering.
    // Patient/source-local blocks keep their existing precedence.
    for (block_id, content) in persistent {
        if block_id.starts_with("professional.") {
            case.blocks.insert(block_id, content);
        } else {
            case.blocks.entry(block_id).or_insert(content);
        }
    }
}

#[cfg(test)]
mod profile_case_hydration_tests {
    use super::merge_persistent_clause_blocks;

    #[test]
    fn persistent_profile_blocks_override_stale_case_copies_but_not_patient_blocks() {
        let mut case = dokkomplekt_core::SemanticCase::default();
        case.blocks.insert(
            "medical.diary.final_text".into(),
            "Текущий подтверждённый текст пациента".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            String::new(),
        );
        let persistent = std::collections::BTreeMap::from([
            (
                "medical.diary.final_text".into(),
                "СТАРЫЙ профильный текст, который не должен победить".into(),
            ),
            (
                "professional.medical.diary.regular.f200".into(),
                "Свежий текст из только что выбранного файла «Тексты».".into(),
            ),
        ]);

        merge_persistent_clause_blocks(&mut case, persistent);

        assert_eq!(
            case.blocks
                .get("medical.diary.final_text")
                .map(String::as_str),
            Some("Текущий подтверждённый текст пациента")
        );
        assert_eq!(
            case.blocks
                .get("professional.medical.diary.regular.f200")
                .map(String::as_str),
            Some("Свежий текст из только что выбранного файла «Тексты».")
        );
    }

    #[test]
    fn fresh_diary_text_from_persistent_profile_survives_stale_empty_case_snapshot() {
        let mut case = dokkomplekt_core::SemanticCase::default();
        case.active_domains
            .push(dokkomplekt_core::DomainKind::Medical);
        for (field_id, value) in [
            ("medical.admission_date", "10.05.2026"),
            ("medical.discharge_date", "13.05.2026"),
            ("medical.diagnosis", "F20.0 Шизофрения параноидная"),
            ("medical.diary_schedule_style", "Каждый день"),
            ("medical.diary_intraday_rhythm", "Один раз в день"),
        ] {
            dokkomplekt_core::set_user_value(&mut case, field_id, value);
        }
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            String::new(),
        );
        let persistent = std::collections::BTreeMap::from([
            (
                "professional.medical.diary.regular.f200".into(),
                "Состояние стабильное, контакт продуктивный, назначения выполняет; это новый выбранный врачом текст.".into(),
            ),
            (
                "professional.medical.diary.final.f200".into(),
                String::new(),
            ),
        ]);
        merge_persistent_clause_blocks(&mut case, persistent);

        let rendered = dokkomplekt_core::render_text_template(
            "{{#each diaries}}{{diary.datetime}} {{diary.text}}\n{{/each}}",
            &case,
            true,
        );
        assert!(
            rendered.missing_fields.is_empty(),
            "{:?}",
            rendered.missing_fields
        );
        assert!(
            rendered.unknown_fields.is_empty(),
            "{:?}",
            rendered.unknown_fields
        );
        assert!(rendered
            .output_text
            .contains("новый выбранный врачом текст"));
    }
}
