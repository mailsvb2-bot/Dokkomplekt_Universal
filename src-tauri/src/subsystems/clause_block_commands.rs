// Professional reusable block storage commands.
//
// Kept separate from the automation runtime so user-owned profile materials
// have one bounded persistence seam and cannot bloat unrelated automation code.

#[tauri::command]
fn list_clause_blocks(app: tauri::AppHandle) -> Result<Vec<ClauseBlockRecord>, String> {
    repository_for(&default_state_db_path(&app)?)?
        .list_clause_blocks()
        .map_err(|e| e.to_string())
}
#[derive(Debug, Deserialize)]
struct SaveClauseBlockRequest {
    block_id: String,
    title: String,
    content: String,
}

const MAX_CLAUSE_BLOCK_ID_CHARS: usize = 512;

fn valid_clause_block_id(raw: &str) -> bool {
    let id = raw.trim();
    !id.is_empty()
        && id.chars().count() <= MAX_CLAUSE_BLOCK_ID_CHARS
        && !id.starts_with('.')
        && !id.ends_with('.')
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod clause_block_id_contract_tests {
    use super::valid_clause_block_id;

    #[test]
    fn reusable_block_ids_accept_unicode_segments_used_by_universal_materials() {
        for id in [
            "professional.medical.diary.regular.f200",
            "professional.material.legal.договор",
            "professional.material.custom-кадры.приказ2026",
            "professional.material.education.учебныйПлан42",
        ] {
            assert!(valid_clause_block_id(id), "valid universal block id rejected: {id}");
        }
    }

    #[test]
    fn reusable_block_id_limit_counts_unicode_characters_not_utf8_bytes() {
        let long_but_valid = format!("professional.material.custom.{}", "я".repeat(400));
        assert!(valid_clause_block_id(&long_but_valid));
        assert!(!valid_clause_block_id(&"я".repeat(MAX_CLAUSE_BLOCK_ID_CHARS + 1)));
    }

    #[test]
    fn reusable_block_ids_still_reject_path_and_empty_segment_tricks() {
        for id in [
            "",
            ".professional",
            "professional.",
            "professional..medical",
            "professional/material",
            r"professional\material",
            "professional material",
            "professional	material",
        ] {
            assert!(!valid_clause_block_id(id), "unsafe block id accepted: {id:?}");
        }
    }
}

fn validate_clause_block_id(raw: &str) -> Result<&str, String> {
    let id = raw.trim();
    if !valid_clause_block_id(id) {
        return Err(
            "Идентификатор блока должен состоять из непустых сегментов: буквы, цифры, _, - и точки между сегментами.".into(),
        );
    }
    Ok(id)
}

#[tauri::command]
fn save_clause_block(
    req: SaveClauseBlockRequest,
    app: tauri::AppHandle,
) -> Result<Vec<ClauseBlockRecord>, String> {
    let id = validate_clause_block_id(&req.block_id)?;
    let repo = repository_for(&default_state_db_path(&app)?)?;
    repo.save_clause_block(id, req.title.trim(), &req.content)
        .map_err(|e| e.to_string())?;
    repo.list_clause_blocks().map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct ReplaceClauseBlocksRequest {
    #[serde(default)]
    delete_block_ids: Vec<String>,
    #[serde(default)]
    blocks: Vec<SaveClauseBlockRequest>,
}

#[tauri::command]
fn replace_clause_blocks(
    req: ReplaceClauseBlocksRequest,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    if req.delete_block_ids.len() > 512 || req.blocks.len() > 512 {
        return Err("Слишком много профильных блоков в одной операции.".into());
    }
    let mut delete_ids = Vec::with_capacity(req.delete_block_ids.len());
    let mut seen_delete = BTreeSet::new();
    for raw in &req.delete_block_ids {
        let id = validate_clause_block_id(raw)?.to_string();
        if seen_delete.insert(id.clone()) {
            delete_ids.push(id);
        }
    }
    let mut replacements = Vec::with_capacity(req.blocks.len());
    let mut seen_replacements = BTreeSet::new();
    for block in req.blocks {
        let id = validate_clause_block_id(&block.block_id)?.to_string();
        if !seen_replacements.insert(id.clone()) {
            return Err(format!("Повторяющийся идентификатор профильного блока: {id}"));
        }
        replacements.push((id, block.title.trim().to_string(), block.content));
    }

    let mut repo = repository_for(&default_state_db_path(&app)?)?;
    repo.replace_clause_blocks(&delete_ids, &replacements)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[derive(Debug, Deserialize)]
struct DeleteClauseBlockRequest {
    block_id: String,
}
#[tauri::command]
fn delete_clause_block(
    req: DeleteClauseBlockRequest,
    app: tauri::AppHandle,
) -> Result<Vec<ClauseBlockRecord>, String> {
    let id = validate_clause_block_id(&req.block_id)?;
    let repo = repository_for(&default_state_db_path(&app)?)?;
    repo.delete_clause_block(id).map_err(|e| e.to_string())?;
    repo.list_clause_blocks().map_err(|e| e.to_string())
}
