fn canonicalize_loaded_pack_roles(pack: &mut DocumentPack) -> usize {
    let mut changed = 0usize;
    for document in &mut pack.documents {
        let canonical = dokkomplekt_core::universal_pipeline::canonical_role_for_category(
            &document.category,
            &document.role_id,
        )
        .unwrap_or_else(|| document.role_id.clone());
        if canonical != document.role_id {
            document.role_id = canonical;
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod loaded_pack_role_canonicalization_tests {
    use super::*;

    fn document(id: &str, category: DomainKind, role_id: &str) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: id.into(),
            button_label: id.into(),
            template_path: format!("{id}.docx"),
            category,
            role_id: role_id.into(),
            required_fields: Vec::new(),
            placeholders: Vec::new(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn legacy_roles_are_canonical_before_the_pack_reaches_the_ui() {
        let mut pack = DocumentPack {
            pack_id: "default".into(),
            name: "legacy".into(),
            documents: vec![
                document("discharge", DomainKind::Medical, "dischargeEpicrisis"),
                document("diary", DomainKind::Medical, "medicalDiary"),
                document("invoice", DomainKind::Accounting, "Счёт на оплату"),
                document("custom", DomainKind::Custom("x".into()), "my-special-role"),
            ],
        };

        assert_eq!(canonicalize_loaded_pack_roles(&mut pack), 3);
        assert_eq!(pack.documents[0].role_id, "discharge");
        assert_eq!(pack.documents[1].role_id, "diaries");
        assert_eq!(pack.documents[2].role_id, "invoice");
        assert_eq!(pack.documents[3].role_id, "my-special-role");
        assert_eq!(
            canonicalize_loaded_pack_roles(&mut pack),
            0,
            "migration must be idempotent"
        );
    }

    #[test]
    fn persisted_selection_is_canonicalized_to_current_pack_order() {
        let pack = DocumentPack {
            pack_id: "default".into(),
            name: "buttons".into(),
            documents: vec![
                document("alpha", DomainKind::Generic, "generic"),
                document("beta", DomainKind::Generic, "generic"),
                document("gamma", DomainKind::Generic, "generic"),
            ],
        };

        assert_eq!(
            normalize_document_selection(
                &pack,
                &[
                    "gamma".into(),
                    "unknown".into(),
                    "alpha".into(),
                    "gamma".into(),
                ],
            ),
            vec!["alpha".to_string(), "gamma".to_string()]
        );
    }
}
