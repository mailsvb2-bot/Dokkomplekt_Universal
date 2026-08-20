use crate::{
    canonical_storage_field_id, is_valid_field_id, related_document_roles, title_for_field,
    DomainKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceShapeDocumentInput {
    pub document_id: String,
    pub title: String,
    pub role_id: String,
    pub domain: DomainKind,
    pub field_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFieldUsage {
    pub field_id: String,
    pub title: String,
    pub document_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDocumentRole {
    pub document_id: String,
    pub title: String,
    pub role_id: String,
    pub role_label: String,
    pub domain: DomainKind,
    pub field_ids: Vec<String>,
    pub local_field_ids: Vec<String>,
    pub group_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDocumentGroup {
    pub group_id: String,
    pub title: String,
    pub domain: DomainKind,
    pub document_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDocumentRelation {
    pub left_document_id: String,
    pub right_document_id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceWorkflowShape {
    pub primary_object: String,
    pub common_fields: Vec<WorkspaceFieldUsage>,
    pub local_fields: BTreeMap<String, Vec<WorkspaceFieldUsage>>,
    pub documents: Vec<WorkspaceDocumentRole>,
    pub groups: Vec<WorkspaceDocumentGroup>,
    pub relations: Vec<WorkspaceDocumentRelation>,
    pub mixed_workflows: bool,
    pub reasons: Vec<String>,
}

pub fn infer_workspace_workflow_shape(
    inputs: &[WorkspaceShapeDocumentInput],
) -> WorkspaceWorkflowShape {
    if inputs.is_empty() {
        return WorkspaceWorkflowShape::default();
    }

    let normalized = inputs
        .iter()
        .map(|document| {
            let mut field_ids = document
                .field_ids
                .iter()
                .filter(|field| is_valid_field_id(field))
                .map(|field| canonical_storage_field_id(field))
                .collect::<Vec<_>>();
            field_ids.sort();
            field_ids.dedup();
            WorkspaceShapeDocumentInput {
                document_id: document.document_id.clone(),
                title: document.title.clone(),
                role_id: document.role_id.clone(),
                domain: document.domain.clone(),
                field_ids,
            }
        })
        .collect::<Vec<_>>();

    let mut field_documents = BTreeMap::<String, BTreeSet<String>>::new();
    for document in &normalized {
        for field_id in &document.field_ids {
            field_documents
                .entry(field_id.clone())
                .or_default()
                .insert(document.document_id.clone());
        }
    }

    let mut common_fields = field_documents
        .iter()
        .filter(|(_, document_ids)| document_ids.len() >= 2)
        .map(|(field_id, document_ids)| field_usage(field_id, document_ids))
        .collect::<Vec<_>>();
    common_fields.sort_by(|left, right| {
        right
            .document_ids
            .len()
            .cmp(&left.document_ids.len())
            .then(left.title.cmp(&right.title))
            .then(left.field_id.cmp(&right.field_id))
    });
    let common_ids = common_fields
        .iter()
        .map(|field| field.field_id.clone())
        .collect::<BTreeSet<_>>();

    let mut local_fields = BTreeMap::<String, Vec<WorkspaceFieldUsage>>::new();
    for document in &normalized {
        let mut usages = document
            .field_ids
            .iter()
            .filter(|field_id| !common_ids.contains(*field_id))
            .map(|field_id| {
                let document_ids = BTreeSet::from([document.document_id.clone()]);
                field_usage(field_id, &document_ids)
            })
            .collect::<Vec<_>>();
        usages.sort_by(|left, right| {
            left.title
                .cmp(&right.title)
                .then(left.field_id.cmp(&right.field_id))
        });
        local_fields.insert(document.document_id.clone(), usages);
    }

    let mut domain_documents = BTreeMap::<String, (DomainKind, Vec<String>)>::new();
    for document in &normalized {
        let key = domain_key(&document.domain);
        let entry = domain_documents
            .entry(key)
            .or_insert_with(|| (document.domain.clone(), Vec::new()));
        entry.1.push(document.document_id.clone());
    }
    for (_, document_ids) in domain_documents.values_mut() {
        document_ids.sort();
    }

    let non_generic_domains = normalized
        .iter()
        .filter(|document| !matches!(document.domain, DomainKind::Generic))
        .map(|document| domain_key(&document.domain))
        .collect::<BTreeSet<_>>();
    let mixed_workflows = non_generic_domains.len() > 1;

    let groups = domain_documents
        .into_iter()
        .map(
            |(group_id, (domain, document_ids))| WorkspaceDocumentGroup {
                group_id,
                title: domain_label(&domain),
                domain,
                document_ids,
            },
        )
        .collect::<Vec<_>>();

    let mut documents = normalized
        .iter()
        .map(|document| WorkspaceDocumentRole {
            document_id: document.document_id.clone(),
            title: document.title.clone(),
            role_id: document.role_id.clone(),
            role_label: role_label(&document.role_id),
            domain: document.domain.clone(),
            field_ids: document.field_ids.clone(),
            local_field_ids: document
                .field_ids
                .iter()
                .filter(|field_id| !common_ids.contains(*field_id))
                .cloned()
                .collect(),
            group_id: domain_key(&document.domain),
        })
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| {
        left.group_id
            .cmp(&right.group_id)
            .then(left.title.cmp(&right.title))
    });

    let relations = infer_relations(&normalized);
    let primary_object = infer_primary_object(&normalized, mixed_workflows);
    let mut reasons = vec![format!(
        "В наборе {} документ(ов), {} общих семантических полей и {} связей между ролями.",
        normalized.len(),
        common_fields.len(),
        relations.len()
    )];
    if mixed_workflows {
        reasons.push(
            "Найдены несколько профессиональных контуров; они сгруппированы отдельно и не объединяются принудительно."
                .into(),
        );
    } else {
        reasons.push(
            "Карта использует только уже распознанные роли, канонические поля и связи routing; она не создаёт отдельный workflow-движок."
                .into(),
        );
    }

    WorkspaceWorkflowShape {
        primary_object,
        common_fields,
        local_fields,
        documents,
        groups,
        relations,
        mixed_workflows,
        reasons,
    }
}

fn field_usage(field_id: &str, document_ids: &BTreeSet<String>) -> WorkspaceFieldUsage {
    WorkspaceFieldUsage {
        field_id: field_id.to_string(),
        title: title_for_field(field_id),
        document_ids: document_ids.iter().cloned().collect(),
    }
}

fn infer_relations(inputs: &[WorkspaceShapeDocumentInput]) -> Vec<WorkspaceDocumentRelation> {
    let mut relations = Vec::new();
    let mut seen = BTreeSet::new();
    for left in inputs {
        if left.role_id == "unknown" || left.role_id.trim().is_empty() {
            continue;
        }
        for expected in related_document_roles(&left.role_id) {
            if let Some(right) = inputs.iter().find(|candidate| {
                candidate.document_id != left.document_id && candidate.role_id == *expected
            }) {
                let mut ids = [left.document_id.clone(), right.document_id.clone()];
                ids.sort();
                let key = format!("{}::{}", ids[0], ids[1]);
                if seen.insert(key) {
                    relations.push(WorkspaceDocumentRelation {
                        left_document_id: left.document_id.clone(),
                        right_document_id: right.document_id.clone(),
                        kind: "canonical_role_relation".into(),
                        label: format!(
                            "{} ↔ {}",
                            role_label(&left.role_id),
                            role_label(&right.role_id)
                        ),
                    });
                }
            }
        }
    }
    relations.sort_by(|left, right| {
        left.left_document_id
            .cmp(&right.left_document_id)
            .then(left.right_document_id.cmp(&right.right_document_id))
    });
    relations
}

fn infer_primary_object(inputs: &[WorkspaceShapeDocumentInput], mixed: bool) -> String {
    if mixed {
        return "Несколько рабочих контуров".into();
    }
    let domain = inputs
        .iter()
        .find(|document| !matches!(document.domain, DomainKind::Generic))
        .map(|document| &document.domain);
    match domain {
        Some(DomainKind::Medical) => "Человек / случай".into(),
        Some(DomainKind::Hr) => "Сотрудник / кадровое событие".into(),
        Some(DomainKind::Legal) => "Клиент / дело".into(),
        Some(DomainKind::Accounting) => "Контрагент / расчёт".into(),
        Some(DomainKind::Education) => "Обучающийся / обучение".into(),
        Some(DomainKind::Custom(name)) => format!("Рабочий объект: {}", name.trim()),
        _ if inputs
            .iter()
            .flat_map(|document| document.field_ids.iter())
            .any(|field| field == "subject.name") =>
        {
            "Человек / субъект".into()
        }
        _ if inputs
            .iter()
            .flat_map(|document| document.field_ids.iter())
            .any(|field| field == "org.name") =>
        {
            "Организация / контрагент".into()
        }
        _ => "Рабочий объект".into(),
    }
}

fn role_label(role_id: &str) -> String {
    match role_id {
        "primary" => "Начало / первичный документ".into(),
        "diaries" => "Повторяющиеся записи".into(),
        "discharge" => "Завершение / итоговый документ".into(),
        "sick_leave_vk" => "Комиссионное решение по больничному".into(),
        "vk_mse" => "Направление на МСЭ".into(),
        "reception" => "Приём / первичная маршрутизация".into(),
        "rvk_act" => "Акт для военного комиссариата".into(),
        "commission" => "Комиссионный документ".into(),
        "employment_contract" => "Трудовой договор".into(),
        "employment_order" => "Кадровый приказ".into(),
        "personal_data_consent" => "Согласие на персональные данные".into(),
        "familiarization_sheet" => "Ознакомление".into(),
        "contract" => "Договор".into(),
        "acceptance_act" => "Акт приёмки".into(),
        "cover_letter" => "Сопроводительный документ".into(),
        "claim" => "Судебный документ / иск".into(),
        "invoice" => "Счёт".into(),
        "service_act" => "Акт услуг / работ".into(),
        "reconciliation" => "Акт сверки".into(),
        "certificate" => "Справка".into(),
        "grade_report" => "Учебная ведомость".into(),
        "unknown" | "" => "Роль ещё не определена".into(),
        other => other.replace('_', " "),
    }
}

fn domain_key(domain: &DomainKind) -> String {
    match domain {
        DomainKind::Generic => "generic".into(),
        DomainKind::Medical => "medical".into(),
        DomainKind::Legal => "legal".into(),
        DomainKind::Hr => "hr".into(),
        DomainKind::Education => "education".into(),
        DomainKind::Accounting => "accounting".into(),
        DomainKind::Custom(name) => format!("custom-{}", slug(name)),
    }
}

fn domain_label(domain: &DomainKind) -> String {
    match domain {
        DomainKind::Generic => "Общие документы".into(),
        DomainKind::Medical => "Медицина".into(),
        DomainKind::Legal => "Юридическая работа".into(),
        DomainKind::Hr => "Кадры".into(),
        DomainKind::Education => "Образование".into(),
        DomainKind::Accounting => "Бухгалтерия".into(),
        DomainKind::Custom(name) => name.trim().to_string(),
    }
}

fn slug(value: &str) -> String {
    let slug = value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "profile".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(
        id: &str,
        title: &str,
        role: &str,
        domain: DomainKind,
        fields: &[&str],
    ) -> WorkspaceShapeDocumentInput {
        WorkspaceShapeDocumentInput {
            document_id: id.into(),
            title: title.into(),
            role_id: role.into(),
            domain,
            field_ids: fields.iter().map(|field| (*field).into()).collect(),
        }
    }

    #[test]
    fn medical_shape_finds_common_local_fields_and_canonical_relations() {
        let shape = infer_workspace_workflow_shape(&[
            document(
                "primary",
                "Первичный осмотр",
                "primary",
                DomainKind::Medical,
                &[
                    "subject.name",
                    "medical.diagnosis",
                    "medical.admission_date",
                ],
            ),
            document(
                "diaries",
                "Дневники наблюдения",
                "diaries",
                DomainKind::Medical,
                &["subject.name", "medical.diagnosis"],
            ),
            document(
                "discharge",
                "Выписной эпикриз",
                "discharge",
                DomainKind::Medical,
                &[
                    "subject.name",
                    "medical.diagnosis",
                    "medical.discharge_date",
                ],
            ),
        ]);

        assert_eq!(shape.primary_object, "Человек / случай");
        assert_eq!(shape.groups.len(), 1);
        assert!(shape
            .common_fields
            .iter()
            .any(|field| field.field_id == "subject.name"));
        assert!(shape
            .common_fields
            .iter()
            .any(|field| field.field_id == "medical.diagnosis"));
        assert!(shape
            .local_fields
            .get("primary")
            .unwrap()
            .iter()
            .any(|field| field.field_id == "medical.admission_date"));
        assert!(shape.relations.iter().any(|relation| {
            relation.left_document_id == "primary" && relation.right_document_id == "diaries"
        }));
        assert!(!shape.mixed_workflows);
    }

    #[test]
    fn mixed_domains_are_grouped_without_forcing_one_profession() {
        let shape = infer_workspace_workflow_shape(&[
            document(
                "claim",
                "Иск",
                "claim",
                DomainKind::Legal,
                &["subject.name"],
            ),
            document(
                "hire",
                "Приказ",
                "employment_order",
                DomainKind::Hr,
                &["subject.name", "employee.position"],
            ),
        ]);
        assert!(shape.mixed_workflows);
        assert_eq!(shape.primary_object, "Несколько рабочих контуров");
        assert_eq!(shape.groups.len(), 2);
        assert!(shape
            .groups
            .iter()
            .any(|group| group.domain == DomainKind::Legal));
        assert!(shape
            .groups
            .iter()
            .any(|group| group.domain == DomainKind::Hr));
    }

    #[test]
    fn unknown_generic_workspace_stays_usable() {
        let shape = infer_workspace_workflow_shape(&[document(
            "generic",
            "Документ",
            "unknown",
            DomainKind::Generic,
            &["document.number", "subject.name"],
        )]);
        assert_eq!(shape.primary_object, "Человек / субъект");
        assert_eq!(shape.groups.len(), 1);
        assert!(shape.relations.is_empty());
    }
}
