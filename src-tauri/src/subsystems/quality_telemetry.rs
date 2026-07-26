#[derive(Debug, Clone, Serialize)]
struct QualityTelemetryBucket {
    key: String,
    count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct QualityRuleSuggestion {
    suggestion_id: String,
    title: String,
    reason: String,
    observations: u64,
    auto_enabled: bool,
    requires_specialist_confirmation: bool,
}

#[derive(Debug, Clone, Serialize)]
struct QualityTelemetryReport {
    generated_at: String,
    stop_reasons: Vec<QualityTelemetryBucket>,
    unrecognized_fields: Vec<QualityTelemetryBucket>,
    broken_templates: Vec<QualityTelemetryBucket>,
    excluded_documents: Vec<QualityTelemetryBucket>,
    repeated_confirmations: Vec<QualityTelemetryBucket>,
    suggestions: Vec<QualityRuleSuggestion>,
    privacy_mode: String,
}

fn increment_bucket(map: &mut BTreeMap<String, u64>, key: impl Into<String>) {
    let key = key.into().trim().to_string();
    if !key.is_empty() {
        *map.entry(key).or_default() += 1;
    }
}

fn buckets(map: BTreeMap<String, u64>) -> Vec<QualityTelemetryBucket> {
    let mut result = map
        .into_iter()
        .map(|(key, count)| QualityTelemetryBucket { key, count })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| right.count.cmp(&left.count).then_with(|| left.key.cmp(&right.key)));
    result
}

fn collect_json_strings(value: &serde_json::Value, key: &str, target: &mut BTreeMap<String, u64>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(item) = map.get(key) {
                match item {
                    serde_json::Value::String(value) => increment_bucket(target, value),
                    serde_json::Value::Array(values) => {
                        for value in values.iter().filter_map(serde_json::Value::as_str) {
                            increment_bucket(target, value);
                        }
                    }
                    _ => {}
                }
            }
            for nested in map.values() {
                collect_json_strings(nested, key, target);
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                collect_json_strings(nested, key, target);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Serialize)]
struct DailyAutomationDashboard {
    date_utc: String,
    processed_cases: usize,
    automatically_completed_cases: usize,
    attention_cases: usize,
    failed_cases: usize,
    generated_documents: usize,
    printed_documents: usize,
    measured_processing_milliseconds: u64,
}

fn json_collection_is_empty(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .is_none_or(|item| match item {
            serde_json::Value::Array(values) => values.is_empty(),
            serde_json::Value::Object(values) => values.is_empty(),
            serde_json::Value::Null => true,
            _ => false,
        })
}

