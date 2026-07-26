use dokkomplekt_core::{
    canonical_field_candidates, template_block_references, template_collection_references,
    template_field_references, template_image_requests, SemanticCase, SemanticRecord,
    SemanticValue,
};
use dokkomplekt_storage::CaseDocumentRecord;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(crate) fn template_is_resume_safe(template_text: &str, semantic_case: &SemanticCase) -> bool {
    let mut visited_blocks = BTreeSet::new();
    template_tree_is_resume_safe(template_text, semantic_case, &mut visited_blocks)
}

fn template_tree_is_resume_safe(
    template_text: &str,
    semantic_case: &SemanticCase,
    visited_blocks: &mut BTreeSet<String>,
) -> bool {
    let normalized = template_text.to_ascii_lowercase();
    if normalized.contains("{{counter")
        || normalized.contains("{{ counter")
        || normalized.contains("sequence.next")
        || normalized.contains("document.counter")
        || normalized.contains("{{image")
        || normalized.contains("{{ image")
        || normalized.contains("working_days")
        || normalized.contains("workdays")
        || normalized.contains("рабочих_дней")
    {
        return false;
    }
    for block_id in template_block_references(template_text) {
        if !visited_blocks.insert(block_id.clone()) {
            continue;
        }
        if let Some(content) = semantic_case.blocks.get(&block_id) {
            if !template_tree_is_resume_safe(content, semantic_case, visited_blocks) {
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Serialize)]
struct DependencySnapshot {
    values: BTreeMap<String, Option<SemanticValue>>,
    collections: BTreeMap<String, Option<Vec<SemanticRecord>>>,
    blocks: BTreeMap<String, Option<String>>,
    asset_sha256: BTreeMap<String, Option<String>>,
    watermark: Option<String>,
}

#[derive(Default)]
struct DependencyIds {
    fields: BTreeSet<String>,
    collections: BTreeSet<String>,
    blocks: BTreeSet<String>,
    images: BTreeSet<String>,
}

/// Fingerprint only the inputs that can affect one document.
///
/// This is deliberately narrower than serialising the whole SemanticCase: a correction to an
/// unrelated field must not invalidate every already-rendered document in a package. Named blocks
/// are traversed recursively and collection contents are included only when referenced. Templates
/// with counters or images are conservatively rendered again because they depend on external state.
pub(crate) fn document_input_fingerprint(
    document_id: &str,
    template_path: &Path,
    template_text: &str,
    semantic_case: &SemanticCase,
    watermark: Option<&str>,
) -> Result<String, String> {
    let template_bytes = std::fs::read(template_path)
        .map_err(|error| format!("Не удалось прочитать шаблон для resume: {error}"))?;
    let mut ids = DependencyIds::default();
    let mut visited_blocks = BTreeSet::new();
    collect_dependencies(template_text, semantic_case, &mut ids, &mut visited_blocks);

    let values = ids
        .fields
        .iter()
        .map(|field_id| {
            (
                field_id.clone(),
                semantic_case.values.get(field_id).cloned(),
            )
        })
        .collect();
    let collections = ids
        .collections
        .iter()
        .map(|collection_id| {
            (
                collection_id.clone(),
                semantic_case.collections.get(collection_id).cloned(),
            )
        })
        .collect();
    let blocks = ids
        .blocks
        .iter()
        .map(|block_id| {
            (
                block_id.clone(),
                semantic_case.blocks.get(block_id).cloned(),
            )
        })
        .collect();
    let asset_sha256 = ids
        .images
        .iter()
        .map(|field_id| {
            let digest = semantic_case
                .get(field_id)
                .and_then(|value| sha256_file_if_present(Path::new(value)));
            (field_id.clone(), digest)
        })
        .collect();
    let snapshot = DependencySnapshot {
        values,
        collections,
        blocks,
        asset_sha256,
        watermark: watermark.map(str::to_string),
    };
    let dependency_json = serde_json::to_vec(&snapshot)
        .map_err(|error| format!("Не удалось сериализовать зависимости документа: {error}"))?;

    let mut digest = Sha256::new();
    digest.update(b"dokkomplekt-resume-v2\0");
    digest.update(document_id.as_bytes());
    digest.update([0_u8]);
    digest.update(Sha256::digest(&template_bytes));
    digest.update([0_u8]);
    digest.update(Sha256::digest(&dependency_json));
    digest.update([0_u8]);
    digest.update(env!("CARGO_PKG_VERSION").as_bytes());
    Ok(hex::encode(digest.finalize()))
}

fn collect_dependencies(
    template_text: &str,
    semantic_case: &SemanticCase,
    ids: &mut DependencyIds,
    visited_blocks: &mut BTreeSet<String>,
) {
    for reference in template_field_references(template_text) {
        ids.fields.insert(reference.clone());
        for candidate in canonical_field_candidates(&reference) {
            ids.fields.insert(candidate);
        }
    }
    ids.collections
        .extend(template_collection_references(template_text));
    for image_id in template_image_requests(template_text) {
        ids.images.insert(image_id.clone());
        for candidate in canonical_field_candidates(&image_id) {
            ids.images.insert(candidate);
        }
    }

    for block_id in template_block_references(template_text) {
        ids.blocks.insert(block_id.clone());
        if !visited_blocks.insert(block_id.clone()) {
            continue;
        }
        if let Some(content) = semantic_case.blocks.get(&block_id) {
            collect_dependencies(content, semantic_case, ids, visited_blocks);
        }
    }
}

fn sha256_file_if_present(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(hex::encode(Sha256::digest(bytes)))
}

pub(crate) fn checkpoint_root(app_data: &Path, case_id: &str) -> PathBuf {
    app_data
        .join("case-checkpoints")
        .join(safe_component(case_id))
}

pub(crate) fn checkpoint_path(app_data: &Path, case_id: &str, file_name: &str) -> PathBuf {
    checkpoint_root(app_data, case_id).join(safe_component(file_name))
}

#[derive(Debug, Clone)]
pub(crate) struct CheckpointArtifact {
    pub path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
}

fn file_integrity(path: &Path) -> Result<(String, u64), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let size_bytes = u64::try_from(bytes.len()).map_err(|_| "Файл слишком большой.".to_string())?;
    if size_bytes == 0 {
        return Err("Checkpoint не может быть пустым.".into());
    }
    Ok((hex::encode(Sha256::digest(&bytes)), size_bytes))
}

pub(crate) fn persist_checkpoint(
    rendered_path: &Path,
    app_data: &Path,
    case_id: &str,
    file_name: &str,
) -> Result<CheckpointArtifact, String> {
    let target = checkpoint_path(app_data, case_id, file_name);
    let parent = target
        .parent()
        .ok_or_else(|| "Некорректный путь checkpoint.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = target.with_extension("checkpoint.tmp");
    std::fs::copy(rendered_path, &temporary).map_err(|error| error.to_string())?;
    if target.exists() {
        std::fs::remove_file(&target).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
    let (sha256, size_bytes) = file_integrity(&target)?;
    Ok(CheckpointArtifact {
        path: target,
        sha256,
        size_bytes,
    })
}

pub(crate) fn reusable_checkpoint<'a>(
    records: &'a [CaseDocumentRecord],
    document_id: &str,
    fingerprint: &str,
) -> Option<&'a CaseDocumentRecord> {
    records.iter().find(|record| {
        record.document_id == document_id
            && record.input_fingerprint == fingerprint
            && matches!(record.status.as_str(), "rendered" | "published" | "reused")
            && file_integrity(Path::new(&record.output_path)).is_ok_and(|(sha256, size_bytes)| {
                sha256 == record.output_sha256 && size_bytes == record.output_size_bytes
            })
    })
}

pub(crate) fn remove_checkpoint_tree(app_data: &Path, case_id: &str) {
    let _ = std::fs::remove_dir_all(checkpoint_root(app_data, case_id));
}

fn safe_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if cleaned.is_empty() {
        "unnamed".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dokkomplekt_core::{SemanticValue, ValueSource};

    fn test_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dokkomplekt-resume-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn counters_disable_checkpoint_reuse() {
        let case = SemanticCase::default();
        assert!(!template_is_resume_safe("Номер {{counter invoice}}", &case));
        assert!(!template_is_resume_safe("{{image org.stamp}}", &case));
        assert!(!template_is_resume_safe(
            "Срок: {{working_days document.start document.end}}",
            &case
        ));
        assert!(template_is_resume_safe("ФИО {{subject.name}}", &case));

        let mut case_with_counter_block = SemanticCase::default();
        case_with_counter_block
            .blocks
            .insert("number".into(), "{{counter invoice}}".into());
        assert!(!template_is_resume_safe(
            "{{block number}}",
            &case_with_counter_block
        ));
    }

    #[test]
    fn path_component_cannot_escape_checkpoint_root() {
        assert_eq!(safe_component("../../x.docx"), ".._.._x.docx");
    }

    #[test]
    fn unrelated_semantic_value_does_not_invalidate_document() {
        let dir = test_directory("unrelated");
        let template = dir.join("template.docx");
        std::fs::write(&template, "ФИО: {{subject.name}}").unwrap();
        let mut case = SemanticCase::default();
        case.values.insert(
            "subject.name".into(),
            SemanticValue::new("subject.name", "Иванов", ValueSource::UserConfirmed, 1.0),
        );
        let first =
            document_input_fingerprint("doc", &template, "ФИО: {{subject.name}}", &case, None)
                .unwrap();
        case.values.insert(
            "contract.number".into(),
            SemanticValue::new("contract.number", "42", ValueSource::UserConfirmed, 1.0),
        );
        let second =
            document_input_fingerprint("doc", &template, "ФИО: {{subject.name}}", &case, None)
                .unwrap();
        assert_eq!(first, second);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn checkpoint_reuse_rejects_tampered_output() {
        let dir = test_directory("tamper");
        let output = dir.join("result.docx");
        std::fs::write(&output, b"original").unwrap();
        let (sha256, size_bytes) = file_integrity(&output).unwrap();
        let records = vec![CaseDocumentRecord {
            case_id: "case-1".into(),
            document_id: "doc-1".into(),
            input_fingerprint: "a".repeat(64),
            output_path: output.display().to_string(),
            output_sha256: sha256,
            output_size_bytes: size_bytes,
            status: "rendered".into(),
            reused_from_case_id: None,
            created_at: String::new(),
            updated_at: String::new(),
        }];
        assert!(reusable_checkpoint(&records, "doc-1", &"a".repeat(64)).is_some());
        std::fs::write(&output, b"tampered").unwrap();
        assert!(reusable_checkpoint(&records, "doc-1", &"a".repeat(64)).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn referenced_value_and_watermark_invalidate_document() {
        let dir = test_directory("referenced");
        let template = dir.join("template.docx");
        std::fs::write(&template, "ФИО: {{subject.name}}").unwrap();
        let mut case = SemanticCase::default();
        case.values.insert(
            "subject.name".into(),
            SemanticValue::new("subject.name", "Иванов", ValueSource::UserConfirmed, 1.0),
        );
        let first =
            document_input_fingerprint("doc", &template, "ФИО: {{subject.name}}", &case, None)
                .unwrap();
        case.values.get_mut("subject.name").unwrap().value = "Петров".into();
        let second =
            document_input_fingerprint("doc", &template, "ФИО: {{subject.name}}", &case, None)
                .unwrap();
        let watermarked = document_input_fingerprint(
            "doc",
            &template,
            "ФИО: {{subject.name}}",
            &case,
            Some("TRIAL"),
        )
        .unwrap();
        assert_ne!(first, second);
        assert_ne!(second, watermarked);
        let _ = std::fs::remove_dir_all(dir);
    }
}
