// Profession-scoped source and prompt overrides. Universal orchestration remains in document_commands.

const MEDICAL_RVK_OPTIONS_BLOCK_ID: &str = "professional.medical.rvk.quick_options";
const MEDICAL_DIARY_DATE_TEMPLATES_BLOCK_ID: &str = "professional.medical.diary.date_templates";

#[derive(Debug, Clone, Deserialize)]
struct MedicalDiaryDateTemplateReference {
    file_name: String,
    source_path: String,
}

fn stored_clause_block(app: &tauri::AppHandle, block_id: &str) -> Result<Option<String>, String> {
    let blocks = repository_for(&default_state_db_path(app)?)?
        .clause_blocks_map()
        .map_err(|error| error.to_string())?;
    Ok(blocks.get(block_id).cloned())
}

fn profile_quick_options(app: &tauri::AppHandle, block_id: &str) -> Result<Vec<String>, String> {
    let Some(content) = stored_clause_block(app, block_id)? else {
        return Ok(Vec::new());
    };
    let values = serde_json::from_str::<Vec<String>>(&content)
        .map_err(|error| format!("Профильные быстрые варианты повреждены: {error}"))?;
    let mut seen = BTreeSet::new();
    Ok(values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.to_lowercase()))
        .collect())
}

fn apply_profile_prompt_overrides(
    app: &tauri::AppHandle,
    plan: &mut WorkflowPlan,
) -> Result<(), String> {
    let rvk_options = profile_quick_options(app, MEDICAL_RVK_OPTIONS_BLOCK_ID)?;
    if !rvk_options.is_empty() {
        if let Some(prompt) = plan
            .prompts
            .iter_mut()
            .find(|prompt| prompt.field_id == "medical.rvk_commissariat")
        {
            prompt.options = rvk_options;
            prompt.allow_custom_option = true;
            prompt.help_text = Some(
                "Быстрые варианты заданы в медицинском профиле; можно ввести другое значение вручную."
                    .into(),
            );
        }
    }
    Ok(())
}

fn is_medical_diary_document(document: &DocumentTemplateSpec) -> bool {
    if !matches!(document.category, DomainKind::Medical) {
        return false;
    }
    let role = document.role_id.trim().to_lowercase();
    role == "diary"
        || role == "diaries"
        || role.ends_with(".diary")
        || role.ends_with(".diaries")
}

fn medical_diary_template_override(
    app: &tauri::AppHandle,
    case: &SemanticCase,
    document: &DocumentTemplateSpec,
) -> Result<Option<PathBuf>, String> {
    if !is_medical_diary_document(document) {
        return Ok(None);
    }
    let Some(content) = stored_clause_block(app, MEDICAL_DIARY_DATE_TEMPLATES_BLOCK_ID)? else {
        return Ok(None);
    };
    let entries = serde_json::from_str::<Vec<MedicalDiaryDateTemplateReference>>(&content)
        .map_err(|error| format!("Список медицинских шаблонов «Даты» повреждён: {error}"))?;
    if entries.is_empty() {
        return Ok(None);
    }
    let admission = case
        .get("medical.admission_date")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Для выбора шаблона «Даты» нужна дата поступления.".to_string())?;
    let normalized = parse_flexible_date(admission, current_year_utc())
        .ok_or_else(|| format!("Не удалось разобрать дату поступления для шаблонов «Даты»: {admission}"))?;
    let day = normalized
        .split('.')
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| "Не удалось определить день поступления для шаблонов «Даты».".to_string())?;
    let names = entries.iter().map(|entry| entry.file_name.clone()).collect::<Vec<_>>();
    let Some(selection) = select_diary_template_for_admission(&names, day) else {
        return Err(format!(
            "В наборе «Даты» нет подходящего шаблона для дня поступления {day:02}. Добавьте файл {day:02}.docx или совместимый D0+1-шаблон."
        ));
    };
    let selected = entries
        .iter()
        .find(|entry| entry.file_name == selection.file_name)
        .ok_or_else(|| "Выбранный шаблон «Даты» отсутствует в сохранённом наборе.".to_string())?;
    let path = PathBuf::from(&selected.source_path);
    if !path.is_file() {
        return Err(format!(
            "Сохранённый шаблон «Даты» больше недоступен: {}. Импортируйте папку 01–31 повторно.",
            path.display()
        ));
    }
    Ok(Some(path))
}