#[tauri::command]
fn get_daily_automation_dashboard(app: tauri::AppHandle) -> Result<DailyAutomationDashboard, String> {
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let today = OffsetDateTime::now_utc().date().to_string();
    let cases = repo.list_case_runs(2_000).map_err(|error| error.to_string())?;
    let audit = repo.list_audit_events(5_000).map_err(|error| error.to_string())?;
    let terminal = ["attention", "completed", "failed", "cancelled"];
    let mut report = DailyAutomationDashboard {
        date_utc: today.clone(),
        processed_cases: 0,
        automatically_completed_cases: 0,
        attention_cases: 0,
        failed_cases: 0,
        generated_documents: 0,
        printed_documents: 0,
        measured_processing_milliseconds: 0,
    };
    for case in cases.iter().filter(|case| case.updated_at.starts_with(&today)) {
        if terminal.contains(&case.status.as_str()) {
            report.processed_cases += 1;
        }
        if case.status == "attention" {
            report.attention_cases += 1;
        }
        if case.status == "failed" {
            report.failed_cases += 1;
        }
        let created_files = serde_json::from_str::<Vec<String>>(&case.created_files_json).unwrap_or_default();
        report.generated_documents += created_files.len();
        if case.status == "completed" {
            let request = serde_json::from_str::<serde_json::Value>(&case.request_json)
                .unwrap_or(serde_json::Value::Null);
            if json_collection_is_empty(&request, "confirmed_fields")
                && json_collection_is_empty(&request, "confirmed_document_ids")
            {
                report.automatically_completed_cases += 1;
            }
        }
        let started = OffsetDateTime::parse(
            &case.created_at,
            &time::format_description::well_known::Rfc3339,
        );
        let finished = OffsetDateTime::parse(
            &case.updated_at,
            &time::format_description::well_known::Rfc3339,
        );
        if let (Ok(started), Ok(finished)) = (started, finished) {
            let milliseconds = (finished - started).whole_milliseconds().max(0) as u64;
            report.measured_processing_milliseconds = report
                .measured_processing_milliseconds
                .saturating_add(milliseconds);
        }
    }
    for event in audit.iter().filter(|event| event.created_at.starts_with(&today)) {
        if !matches!(
            event.event_type.as_str(),
            "manual_print_queued" | "automatic_print_queued_after_triage"
        ) {
            continue;
        }
        let details = serde_json::from_str::<serde_json::Value>(&event.detail_json)
            .unwrap_or(serde_json::Value::Null);
        let count = details
            .get("printed_files")
            .or_else(|| details.get("successful_files"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            .unwrap_or(1);
        report.printed_documents = report.printed_documents.saturating_add(count);
    }
    Ok(report)
}

#[tauri::command]
fn get_quality_telemetry(app: tauri::AppHandle) -> Result<QualityTelemetryReport, String> {
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let exceptions = repo.list_exceptions(true).map_err(|error| error.to_string())?;
    let audit = repo.list_audit_events(1_000).map_err(|error| error.to_string())?;
    let mut stop_reasons = BTreeMap::new();
    let mut unrecognized_fields = BTreeMap::new();
    let mut broken_templates = BTreeMap::new();
    let mut excluded_documents = BTreeMap::new();
    let mut repeated_confirmations = BTreeMap::new();

    for exception in &exceptions {
        increment_bucket(&mut stop_reasons, &exception.category);
        if let Ok(details) = serde_json::from_str::<serde_json::Value>(&exception.details_json) {
            for key in ["missing_fields", "missing", "rejected_fields", "unrecognized_fields"] {
                collect_json_strings(&details, key, &mut unrecognized_fields);
            }
            for key in ["document_id", "broken_template_id", "template_id"] {
                if exception.category.contains("template") {
                    collect_json_strings(&details, key, &mut broken_templates);
                }
            }
            for key in ["excluded_document_ids", "excluded_documents"] {
                collect_json_strings(&details, key, &mut excluded_documents);
            }
        }
    }

    for event in &audit {
        let details = serde_json::from_str::<serde_json::Value>(&event.detail_json)
            .unwrap_or(serde_json::Value::Null);
        match event.event_type.as_str() {
            "risk_values_batch_confirmed" => {
                collect_json_strings(&details, "fields", &mut repeated_confirmations);
            }
            "document_bundle_confirmed" => {
                collect_json_strings(&details, "document_ids", &mut repeated_confirmations);
            }
            "template_regression_acknowledged" | "template_regression_blocked" => {
                collect_json_strings(&details, "document_id", &mut broken_templates);
            }
            "document_bundle_decided" => {
                collect_json_strings(&details, "excluded_document_ids", &mut excluded_documents);
            }
            _ => {}
        }
    }

    let confirmation_buckets = buckets(repeated_confirmations);
    let mut suggestions = confirmation_buckets
        .iter()
        .filter(|bucket| bucket.count >= 3)
        .map(|bucket| QualityRuleSuggestion {
            suggestion_id: format!("confirmation:{}", bucket.key),
            title: format!("Предложить правило для «{}»", bucket.key),
            reason: format!(
                "Специалист повторил одно и то же подтверждение {} раз. Создание правила возможно только после явного подтверждения.",
                bucket.count
            ),
            observations: bucket.count,
            auto_enabled: false,
            requires_specialist_confirmation: true,
        })
        .collect::<Vec<_>>();
    suggestions.sort_by_key(|item| std::cmp::Reverse(item.observations));

    Ok(QualityTelemetryReport {
        generated_at: OffsetDateTime::now_utc().to_string(),
        stop_reasons: buckets(stop_reasons),
        unrecognized_fields: buckets(unrecognized_fields),
        broken_templates: buckets(broken_templates),
        excluded_documents: buckets(excluded_documents),
        repeated_confirmations: confirmation_buckets,
        suggestions,
        privacy_mode: "local_aggregate_only_no_document_text".into(),
    })
}
