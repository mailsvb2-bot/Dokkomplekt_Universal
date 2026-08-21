use crate::{canonical_storage_field_id, known_field_ids, DocumentPack, DomainKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const WORKSPACE_PROFILE_STATE_SCHEMA_VERSION: u32 = 1;
pub const WORKSPACE_PROFILE_STATE_KEY: &str = "workspace_profile_state_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedWorkspaceDocumentProfile {
    pub document_id: String,
    pub role_id: String,
    pub domain_key: String,
    pub field_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedWorkspaceProfile {
    pub schema_version: u32,
    pub workspace_fingerprint: String,
    pub documents: Vec<PersistedWorkspaceDocumentProfile>,
    pub domain_counts: BTreeMap<String, usize>,
}

pub fn workspace_profile_from_pack(pack: &DocumentPack) -> PersistedWorkspaceProfile {
    let known_fields = known_field_ids();
    let mut documents = pack
        .documents
        .iter()
        .map(|document| {
            let mut fields = BTreeSet::new();
            for field in document
                .placeholders
                .iter()
                .chain(document.required_fields.iter())
            {
                let canonical = canonical_storage_field_id(field);
                fields.insert(privacy_safe_field_key(&canonical, &known_fields));
            }
            for field in &document.popup_fields {
                let canonical = canonical_storage_field_id(&field.field_id);
                fields.insert(privacy_safe_field_key(&canonical, &known_fields));
            }
            PersistedWorkspaceDocumentProfile {
                document_id: document.id.clone(),
                role_id: document.role_id.clone(),
                domain_key: privacy_safe_domain_key(&document.category),
                field_ids: fields.into_iter().collect(),
            }
        })
        .collect::<Vec<_>>();
    documents.sort_by(|a, b| a.document_id.cmp(&b.document_id));
    let mut domain_counts = BTreeMap::new();
    for document in &documents {
        *domain_counts
            .entry(document.domain_key.clone())
            .or_insert(0) += 1;
    }
    let workspace_fingerprint = fingerprint_documents(&documents);
    PersistedWorkspaceProfile {
        schema_version: WORKSPACE_PROFILE_STATE_SCHEMA_VERSION,
        workspace_fingerprint,
        documents,
        domain_counts,
    }
}

pub fn workspace_profile_matches_pack(
    profile: &PersistedWorkspaceProfile,
    pack: &DocumentPack,
) -> bool {
    profile.schema_version == WORKSPACE_PROFILE_STATE_SCHEMA_VERSION
        && profile.workspace_fingerprint == workspace_profile_from_pack(pack).workspace_fingerprint
}

fn fingerprint_documents(documents: &[PersistedWorkspaceDocumentProfile]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"dokkomplekt-workspace-profile-v1\0");
    for document in documents {
        hasher.update(document.document_id.as_bytes());
        hasher.update([0]);
        hasher.update(document.role_id.as_bytes());
        hasher.update([0]);
        hasher.update(document.domain_key.as_bytes());
        hasher.update([0]);
        for field_id in &document.field_ids {
            hasher.update(field_id.as_bytes());
            hasher.update([0]);
        }
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

fn privacy_safe_field_key(field_id: &str, known_fields: &BTreeSet<String>) -> String {
    if known_fields.contains(field_id) {
        return field_id.to_string();
    }
    let digest = Sha256::digest(field_id.trim().to_lowercase().as_bytes());
    format!("extension-sha256:{digest:x}")
}

fn privacy_safe_domain_key(domain: &DomainKind) -> String {
    match domain {
        DomainKind::Generic => "generic".into(),
        DomainKind::Medical => "medical".into(),
        DomainKind::Legal => "legal".into(),
        DomainKind::Hr => "hr".into(),
        DomainKind::Accounting => "accounting".into(),
        DomainKind::Education => "education".into(),
        DomainKind::Custom(value) => {
            let normalized = value.trim().to_lowercase();
            let digest = Sha256::digest(normalized.as_bytes());
            format!("custom-sha256:{digest:x}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentTemplateSpec;

    fn doc(
        id: &str,
        label: &str,
        role: &str,
        domain: DomainKind,
        fields: &[&str],
    ) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: id.into(),
            button_label: label.into(),
            template_path: format!("C:/Private/{label}.docx"),
            category: domain,
            role_id: role.into(),
            required_fields: fields.iter().map(|v| v.to_string()).collect(),
            placeholders: fields.iter().map(|v| v.to_string()).collect(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn fingerprint_ignores_titles_and_paths_but_tracks_structure() {
        let mut pack = DocumentPack {
            pack_id: "x".into(),
            name: "x".into(),
            documents: vec![doc(
                "a",
                "Иванов",
                "claim",
                DomainKind::Legal,
                &["subject.name"],
            )],
        };
        let first = workspace_profile_from_pack(&pack);
        pack.documents[0].button_label = "Петров".into();
        pack.documents[0].template_path = "D:/Secret/Other.docx".into();
        let renamed = workspace_profile_from_pack(&pack);
        assert_eq!(first.workspace_fingerprint, renamed.workspace_fingerprint);
        pack.documents[0]
            .required_fields
            .push("document.number".into());
        let changed = workspace_profile_from_pack(&pack);
        assert_ne!(first.workspace_fingerprint, changed.workspace_fingerprint);
    }

    #[test]
    fn persisted_profile_contains_no_titles_paths_or_values() {
        let pack = DocumentPack {
            pack_id: "x".into(),
            name: "Пациент Иванов".into(),
            documents: vec![doc(
                "a",
                "Иванов Иван",
                "primary",
                DomainKind::Custom("Сотрудник Иванов".into()),
                &["subject.name"],
            )],
        };
        let mut pack = pack;
        pack.documents[0]
            .required_fields
            .push("custom.ivanov_secret".into());
        let json = serde_json::to_string(&workspace_profile_from_pack(&pack)).unwrap();
        assert!(!json.contains("Иванов"));
        assert!(!json.contains("Private"));
        assert!(!json.contains("docx"));
        assert!(!json.contains("Сотрудник"));
        assert!(json.contains("custom-sha256:"));
        assert!(json.contains("extension-sha256:"));
        assert!(!json.contains("custom.ivanov_secret"));
        assert!(json.contains("subject.name"));
    }

    #[test]
    fn profile_from_another_workspace_never_matches() {
        let left = DocumentPack {
            pack_id: "x".into(),
            name: "x".into(),
            documents: vec![doc("a", "A", "claim", DomainKind::Legal, &["subject.name"])],
        };
        let right = DocumentPack {
            pack_id: "x".into(),
            name: "x".into(),
            documents: vec![doc(
                "b",
                "B",
                "employment_order",
                DomainKind::Hr,
                &["subject.name"],
            )],
        };
        let snapshot = workspace_profile_from_pack(&left);
        assert!(!workspace_profile_matches_pack(&snapshot, &right));
    }
}
