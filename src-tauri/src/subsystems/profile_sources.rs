// Profession-scoped source and prompt overrides. Universal orchestration remains in document_commands.

const MEDICAL_RVK_OPTIONS_BLOCK_ID: &str = "professional.medical.rvk.quick_options";
const MEDICAL_DIARY_PROGRAM_TEMPLATE_VERSION: &str = "v4";
const MEDICAL_DIARY_PROGRAM_TEMPLATE_TEXT: &str = concat!(
    "{{#each diaries}}\n",
    "{{diary.datetime}} {{diary.text}}\n",
    "{{diary.treating_physician_signature}}\n",
    "{{diary.department_head_signature}}\n",
    "\n",
    "{{/each}}\n",
);

fn stored_clause_block(app: &tauri::AppHandle, block_id: &str) -> Result<Option<String>, String> {
    let blocks = repository_for(&default_state_db_path(app)?)?
        .clause_blocks_map()
        .map_err(|error| error.to_string())?;
    Ok(blocks.get(block_id).cloned())
}

fn parse_profile_quick_options(content: &str) -> Vec<String> {
    let values = match serde_json::from_str::<Vec<String>>(content) {
        Ok(values) => values,
        Err(error) => {
            // Quick options are optional convenience data. Corruption here must
            // never brick document generation; the canonical prompt still allows
            // manual input and remains the source of truth.
            eprintln!("Повреждённые профильные быстрые варианты проигнорированы: {error}");
            return Vec::new();
        }
    };
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.to_lowercase()))
        .collect()
}

fn profile_quick_options(app: &tauri::AppHandle, block_id: &str) -> Result<Vec<String>, String> {
    let Some(content) = stored_clause_block(app, block_id)? else {
        return Ok(Vec::new());
    };
    Ok(parse_profile_quick_options(&content))
}

fn apply_profile_prompt_overrides(
    app: &tauri::AppHandle,
    plan: &mut WorkflowPlan,
) -> Result<(), String> {
    if let Some(prompt_index) = plan
        .prompts
        .iter()
        .position(|prompt| prompt.field_id == "medical.rvk_commissariat")
    {
        let rvk_options = profile_quick_options(app, MEDICAL_RVK_OPTIONS_BLOCK_ID)?;
        if !rvk_options.is_empty() {
            let prompt = &mut plan.prompts[prompt_index];
            prompt.options = rvk_options;
            prompt.allow_custom_option = true;
            prompt.help_text = Some(
                "Быстрые варианты заданы в медицинском профиле; можно ввести другое значение вручную."
                    .into(),
            );
        }
    }

    // The working donor applications ask the doctor for diary decisions (style,
    // intraday rhythm and the sick-leave/dynamic-epicrisis choice), but they do not
    // introduce separate mandatory questions for the technical time window. Keep the
    // generic series engine bounded internally. The frontend keeps
    // these internal prompts hidden but still submits their current values, so the
    // backend remains the single source of truth.
    let is_diary_plan = plan
        .prompts
        .iter()
        .any(|prompt| prompt.field_id == dokkomplekt_core::DIARY_SCHEDULE_STYLE)
        && plan
            .prompts
            .iter()
            .any(|prompt| prompt.field_id == dokkomplekt_core::DIARY_INTRADAY_RHYTHM);
    if is_diary_plan {
        for prompt in &mut plan.prompts {
            if prompt.field_id == dokkomplekt_core::DIARY_DAY_START_TIME {
                if prompt
                    .current_value
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                {
                    prompt.current_value = Some("00:00".into());
                }
                prompt.required = false;
                prompt.help_text = Some(
                    "Внутреннее начало суток для донорского ритма; пользовательский мастер его не спрашивает."
                        .into(),
                );
            } else if prompt.field_id == dokkomplekt_core::DIARY_DAY_END_TIME {
                if prompt
                    .current_value
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                {
                    prompt.current_value = Some("23:59".into());
                }
                prompt.required = false;
                prompt.help_text = Some(
                    "Внутренняя граница суток для донорского ритма; пользовательский мастер её не спрашивает."
                        .into(),
                );
            }
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

fn program_calendar_diary_template(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("profile-generated-templates");
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Не удалось подготовить локальный генератор дневников: {error}"))?;
    let path = root.join(format!(
        "medical-diary-program-calendar-{MEDICAL_DIARY_PROGRAM_TEMPLATE_VERSION}.docx"
    ));

    let usable = path.is_file()
        && validate_safe_template_file(&path).is_ok()
        && extract_docx_text(&path)
            .map(|text| {
                text.contains("{{#each diaries}}")
                    && text.contains("{{diary.datetime}}")
                    && text.contains("{{diary.text}}")
                    && text.contains("{{diary.treating_physician_signature}}")
                    && text.contains("{{diary.department_head_signature}}")
            })
            .unwrap_or(false);
    if !usable {
        create_docx_from_text(&path, MEDICAL_DIARY_PROGRAM_TEMPLATE_TEXT)
            .map_err(|error| format!("Не удалось создать локальный шаблон текстовых дневников: {error}"))?;
        validate_safe_template_file(&path)
            .map_err(|error| format!("Созданный шаблон дневников не прошёл проверку: {error}"))?;
    }
    Ok(path)
}

/// Normal medical diary generation follows the current donor Dokkomplekt path:
/// dates are produced by the program calendar from admission/discharge plus the
/// doctor's selected cadence; diary body text comes from the doctor-owned Texts
/// library. Numbered 01-31 files are legacy compatibility data and must never
/// replace the normal diary document during generation.
fn medical_diary_template_override(
    app: &tauri::AppHandle,
    _case: &SemanticCase,
    document: &DocumentTemplateSpec,
) -> Result<Option<PathBuf>, String> {
    if !is_medical_diary_document(document) {
        return Ok(None);
    }
    program_calendar_diary_template(app).map(Some)
}

#[cfg(test)]
mod profile_sources_tests {
    use super::parse_profile_quick_options;

    #[test]
    fn corrupted_optional_quick_options_do_not_block_the_profile() {
        assert!(parse_profile_quick_options("{broken json").is_empty());
    }

    #[test]
    fn quick_options_are_trimmed_and_deduplicated_case_insensitively() {
        let options = parse_profile_quick_options(
            r#"[" Ленинский ", "ленинский", "", "Сормовский"]"#,
        );
        assert_eq!(options, vec!["Ленинский", "Сормовский"]);
    }
}
