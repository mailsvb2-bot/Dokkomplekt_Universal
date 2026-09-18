use crate::{
    accounting_fields, education_fields, generic_fields, hr_fields, legal_fields, medical_fields,
    plugin_by_id, DomainKind, DomainPluginId, DomainProfile, WorkflowRule,
};

pub fn generic_profile() -> DomainProfile {
    DomainProfile {
        id: "generic".into(),
        title: "Универсальные документы".into(),
        kind: DomainKind::Generic,
        fields: generic_fields(),
        workflow_rules: vec![],
    }
}

pub fn medical_profile() -> DomainProfile {
    DomainProfile {
        id: "medical".into(),
        title: "Медицинский профиль".into(),
        kind: DomainKind::Medical,
        fields: medical_fields(),
        workflow_rules: vec![
            WorkflowRule::RequireField {
                document_role: "discharge".into(),
                field_id: "medical.discharge_date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "diaries".into(),
                field_id: "medical.discharge_date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "rvk_act".into(),
                field_id: "medical.discharge_date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "rvk_act".into(),
                field_id: "medical.rvk_commissariat".into(),
            },
            WorkflowRule::RequireFieldWhenFlag {
                document_role: "discharge".into(),
                field_id: "medical.sick_leave_number".into(),
                flag: "sick_leave_enabled".into(),
            },
            WorkflowRule::RequireFieldUnlessPresent {
                document_role: "discharge".into(),
                field_id: "medical.treatment".into(),
                unless_field: "medical.treatment".into(),
            },
            WorkflowRule::SkipForRole {
                document_role: "diaries".into(),
                field_id: "medical.treatment".into(),
            },
        ],
    }
}

pub fn legal_profile() -> DomainProfile {
    DomainProfile {
        id: "legal".into(),
        title: "Юридические документы".into(),
        kind: DomainKind::Legal,
        fields: legal_fields(),
        workflow_rules: plugin_compat_rules(DomainPluginId::Legal),
    }
}

pub fn hr_profile() -> DomainProfile {
    DomainProfile {
        id: "hr".into(),
        title: "Кадровые документы".into(),
        kind: DomainKind::Hr,
        fields: hr_fields(),
        workflow_rules: plugin_compat_rules(DomainPluginId::Hr),
    }
}

pub fn education_profile() -> DomainProfile {
    DomainProfile {
        id: "education".into(),
        title: "Образовательные документы".into(),
        kind: DomainKind::Education,
        fields: education_fields(),
        workflow_rules: plugin_compat_rules(DomainPluginId::Education),
    }
}

pub fn accounting_profile() -> DomainProfile {
    DomainProfile {
        id: "accounting".into(),
        title: "Бухгалтерские документы".into(),
        kind: DomainKind::Accounting,
        fields: accounting_fields(),
        workflow_rules: plugin_compat_rules(DomainPluginId::Accounting),
    }
}

/// Legacy `DomainProfile.workflow_rules` is compatibility metadata. Runtime
/// requiredness is owned by `DomainPluginV2.required_rules`; keep these old
/// profiles as a projection instead of maintaining a second handwritten rule set.
///
/// All built-in non-medical rules are currently unconditional. If a future plugin
/// adds a condition, the regression below fails and requires an explicit legacy
/// compatibility decision. Until then, projecting such a rule as hard-required is
/// conservative/fail-closed rather than silently dropping a canonical requirement.
fn plugin_compat_rules(plugin_id: DomainPluginId) -> Vec<WorkflowRule> {
    plugin_by_id(&plugin_id)
        .required_rules
        .into_iter()
        .map(|rule| WorkflowRule::RequireField {
            document_role: rule.role,
            field_id: rule.field_id,
        })
        .collect()
}

pub fn builtin_profiles() -> Vec<DomainProfile> {
    vec![
        generic_profile(),
        medical_profile(),
        legal_profile(),
        hr_profile(),
        education_profile(),
        accounting_profile(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_rules(plugin_id: DomainPluginId) -> Vec<WorkflowRule> {
        let plugin = plugin_by_id(&plugin_id);
        assert!(
            plugin
                .required_rules
                .iter()
                .all(|rule| rule.when_flag.is_none() && rule.unless_present.is_none()),
            "{plugin_id:?} gained a conditional required rule; define its legacy compatibility semantics explicitly"
        );
        plugin
            .required_rules
            .into_iter()
            .map(|rule| WorkflowRule::RequireField {
                document_role: rule.role,
                field_id: rule.field_id,
            })
            .collect()
    }

    #[test]
    fn nonmedical_profile_rules_are_exact_plugin_compatibility_projections() {
        let cases = [
            (DomainPluginId::Legal, legal_profile()),
            (DomainPluginId::Hr, hr_profile()),
            (DomainPluginId::Education, education_profile()),
            (DomainPluginId::Accounting, accounting_profile()),
        ];

        for (plugin_id, profile) in cases {
            assert_eq!(
                profile.workflow_rules,
                expected_rules(plugin_id.clone()),
                "legacy profile drift for {plugin_id:?}"
            );
        }
    }

    #[test]
    fn projection_closes_known_legal_education_and_accounting_drift() {
        let legal = legal_profile().workflow_rules;
        assert!(legal.contains(&WorkflowRule::RequireField {
            document_role: "acceptance_act".into(),
            field_id: "contract.party_a".into(),
        }));
        assert!(legal.contains(&WorkflowRule::RequireField {
            document_role: "acceptance_act".into(),
            field_id: "contract.party_b".into(),
        }));

        let education = education_profile().workflow_rules;
        assert!(education.contains(&WorkflowRule::RequireField {
            document_role: "certificate".into(),
            field_id: "document.number".into(),
        }));
        assert!(education.contains(&WorkflowRule::RequireField {
            document_role: "certificate".into(),
            field_id: "education.institution".into(),
        }));

        let accounting = accounting_profile().workflow_rules;
        assert!(accounting.contains(&WorkflowRule::RequireField {
            document_role: "invoice".into(),
            field_id: "org.name".into(),
        }));
        assert!(accounting.contains(&WorkflowRule::RequireField {
            document_role: "service_act".into(),
            field_id: "org.name".into(),
        }));
        assert!(accounting.contains(&WorkflowRule::RequireField {
            document_role: "reconciliation".into(),
            field_id: "org.name".into(),
        }));
    }

    #[test]
    fn medical_profile_remains_owned_by_medical_compatibility_mechanics() {
        let medical = medical_profile();
        assert!(medical.workflow_rules.contains(&WorkflowRule::SkipForRole {
            document_role: "diaries".into(),
            field_id: "medical.treatment".into(),
        }));
        assert!(medical.workflow_rules.contains(&WorkflowRule::RequireFieldWhenFlag {
            document_role: "discharge".into(),
            field_id: "medical.sick_leave_number".into(),
            flag: "sick_leave_enabled".into(),
        }));
    }
}
