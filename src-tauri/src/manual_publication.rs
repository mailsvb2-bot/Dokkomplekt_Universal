use dokkomplekt_core::SemanticCase;
use dokkomplekt_docx::extract_docx_text;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Manual generation stages beside the user-visible output root. The visible
/// root is created only by the final publication operation, so a render failure
/// cannot leave an empty directory that looks like a successful kit.
pub(crate) fn stage_parent(output_root: &Path) -> PathBuf {
    output_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_root.to_path_buf())
}

/// A trust report is an ancillary local audit artifact. It must never roll back
/// a successfully rendered document kit. The caller may surface the returned
/// warning, but document publication continues regardless of report failure.
pub(crate) fn optional_trust_report_warning(
    stage: &Path,
    semantic_case: &SemanticCase,
    provenance: Option<&crate::SourceProvenance>,
    generated_names: &[String],
    used_field_ids: &BTreeSet<String>,
    include_values: bool,
) -> Option<String> {
    let provenance = match provenance {
        Some(value) => value,
        None => {
            return Some(
                "DOCX созданы; локальный отчёт проверяемости пропущен: provenance исходника недоступен после восстановления состояния."
                    .into(),
            )
        }
    };
    crate::write_trust_report(
        stage,
        semantic_case,
        crate::TrustReportContext {
            source_name: &provenance.source_name,
            source_sha256: &provenance.source_sha256,
            generated_names,
            used_field_ids,
            include_values,
            source_warnings: &[],
        },
    )
    .err()
    .map(|error| format!("DOCX созданы; локальный отчёт проверяемости не создан: {error}"))
}

/// Prove that rendering produced exactly one non-empty physical file for every
/// requested document before the publication boundary is crossed.
pub(crate) fn verify_staged_docx(
    staged_paths: &[PathBuf],
    expected_count: usize,
) -> Result<(), String> {
    if staged_paths.len() != expected_count {
        return Err(format!(
            "Комплект не опубликован: запрошено {expected_count} документ(ов), физически подготовлено {}.",
            staged_paths.len()
        ));
    }
    for path in staged_paths {
        let metadata = std::fs::metadata(path).map_err(|error| {
            format!(
                "Комплект не опубликован: подготовленный DOCX исчез до публикации ({}): {error}",
                path.display()
            )
        })?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "Комплект не опубликован: подготовленный DOCX пуст или не является файлом: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Reconstruct the published paths from the staged filenames and then prove the
/// post-publication state from disk. A command may report success only when all
/// expected files exist, are non-empty and can be parsed as DOCX.
pub(crate) fn verify_published_docx(
    staged_paths: &[PathBuf],
    output_folder: &Path,
    expected_count: usize,
) -> Result<Vec<PathBuf>, String> {
    let created_files = staged_paths
        .iter()
        .map(|path| {
            path.file_name()
                .ok_or_else(|| format!("Созданный файл не имеет имени: {}", path.display()))
                .map(|name| output_folder.join(name))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if created_files.len() != expected_count {
        return Err(format!(
            "КРИТИЧЕСКАЯ ОШИБКА публикации: ожидалось {expected_count} DOCX, получено {} в {}.",
            created_files.len(),
            output_folder.display()
        ));
    }
    for path in &created_files {
        let metadata = std::fs::metadata(path).map_err(|error| {
            format!(
                "КРИТИЧЕСКАЯ ОШИБКА публикации: файл отсутствует на диске после публикации ({}): {error}",
                path.display()
            )
        })?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "КРИТИЧЕСКАЯ ОШИБКА публикации: опубликованный DOCX пуст или не является файлом: {}",
                path.display()
            ));
        }
        extract_docx_text(path).map_err(|error| {
            format!(
                "КРИТИЧЕСКАЯ ОШИБКА публикации: опубликованный DOCX не читается как Word-документ ({}): {error}",
                path.display()
            )
        })?;
    }
    Ok(created_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn stage_parent_is_sibling_boundary() {
        let root = PathBuf::from("/tmp/Desktop/Выписанные пациенты");
        assert_eq!(stage_parent(&root), PathBuf::from("/tmp/Desktop"));
    }

    #[test]
    fn staged_count_mismatch_fails_closed() {
        let error = verify_staged_docx(&[], 1).unwrap_err();
        assert!(error.contains("физически подготовлено 0"));
    }

    #[test]
    fn empty_staged_file_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-manual-publication-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("empty.docx");
        fs::write(&path, []).unwrap();
        let error = verify_staged_docx(&[path], 1).unwrap_err();
        assert!(error.contains("пуст"));
        let _ = fs::remove_dir_all(root);
    }
}
