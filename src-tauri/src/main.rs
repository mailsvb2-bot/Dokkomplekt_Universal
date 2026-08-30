mod central_queue;
mod component_manager;
mod generation_publication;
mod privacy_runtime;
mod reference_data_update;
mod resume_engine;
mod semantic_model;
mod semantic_runtime;
mod state_transaction;
mod template_snapshot;
mod threshold_calibration;
mod universal_intake;
mod workspace_hygiene;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use dokkomplekt_core::{
    analyze_template_text, analyze_template_text_with_domain_hint,
    apply_model_consensus_with_source, apply_model_output_with_source, apply_popup_answers,
    apply_scanner_marks, attention_file_name, build_corpus_entry, build_diary_plan,
    build_extraction_prompt_for_domain_and_language, build_merged_popup_plan, build_series_plan,
    case_for_mail_merge_row, corpus_entry_metrics, create_pack_from_confirmations,
    decide_document_bundle, decision_for_key, detect_field_conflict,
    document_pack_contains_template_source, empty_first_run_pack, evaluate_automation_quality,
    evaluate_print_triage_with_thresholds, extract_understanding, format_counter_value,
    is_valid_field_id, merge_document_pack, missing_medical_template_render_paths,
    normalize_popup_fields, parse_delimited_table, parse_source_text, plan_created_documents_batch,
    plan_output_paths, plan_workflow_batch, prepare_template_confirmations_with_existing_pack,
    recommend_document_bundle, remove_document_button as remove_button_from_pack,
    rename_document_button as rename_button_in_pack, render_text_template, route_intake_event,
    run_universal_constructor_pipeline, sanitize_path_component, segment_case_fragments,
    set_user_value, suggest_icd10, suggest_template_markup, template_counter_requests,
    template_image_requests, validate_case_relations, validate_field_value,
    validate_output_button_labels, validate_popup_fields, BundleDecision, CaseFragment,
    ConfiguredDocument, CorpusAcceptanceSource, CorpusEntry, CorpusEntryMetrics,
    CorpusEntryRequest, CreatedDocumentsBatch, DocumentPack, DocumentRoutingRecommendation,
    DocumentTemplateSpec, DomainKind, ExtractedField, FolderNamePart, IntakeDecision,
    IntakeDeduplicator, KitLearningDecision, KitPromotionPolicy, KitRuleKey, MailMergeTable,
    ParsedSourceReport, PopupAnswer, PopupApplyResult, PopupFieldConfig, PrintTriageReport,
    ProductPlanId, ScannerMark, SemanticCase, SeriesPlanRequest, TemplateCandidate,
    TemplateConfirmationRow, TemplateLearningInput, TemplateLearningReport,
    TemplateMarkupCandidate, UniversalPipelineFlags, UniversalPipelineInput, ValueSource,
    WorkflowFlags, WorkflowPlan, EXPIRED_DEMO_WATERMARK_TEXT, TRIAL_WATERMARK_TEXT,
};
use dokkomplekt_docx::{
    apply_template_learning_map_file, apply_template_markup_file, compare_docx_structures,
    compile_labeled_template_file, create_docx_from_text, extract_docx_text,
    extract_docx_text_from_bytes, inject_docx_images, render_docx_file_with_watermark_proof,
    validate_safe_template_file, RenderedDocxProof, TemplateLearningMapField,
    TemplateLearningMapReport, TemplateMarkupReplacement, TemplateMarkupReport,
    TemplateRegressionReport,
};
use dokkomplekt_license_core::{
    evaluate_access as evaluate_signed_access, max_documents_per_run as signed_run_limit,
    verify_license_document_now, AccessRequest as SignedAccessRequest,
    AccessStatus as SignedAccessStatus, LicenseDocument, MachineFacts, MachineFingerprint,
    PlanId as SignedPlanId, UsageLedger, WatermarkMode,
};
use dokkomplekt_storage::{
    AuditEventRecord, AutomationExceptionRecord, AutomationMetrics, CaseDocumentRecord,
    CaseRunRecord, ClauseBlockRecord, CounterValue, DesktopSnapshotPublication, LocalRepository,
    TemplateVersionDraft, TemplateVersionRecord, UsageReservation,
};
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey};
use notify::{RecursiveMode, Watcher as _};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read as _, Write as _};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{Emitter, Manager, State};
use time::OffsetDateTime;
use uuid::Uuid;

#[cfg(test)]
use generation_publication::{
    local_completion_receipt, local_completion_receipt_matches, mark_local_completion,
};
use privacy_runtime::{
    cleanup_intake_workspace, load_privacy_preferences, lock_learning_workspace,
    persist_privacy_preferences, start_periodic_intake_cleanup, PrivacyPreferences,
};
use semantic_model::{
    LocalSemanticModelConfig, LocalSemanticModelStatus, LocalSemanticModelTransport,
};
use state_transaction::transact_default_state;
use workspace_hygiene::{WorkspaceHygieneReport, WorkspaceRetentionPolicy};

/// Install the explicitly selected rustls crypto backend before any reqwest
/// client is created. The operation is process-global and safely idempotent.
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Ed25519 public key the desktop build trusts for license verification.
///
/// The trust anchor is baked into the binary at compile time — it is **never**
/// accepted from the UI in release builds, otherwise anyone could "verify" a
/// self-signed license. Production installers are built with
/// `DOKKOMPLEKT_LICENSE_PUBKEY_B64=<issuer public key>` in the environment
/// (`scripts/generate_license_keypair.py` produces a pair). The fallback below
/// is a documentation-only key whose private half was destroyed, so unofficial
/// builds fail closed: no license can ever validate against it.
const TRUSTED_LICENSE_PUBKEY_B64: &str = match option_env!("DOKKOMPLEKT_LICENSE_PUBKEY_B64") {
    Some(key) => key,
    None => "Wxq3/5yQAVAUwQu+y+h3mQCYxypmOvMrWb81ms+Mqs8=",
};
const LICENSE_TRUST_ANCHOR_IS_CONFIGURED: bool =
    option_env!("DOKKOMPLEKT_LICENSE_PUBKEY_B64").is_some();

/// A separate trust anchor is used for software updates. It is deliberately not
/// shared with licensing and is never accepted as a command argument from the UI.
const TRUSTED_UPDATE_PUBKEY_B64: &str = match option_env!("DOKKOMPLEKT_UPDATE_PUBKEY_B64") {
    Some(key) => key,
    None => "jIswwPnOeUrKVFTPi9vZ9ZM7roY3iO2xXw0vWMSyVFY=",
};

/// Separate Ed25519 trust anchor for corpus-derived print thresholds. A package
/// cannot nominate its own trusted key; release builds inject the issuer key.
const TRUSTED_THRESHOLD_PUBKEY_B64: &str = match option_env!("DOKKOMPLEKT_THRESHOLD_PUBKEY_B64") {
    Some(key) => key,
    None => "Oo+mU6FHFQa77t2FvaMG2XDVq986RFYrSCpUoXxbKQw=",
};
const TRUSTED_UPDATE_MANIFEST_URL: &str = match option_env!("DOKKOMPLEKT_UPDATE_MANIFEST_URL") {
    Some(url) => url,
    None => "https://updates.dokkomplekt.invalid/update-manifest.json",
};
const MAX_UPDATE_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_UPDATE_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

/// Default per-user state database name, resolved under the app data dir.
const DEFAULT_STATE_DB: &str = "dokkomplekt-user-state.sqlite";
const MAX_DOCX_BYTES: usize = 50 * 1024 * 1024;

include!("subsystems/update_runtime.rs");
include!("subsystems/automation_consistency.rs");
include!("subsystems/automation_mail_merge.rs");

struct WatcherHandle {
    stop: Arc<AtomicBool>,
    folder: PathBuf,
}

const LEARNED_SCANNER_RULES_STATE_KEY: &str = "learned_scanner_rules_v1";

#[derive(Debug, Clone)]
struct WordScannerSessionState {
    session_id: String,
    mode: String,
    opened_path: PathBuf,
    working_copy: bool,
    word_was_running: bool,
    last_capture: Option<WordScannerCaptureInternal>,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone)]
struct WordScannerCaptureInternal {
    selected_text: String,
    context_text: String,
    before_text: String,
    after_text: String,
    selection_start: i64,
    selection_end: i64,
    expanded_from_cursor: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LearnedScannerRule {
    rule_id: String,
    field_id: String,
    title: String,
    label_hint: String,
    before_text: String,
    after_text: String,
    sample_value: String,
    input_kind: String,
    created_at: String,
    #[serde(default)]
    layout_fingerprint: Option<String>,
    #[serde(default)]
    successful_applications: u32,
    #[serde(default)]
    last_applied_at: Option<String>,
    #[serde(default = "default_learning_status")]
    learning_status: String,
    #[serde(default)]
    shadow_observations: u32,
    #[serde(default)]
    shadow_agreements: u32,
    #[serde(default)]
    shadow_conflicts: u32,
    #[serde(default)]
    promoted_at: Option<String>,
}

fn default_learning_status() -> String {
    // Existing installations applied learned rules immediately. Preserve those
    // records as promoted while every newly created rule starts in shadow mode.
    "promoted".into()
}

const SEMANTIC_MODEL_CONFIG_STATE_KEY: &str = "local_semantic_model_config_v1";

fn load_semantic_model_config(app: &tauri::AppHandle) -> Result<LocalSemanticModelConfig, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    Ok(repo
        .load_state_value::<LocalSemanticModelConfig>(SEMANTIC_MODEL_CONFIG_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default())
}

fn persist_semantic_model_config(
    app: &tauri::AppHandle,
    config: &LocalSemanticModelConfig,
) -> Result<(), String> {
    config.validate()?;
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(SEMANTIC_MODEL_CONFIG_STATE_KEY, config)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
struct SemanticModelConfigurationResponse {
    config: LocalSemanticModelConfig,
    status: LocalSemanticModelStatus,
}

fn semantic_model_configuration_response(
    config: LocalSemanticModelConfig,
    effective: LocalSemanticModelConfig,
) -> SemanticModelConfigurationResponse {
    let status = match LocalSemanticModelTransport::new(&effective) {
        Ok(transport) if config.enabled => transport.status(),
        Ok(_) => LocalSemanticModelStatus {
            configured: true,
            reachable: false,
            provider: config.provider.clone(),
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            available_models: Vec::new(),
            message: "Локальная SemanticModel настроена, но отключена.".into(),
        },
        Err(error) => LocalSemanticModelStatus {
            configured: false,
            reachable: false,
            provider: config.provider.clone(),
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            available_models: Vec::new(),
            message: error,
        },
    };
    SemanticModelConfigurationResponse { config, status }
}

#[tauri::command]
fn get_semantic_model_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SemanticModelConfigurationResponse, String> {
    let config = load_semantic_model_config(&app)?;
    let effective = semantic_runtime::effective_config(&state.semantic_runtime, &config)?;
    Ok(semantic_model_configuration_response(config, effective))
}

#[derive(Debug, Deserialize)]
struct UpdateSemanticModelConfigRequest {
    config: LocalSemanticModelConfig,
}

#[tauri::command]
fn update_semantic_model_config(
    req: UpdateSemanticModelConfigRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SemanticModelConfigurationResponse, String> {
    persist_semantic_model_config(&app, &req.config)?;
    append_audit_event(
        &app,
        "semantic_model_config_updated",
        "",
        &serde_json::json!({
            "enabled": req.config.enabled,
            "provider": &req.config.provider,
            "endpoint": &req.config.endpoint,
            "model": &req.config.model,
            "shadow_mode": req.config.shadow_mode,
            "auto_apply_zero_touch": req.config.auto_apply_zero_touch,
        }),
    )?;
    let effective = semantic_runtime::effective_config(&state.semantic_runtime, &req.config)?;
    Ok(semantic_model_configuration_response(req.config, effective))
}

#[tauri::command]
fn test_semantic_model(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<LocalSemanticModelStatus, String> {
    let config = load_semantic_model_config(&app)?;
    if !config.enabled {
        return Err("Сначала включите локальную SemanticModel в настройках.".into());
    }
    let effective = semantic_runtime::effective_config(&state.semantic_runtime, &config)?;
    Ok(LocalSemanticModelTransport::new(&effective)?.status())
}

#[tauri::command]
fn get_calibrated_threshold_status(
    app: tauri::AppHandle,
) -> Result<Vec<threshold_calibration::CalibratedThresholdStatus>, String> {
    threshold_calibration::list_statuses(&app)
}

#[tauri::command]
fn import_calibrated_thresholds(
    req: threshold_calibration::ImportCalibratedThresholdsRequest,
    app: tauri::AppHandle,
) -> Result<threshold_calibration::CalibratedThresholdStatus, String> {
    threshold_calibration::import_package(&app, req)
}

#[tauri::command]
fn get_reference_data_status(
    app: tauri::AppHandle,
) -> Result<reference_data_update::ReferenceDataStatus, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    reference_data_update::status(&app_data)
}

#[tauri::command]
fn update_reference_data(
    app: tauri::AppHandle,
) -> Result<reference_data_update::ReferenceDataStatus, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let status = reference_data_update::download_and_install(&app_data)?;
    append_audit_event(
        &app,
        "reference_data_updated",
        "production_calendar_ru",
        &serde_json::to_value(&status).map_err(|error| error.to_string())?,
    )?;
    Ok(status)
}

#[derive(Debug, Deserialize)]
struct ImportReferenceDataRequest {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    bytes_base64: Option<String>,
}

#[tauri::command]
fn import_reference_data(
    req: ImportReferenceDataRequest,
    app: tauri::AppHandle,
) -> Result<reference_data_update::ReferenceDataStatus, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let status = match (req.path.as_deref(), req.bytes_base64.as_deref()) {
        (Some(path), None) if !path.trim().is_empty() => {
            let source = resolve_user_path(&app, path)?;
            reference_data_update::import_package(&app_data, &source)?
        }
        (None, Some(encoded)) => {
            let file_name = req.file_name.as_deref().unwrap_or("calendar.signed.json");
            if !file_name.to_ascii_lowercase().ends_with(".json") {
                return Err("Подписанный календарный пакет должен быть JSON-файлом.".into());
            }
            let bytes = BASE64_STANDARD
                .decode(encoded.trim())
                .map_err(|_| "Пакет календаря содержит некорректный base64".to_string())?;
            reference_data_update::import_package_bytes(&app_data, &bytes)?
        }
        _ => {
            return Err(
                "Укажите либо безопасный путь, либо байты подписанного календарного пакета.".into(),
            )
        }
    };
    append_audit_event(
        &app,
        "reference_data_imported",
        "production_calendar_ru",
        &serde_json::to_value(&status).map_err(|error| error.to_string())?,
    )?;
    Ok(status)
}

fn create_automation_exception(
    app: &tauri::AppHandle,
    category: &str,
    source_path: &str,
    message: &str,
    details: &serde_json::Value,
) -> Result<AutomationExceptionRecord, String> {
    repository_for(&default_state_db_path(app)?)?
        .create_exception(category, source_path, message, &details.to_string())
        .map_err(|error| error.to_string())
}

fn append_audit_event(
    app: &tauri::AppHandle,
    event_type: &str,
    object_hash: &str,
    details: &serde_json::Value,
) -> Result<AuditEventRecord, String> {
    let mut repo = repository_for(&default_state_db_path(app)?)?;
    repo.append_audit_event(event_type, object_hash, &details.to_string())
        .map_err(|error| error.to_string())
}

fn increment_metric(app: &tauri::AppHandle, metric: &str, amount: u64) {
    if let Ok(path) = default_state_db_path(app) {
        if let Ok(repo) = repository_for(&path) {
            let _ = repo.increment_metric(metric, amount);
        }
    }
}

struct CaseRunTracker<'a> {
    app: &'a tauri::AppHandle,
    case_id: String,
    terminal: bool,
}

impl<'a> CaseRunTracker<'a> {
    fn start(
        app: &'a tauri::AppHandle,
        source_sha256: &str,
        processing_fingerprint: &str,
        source_path: &Path,
        request: &CreatedDocumentsIntakeRequest,
    ) -> Result<Self, String> {
        let request_json = serde_json::to_string(request).map_err(|error| error.to_string())?;
        let record = repository_for(&default_state_db_path(app)?)?
            .start_case_run(
                source_sha256,
                processing_fingerprint,
                &source_path.display().to_string(),
                &request_json,
                &request.output_root,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            app,
            case_id: record.case_id,
            terminal: false,
        })
    }

    fn case_id(&self) -> &str {
        &self.case_id
    }

    fn update_source_path(&self, source_path: &Path) -> Result<(), String> {
        let updated = repository_for(&default_state_db_path(self.app)?)?
            .update_case_run_source_path(&self.case_id, &source_path.display().to_string())
            .map_err(|error| error.to_string())?;
        if updated {
            Ok(())
        } else {
            Err("Состояние дела исчезло из локальной базы.".into())
        }
    }

    fn transition(&self, status: &str) -> Result<(), String> {
        let updated = repository_for(&default_state_db_path(self.app)?)?
            .update_case_run(&self.case_id, status, None, "[]", "[]", None)
            .map_err(|error| error.to_string())?;
        if updated {
            Ok(())
        } else {
            Err("Состояние дела исчезло из локальной базы.".into())
        }
    }

    fn finish(
        &mut self,
        status: &str,
        patient_folder: Option<&Path>,
        created_files: &[String],
        missing: &[String],
        last_error: Option<&str>,
    ) -> Result<(), String> {
        let created_json =
            serde_json::to_string(created_files).map_err(|error| error.to_string())?;
        let missing_json = serde_json::to_string(missing).map_err(|error| error.to_string())?;
        let patient_folder = patient_folder.map(|path| path.display().to_string());
        let updated = repository_for(&default_state_db_path(self.app)?)?
            .update_case_run(
                &self.case_id,
                status,
                patient_folder.as_deref(),
                &created_json,
                &missing_json,
                last_error,
            )
            .map_err(|error| error.to_string())?;
        if !updated {
            return Err("Состояние дела исчезло из локальной базы.".into());
        }
        self.terminal = true;
        Ok(())
    }

    fn mark_business_terminal(&mut self) {
        self.terminal = true;
    }
}

impl Drop for CaseRunTracker<'_> {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        if let Ok(path) = default_state_db_path(self.app) {
            if let Ok(repo) = repository_for(&path) {
                let _ = repo.update_case_run(
                    &self.case_id,
                    "failed",
                    None,
                    "[]",
                    "[]",
                    Some("Обработка прервана до безопасной публикации; исходник можно повторить."),
                );
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceProvenance {
    source_name: String,
    source_sha256: String,
}

impl SourceProvenance {
    fn from_bytes(source_name: &str, bytes: &[u8]) -> Self {
        Self {
            source_name: sanitize_source_name(source_name),
            source_sha256: hex::encode(Sha256::digest(bytes)),
        }
    }

    fn from_sha256(source_name: &str, source_sha256: &str) -> Result<Self, String> {
        if !is_sha256_hex(source_sha256) {
            return Err("Источник не содержит проверяемый SHA-256.".into());
        }
        Ok(Self {
            source_name: sanitize_source_name(source_name),
            source_sha256: source_sha256.to_ascii_lowercase(),
        })
    }
}

fn sanitize_source_name(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let compact = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let shortened = compact.chars().take(240).collect::<String>();
    if shortened.is_empty() {
        "источник".into()
    } else {
        shortened
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

struct AppState {
    semantic_case: Mutex<SemanticCase>,
    pack: Mutex<DocumentPack>,
    intake_dedup: Mutex<IntakeDeduplicator>,
    db_path: Mutex<Option<PathBuf>>,
    watcher: Mutex<Option<WatcherHandle>>,
    instance_lock: Mutex<Option<PathBuf>>,
    license_document: Mutex<Option<LicenseDocument>>,
    word_scanner: Mutex<Option<WordScannerSessionState>>,
    word_scanner_source_session: Mutex<Option<universal_intake::UploadedSourceSession>>,
    retained_uploaded_source: Mutex<Option<universal_intake::RetainedUploadedSource>>,
    source_provenance: Mutex<Option<SourceProvenance>>,
    semantic_runtime: Mutex<Option<semantic_runtime::ManagedSemanticRuntime>>,
    persistence_gate: Mutex<()>,
    persistence_blocked: AtomicBool,
    persistence_error: Mutex<Option<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            semantic_case: Mutex::new(SemanticCase::default()),
            pack: Mutex::new(empty_first_run_pack("default", "Пользовательские шаблоны")),
            intake_dedup: Mutex::new(IntakeDeduplicator::new(Duration::from_secs(3))),
            db_path: Mutex::new(None),
            watcher: Mutex::new(None),
            instance_lock: Mutex::new(None),
            license_document: Mutex::new(None),
            word_scanner: Mutex::new(None),
            word_scanner_source_session: Mutex::new(None),
            retained_uploaded_source: Mutex::new(None),
            source_provenance: Mutex::new(None),
            semantic_runtime: Mutex::new(None),
            persistence_gate: Mutex::new(()),
            persistence_blocked: AtomicBool::new(false),
            persistence_error: Mutex::new(None),
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Ok(slot) = self.instance_lock.get_mut() {
            if let Some(path) = slot.take() {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Ok(slot) = self.word_scanner.get_mut() {
            if let Some(session) = slot.take() {
                let _ = close_word_document(&session.opened_path, session.word_was_running, false);
                if session.working_copy {
                    let _ = std::fs::remove_file(session.opened_path);
                }
            }
        }
        if let Ok(slot) = self.word_scanner_source_session.get_mut() {
            let _ = slot.take();
        }
        if let Ok(slot) = self.retained_uploaded_source.get_mut() {
            let _ = slot.take();
        }
    }
}

#[cfg(target_os = "windows")]
fn process_is_alive(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let filter = format!("PID eq {pid}");
    std::process::Command::new("tasklist.exe")
        .arg("/FI")
        .arg(filter)
        .arg("/FO")
        .arg("CSV")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn process_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[derive(Debug, PartialEq, Eq)]
enum InstanceLockOutcome {
    Acquired(PathBuf),
    AlreadyRunning,
}

const ACTIVATION_TEMP_MAX_AGE: Duration = Duration::from_secs(5 * 60);

fn activation_queue_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("activation-requests"))
}

fn cleanup_activation_queue(queue_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(queue_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let temporary = path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.ends_with(".tmp"));
        let stale_regular_file = std::fs::symlink_metadata(&path)
            .ok()
            .filter(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= ACTIVATION_TEMP_MAX_AGE);
        if temporary && stale_regular_file {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn enqueue_activation_request(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let queue_dir = activation_queue_dir(app)?;
    std::fs::create_dir_all(&queue_dir).map_err(|error| error.to_string())?;
    let request_id = Uuid::new_v4();
    let temporary = queue_dir.join(format!(".{request_id}.tmp"));
    let final_path = queue_dir.join(format!("{request_id}.request"));
    let mut request = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Не удалось создать запрос активации окна: {error}"))?;
    writeln!(request, "pid={}", std::process::id())
        .map_err(|error| format!("Не удалось записать запрос активации окна: {error}"))?;
    request
        .sync_all()
        .map_err(|error| format!("Не удалось синхронизировать запрос активации окна: {error}"))?;
    drop(request);
    std::fs::rename(&temporary, &final_path)
        .map_err(|error| format!("Не удалось опубликовать запрос активации окна: {error}"))?;
    Ok(final_path)
}

fn restore_main_window(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}

fn start_activation_listener(app: tauri::AppHandle) -> Result<(), String> {
    let queue_dir = activation_queue_dir(&app)?;
    std::fs::create_dir_all(&queue_dir).map_err(|error| error.to_string())?;
    cleanup_activation_queue(&queue_dir);
    std::thread::spawn(move || loop {
        let mut activate = false;
        if let Ok(entries) = std::fs::read_dir(&queue_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let request = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.ends_with(".request"));
                let regular_file = std::fs::symlink_metadata(&path)
                    .ok()
                    .is_some_and(|metadata| {
                        metadata.file_type().is_file() && !metadata.file_type().is_symlink()
                    });
                if request && regular_file && std::fs::remove_file(path).is_ok() {
                    activate = true;
                }
            }
        }
        if activate {
            restore_main_window(&app);
        }
        std::thread::sleep(Duration::from_millis(150));
    });
    Ok(())
}

fn acquire_instance_lock(
    app: &tauri::AppHandle,
    background: bool,
) -> Result<InstanceLockOutcome, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let mode = if background { "watcher" } else { "ui" };
    let path = data_dir.join(format!("dokkomplekt-{mode}.instance"));
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write as _;
                writeln!(file, "{}", std::process::id()).map_err(|e| e.to_string())?;
                return Ok(InstanceLockOutcome::Acquired(path));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing_pid = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok());
                if existing_pid.map(process_is_alive).unwrap_or(false) {
                    return Ok(InstanceLockOutcome::AlreadyRunning);
                }
                std::fs::remove_file(&path).map_err(|e| {
                    format!("Не удалось удалить устаревшую блокировку экземпляра: {e}")
                })?;
            }
            Err(error) => {
                return Err(format!("Не удалось создать блокировку экземпляра: {error}"));
            }
        }
    }
    Err("Не удалось установить блокировку экземпляра.".into())
}

fn cleanup_stale_stage_directories(root: &Path, max_age: Duration) -> Result<usize, String> {
    if !root.exists() {
        return Ok(0);
    }
    let now = std::time::SystemTime::now();
    let mut removed = 0usize;
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(".dokkomplekt-stage-")
            && !name.starts_with(".dokkomplekt-manual-stage-")
            && !name.starts_with(".mail-merge-stage-")
            && !name.starts_with(".kedo-stage-")
        {
            continue;
        }
        let old_enough = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if old_enough && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn reject_parent_traversal(path: &Path) -> Result<(), String> {
    use std::path::Component;
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        Err(format!(
            "Путь содержит запрещённый переход «..»: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

/// Resolve a path that must stay under the application-data directory.
/// Absolute paths and parent traversal are rejected by contract.
fn resolve_under_app_data(app: &tauri::AppHandle, raw: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(raw.trim());
    if candidate.as_os_str().is_empty() {
        return Err("Пустой путь приложения".into());
    }
    if candidate.is_absolute() {
        return Err(format!(
            "Ожидался относительный путь внутри app_data, получен абсолютный: {}",
            candidate.display()
        ));
    }
    reject_parent_traversal(&candidate)?;
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(base.join(candidate))
}

/// Resolve a user-selected input/output path. Absolute paths are intentionally
/// allowed; relative paths are anchored under app_data for legacy/internal flows.
/// User-visible output/watch roots must use `resolve_user_visible_absolute_path`
/// instead so the UI can never display a relative path that silently lands in app_data.
fn resolve_user_path(app: &tauri::AppHandle, raw: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(raw.trim());
    if candidate.as_os_str().is_empty() {
        return Err("Пустой путь".into());
    }
    reject_parent_traversal(&candidate)?;
    if candidate.is_absolute() {
        Ok(candidate)
    } else {
        resolve_under_app_data(app, raw)
    }
}

fn resolve_user_visible_absolute_path(raw: &str, label: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(raw.trim());
    if candidate.as_os_str().is_empty() {
        return Err(format!("{label} не указан."));
    }
    reject_parent_traversal(&candidate)?;
    if !candidate.is_absolute() {
        return Err(format!(
            "{label} должен быть абсолютным путём, выбранным на компьютере: {}",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn default_state_db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_under_app_data(app, DEFAULT_STATE_DB)
}

include!("subsystems/desktop_io.rs");

/// Opens local storage with authenticated encryption for semantic-case data.
///
/// A deployment can supply `DOKKOMPLEKT_LOCAL_DATA_KEY_B64` (32 random bytes,
/// base64-encoded). Otherwise the desktop app creates a random installation-local
/// key next to the database. On Windows the key file is protected with user-bound
/// DPAPI; on Unix it is restricted to mode 0600. The key is never stored inside
/// SQLite or written to logs. Existing plaintext rows remain readable and migrate
/// to encrypted form on the next save.
fn local_data_key_for(path: &Path) -> Result<[u8; 32], String> {
    if let Some(encoded) = std::env::var("DOKKOMPLEKT_LOCAL_DATA_KEY_B64")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let decoded = BASE64_STANDARD
            .decode(encoded)
            .map_err(|error| format!("Некорректный ключ защиты локальных данных: {error}"))?;
        decoded.try_into().map_err(|_| {
            "Ключ защиты локальных данных должен содержать ровно 32 байта после base64-декодирования."
                .to_string()
        })
    } else {
        load_or_create_local_data_key(path)
    }
}

fn repository_for(path: &Path) -> Result<LocalRepository, String> {
    LocalRepository::open_with_key(path, local_data_key_for(path)?)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
const DPAPI_KEY_FILE_MAGIC: &[u8] = b"DKDPAPI1\0";

#[cfg(any(windows, test))]
fn raw_key_backup_candidates(key_path: &Path) -> Result<Vec<PathBuf>, String> {
    let parent = key_path
        .parent()
        .ok_or_else(|| "Путь локального ключа не имеет родительской папки.".to_string())?;
    let key_name = key_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Имя локального ключа не является допустимым UTF-8.".to_string())?;
    let prefix = format!("{key_name}.raw.");
    let mut backups = Vec::new();
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(backups),
        Err(error) => {
            return Err(format!(
                "Не удалось проверить резервные копии локального ключа {}: {error}",
                parent.display()
            ))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Не удалось прочитать запись папки локального ключа {}: {error}",
                parent.display()
            )
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".bak") {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Не удалось проверить резервную копию локального ключа {}: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Резервная копия локального ключа имеет недопустимый тип: {}",
                path.display()
            ));
        }
        backups.push(path);
    }
    backups.sort();
    Ok(backups)
}

#[cfg(any(windows, test))]
fn recover_interrupted_key_migration(key_path: &Path) -> Result<(), String> {
    let backups = raw_key_backup_candidates(key_path)?;
    match std::fs::symlink_metadata(key_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(format!(
                    "Локальный ключ имеет недопустимый тип: {}",
                    key_path.display()
                ));
            }
            // Do not delete raw backups until the primary key has been decoded
            // successfully. A present-but-corrupt primary file must not destroy
            // the only recoverable copy.
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match backups.as_slice() {
            [] => Ok(()),
            [backup] => std::fs::rename(backup, key_path).map_err(|error| {
                format!(
                    "Обнаружен прерванный перенос локального ключа, но не удалось восстановить backup {}: {error}",
                    backup.display()
                )
            }),
            _ => Err(format!(
                "Найдено несколько резервных копий локального ключа ({}); автоматический выбор небезопасен.",
                backups.len()
            )),
        },
        Err(error) => Err(format!(
            "Не удалось безопасно проверить локальный ключ {}: {error}",
            key_path.display()
        )),
    }
}

#[cfg(any(windows, test))]
fn cleanup_raw_key_backups(key_path: &Path) -> Result<(), String> {
    for backup in raw_key_backup_candidates(key_path)? {
        std::fs::remove_file(&backup).map_err(|error| {
            format!(
                "Локальный ключ успешно проверен, но не удалось удалить сырой backup {}: {error}",
                backup.display()
            )
        })?;
    }
    Ok(())
}

fn load_or_create_local_data_key(db_path: &Path) -> Result<[u8; 32], String> {
    let file_name = db_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("dokkomplekt-state.db");
    let key_path = db_path.with_file_name(format!("{file_name}.key"));
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    #[cfg(windows)]
    recover_interrupted_key_migration(&key_path)?;

    match std::fs::symlink_metadata(&key_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(format!(
                    "Локальный ключ имеет недопустимый тип: {}",
                    key_path.display()
                ));
            }
            let stored = std::fs::read(&key_path)
                .map_err(|error| format!("Не удалось прочитать локальный ключ защиты: {error}"))?;
            let key = decode_or_migrate_local_key(&key_path, &stored)?;
            #[cfg(windows)]
            cleanup_raw_key_backups(&key_path)?;
            return Ok(key);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Не удалось безопасно проверить локальный ключ {}: {error}",
                key_path.display()
            ))
        }
    }

    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key)
        .map_err(|error| format!("Не удалось создать ключ защиты локальных данных: {error}"))?;
    let encoded = encode_local_key_for_platform(&key)?;
    match write_new_key_file(&key_path, &encoded) {
        Ok(()) => Ok(key),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let stored = std::fs::read(&key_path).map_err(|read_error| {
                format!("Не удалось прочитать параллельно созданный локальный ключ: {read_error}")
            })?;
            decode_or_migrate_local_key(&key_path, &stored)
        }
        Err(error) => Err(format!(
            "Не удалось создать локальный ключ защиты данных: {error}"
        )),
    }
}

fn write_new_key_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    restrict_key_file_permissions(path)?;
    Ok(())
}

#[cfg(windows)]
fn replace_key_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let nonce = Uuid::new_v4();
    let temp = path.with_extension(format!("key.protected.{nonce}.tmp"));
    let backup = path.with_extension(format!("key.raw.{nonce}.bak"));
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Не удалось подготовить защищённый ключ: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("Не удалось записать защищённый ключ: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Не удалось синхронизировать защищённый ключ: {error}"))?;
    }
    restrict_key_file_permissions(&temp)
        .map_err(|error| format!("Не удалось ограничить права на защищённый ключ: {error}"))?;
    std::fs::rename(path, &backup)
        .map_err(|error| format!("Не удалось начать миграцию локального ключа: {error}"))?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let rollback = std::fs::rename(&backup, path);
        let cleanup = std::fs::remove_file(&temp);
        if let Err(rollback_error) = rollback {
            return Err(format!(
                "Не удалось завершить миграцию локального ключа: {error}; также не удалось восстановить сырой backup {}: {rollback_error}. Backup сохранён для автоматического восстановления при следующем запуске.",
                backup.display()
            ));
        }
        if let Err(cleanup_error) = cleanup {
            return Err(format!(
                "Не удалось завершить миграцию локального ключа: {error}; исходный ключ восстановлен, но временный защищённый файл {} не удалён: {cleanup_error}",
                temp.display()
            ));
        }
        return Err(format!(
            "Не удалось завершить миграцию локального ключа: {error}; исходный ключ восстановлен."
        ));
    }
    std::fs::remove_file(&backup).map_err(|error| {
        format!(
            "Защищённый локальный ключ установлен, но сырой backup {} не удалось удалить: {error}",
            backup.display()
        )
    })?;
    Ok(())
}

fn restrict_key_file_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn decode_or_migrate_local_key(path: &Path, stored: &[u8]) -> Result<[u8; 32], String> {
    #[cfg(windows)]
    {
        if let Some(protected) = stored.strip_prefix(DPAPI_KEY_FILE_MAGIC) {
            return dpapi_unprotect_key(protected);
        }
        if stored.len() == 32 {
            let key: [u8; 32] = stored
                .try_into()
                .map_err(|_| "Локальный ключ защиты повреждён".to_string())?;
            let protected = encode_local_key_for_platform(&key)?;
            replace_key_file(path, &protected)?;
            return Ok(key);
        }
        Err("Локальный ключ защиты повреждён: неизвестный формат DPAPI.".to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        stored
            .try_into()
            .map_err(|_| "Локальный ключ защиты повреждён: ожидалось ровно 32 байта.".to_string())
    }
}

fn encode_local_key_for_platform(key: &[u8; 32]) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        let protected = dpapi_protect_key(key)?;
        let mut out = Vec::with_capacity(DPAPI_KEY_FILE_MAGIC.len() + protected.len());
        out.extend_from_slice(DPAPI_KEY_FILE_MAGIC);
        out.extend_from_slice(&protected);
        Ok(out)
    }
    #[cfg(not(windows))]
    {
        Ok(key.to_vec())
    }
}

#[cfg(windows)]
fn dpapi_protect_key(key: &[u8; 32]) -> Result<Vec<u8>, String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: key.len() as u32,
        pbData: key.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            null(),
            null_mut(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "Windows DPAPI не смог защитить локальный ключ: {}",
            std::io::Error::last_os_error()
        ));
    }
    let protected = unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(output.pbData.cast());
        bytes
    };
    Ok(protected)
}

#[cfg(windows)]
fn dpapi_unprotect_key(protected: &[u8]) -> Result<[u8; 32], String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null(),
            null_mut(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "Windows DPAPI не смог открыть локальный ключ текущего пользователя: {}",
            std::io::Error::last_os_error()
        ));
    }
    let decoded = unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(output.pbData.cast());
        bytes
    };
    decoded
        .try_into()
        .map_err(|_| "Windows DPAPI вернул ключ неверной длины.".to_string())
}

const TRIAL_DOCUMENT_LIMIT_MONTH: u32 = 30;
const TRIAL_MAX_DOCUMENTS_PER_RUN: u32 = TRIAL_DOCUMENT_LIMIT_MONTH;

#[derive(Debug, Clone, Serialize)]
struct DesktopAccessDecision {
    accepted: bool,
    mode: String,
    plan: String,
    reason: String,
    watermark: Option<String>,
    document_limit_month: u32,
    max_documents_per_run: u32,
    documents_used_month: u32,
    documents_left_month: u32,
}

#[derive(Debug, Clone)]
struct GenerationPermit {
    reservation: UsageReservation,
    watermark: Option<String>,
}

fn trusted_license_key() -> Result<dokkomplekt_license_core::PublicKeyBytes, String> {
    dokkomplekt_license_core::PublicKeyBytes::from_base64(TRUSTED_LICENSE_PUBKEY_B64)
        .map_err(|error| error.to_string())
}

fn current_month_key(now: OffsetDateTime) -> String {
    format!("{:04}-{:02}", now.year(), u8::from(now.month()))
}

fn signed_plan_to_product_plan(plan: &SignedPlanId) -> ProductPlanId {
    match plan {
        SignedPlanId::Trial => ProductPlanId::Trial,
        SignedPlanId::DoctorStart => ProductPlanId::DoctorStart,
        SignedPlanId::DoctorPro => ProductPlanId::DoctorPro,
        SignedPlanId::Department => ProductPlanId::Department,
        SignedPlanId::Clinic => ProductPlanId::Clinic,
        SignedPlanId::Enterprise => ProductPlanId::Enterprise,
        SignedPlanId::Vip => ProductPlanId::Vip,
    }
}

fn plan_label(plan: &SignedPlanId) -> &'static str {
    signed_plan_to_product_plan(plan).as_wire_id()
}

fn watermark_text(mode: &WatermarkMode) -> Option<String> {
    match mode {
        WatermarkMode::None => None,
        WatermarkMode::Trial => Some(TRIAL_WATERMARK_TEXT.to_string()),
        WatermarkMode::Demo => Some(EXPIRED_DEMO_WATERMARK_TEXT.to_string()),
    }
}

fn load_or_create_install_id(app: &tauri::AppHandle) -> Result<String, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    let path = data_dir.join("install-id");
    if let Ok(value) = std::fs::read_to_string(&path) {
        let value = value.trim();
        if !value.is_empty() {
            return Ok(value.to_string());
        }
    }
    let value = Uuid::new_v4().to_string();
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write as _;
            file.write_all(value.as_bytes())
                .map_err(|error| error.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::read_to_string(&path)
                .map(|value| value.trim().to_string())
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn stable_machine_guid() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = std::process::Command::new("reg.exe")
            .args([
                "query",
                r"HKLM\SOFTWARE\Microsoft\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find(|line| line.contains("MachineGuid"))
            .and_then(|line| line.split_whitespace().last())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }
    #[cfg(target_os = "linux")]
    {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(value) = std::fs::read_to_string(path) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find(|line| line.contains("IOPlatformUUID"))
            .and_then(|line| line.split('=').nth(1))
            .map(|value| value.trim().trim_matches('"').to_string())
            .filter(|value| !value.is_empty())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

fn machine_fingerprint(app: &tauri::AppHandle) -> Result<MachineFingerprint, String> {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok();
    Ok(MachineFingerprint::from_facts(&MachineFacts {
        os: std::env::consts::OS.to_string(),
        hostname,
        machine_guid: stable_machine_guid(),
        install_id: Some(load_or_create_install_id(app)?),
    }))
}

fn inspect_desktop_access(
    app: &tauri::AppHandle,
    state: &AppState,
    requested_documents: u32,
) -> Result<DesktopAccessDecision, String> {
    let now = OffsetDateTime::now_utc();
    let month_key = current_month_key(now);
    let db_path = default_state_db_path(app)?;
    let repo = repository_for(&db_path)?;
    let usage_snapshot = repo
        .usage_snapshot(&month_key)
        .map_err(|error| error.to_string())?;
    let used = usage_snapshot.created_documents;

    let license = state
        .license_document
        .lock()
        .map_err(|_| "license state lock failed")?
        .clone();
    if let Some(document) = license {
        match verify_license_document_now(&document, &trusted_license_key()?) {
            Ok(()) => {
                let payload = &document.license.payload;
                let pack = state.pack.lock().map_err(|_| "state lock failed")?;
                let case = state
                    .semantic_case
                    .lock()
                    .map_err(|_| "state lock failed")?;
                let request = SignedAccessRequest {
                    now_utc: now,
                    month_key: month_key.clone(),
                    machine: machine_fingerprint(app)?,
                    requested_documents,
                    template_count: Some(pack.documents.len().try_into().unwrap_or(u32::MAX)),
                    profile_count: Some(case.active_domains.len().try_into().unwrap_or(u32::MAX)),
                };
                let mut usage = UsageLedger::default();
                usage.record_documents(&month_key, usage_snapshot.created_documents);
                usage.trial_created_total = usage_snapshot.trial_documents_total;
                usage.last_seen_utc = Some(now.to_string());
                let decision = evaluate_signed_access(payload, &usage, &request)
                    .map_err(|error| error.to_string())?;
                let accepted = !matches!(decision.status, SignedAccessStatus::Denied);
                return Ok(DesktopAccessDecision {
                    accepted,
                    mode: match decision.status {
                        SignedAccessStatus::Allowed => "paid",
                        SignedAccessStatus::Warning => "warning",
                        SignedAccessStatus::Denied => "blocked",
                    }
                    .into(),
                    plan: plan_label(&decision.plan).into(),
                    reason: decision.code,
                    watermark: watermark_text(&payload.watermark_mode),
                    document_limit_month: payload.document_limit_month,
                    max_documents_per_run: signed_run_limit(&payload.plan),
                    documents_used_month: used,
                    documents_left_month: decision.documents_left_month,
                });
            }
            Err(error) if LICENSE_TRUST_ANCHOR_IS_CONFIGURED => {
                return Err(format!("license verification failed: {error}"));
            }
            Err(_) => {
                // Free unsigned preview builds intentionally have no production trust
                // anchor. A persisted production license is therefore unverifiable in
                // that diagnostic build and must not disable the independent local trial.
                // Production builds always inject the key and stay fail-closed above.
            }
        }
    }

    Ok(local_trial_access_decision(
        usage_snapshot.trial_documents_total,
        requested_documents,
    ))
}

fn local_trial_access_decision(trial_used: u32, requested_documents: u32) -> DesktopAccessDecision {
    // Trial entitlement is lifetime-trial usage, not all documents created this month.
    // Paid/previously licensed generation must never consume the fallback trial budget
    // after an upgrade or when an unsigned preview is used for diagnostics.
    let projected = trial_used.saturating_add(requested_documents);
    let per_run_ok = requested_documents <= TRIAL_MAX_DOCUMENTS_PER_RUN;
    let trial_total_ok = projected <= TRIAL_DOCUMENT_LIMIT_MONTH;
    DesktopAccessDecision {
        accepted: per_run_ok && trial_total_ok,
        mode: if per_run_ok && trial_total_ok {
            "trial"
        } else {
            "blocked"
        }
        .into(),
        plan: "trial".into(),
        reason: if !per_run_ok {
            "per_run_limit"
        } else if !trial_total_ok {
            "trial_total_limit"
        } else {
            "local_trial"
        }
        .into(),
        watermark: Some(TRIAL_WATERMARK_TEXT.to_string()),
        document_limit_month: TRIAL_DOCUMENT_LIMIT_MONTH,
        max_documents_per_run: TRIAL_MAX_DOCUMENTS_PER_RUN,
        documents_used_month: trial_used,
        documents_left_month: TRIAL_DOCUMENT_LIMIT_MONTH.saturating_sub(projected),
    }
}

fn reserve_generation_access(
    app: &tauri::AppHandle,
    state: &AppState,
    requested_documents: u32,
) -> Result<GenerationPermit, String> {
    if requested_documents == 0 {
        return Err("generation request contains no documents".into());
    }
    let decision = inspect_desktop_access(app, state, requested_documents)?;
    if !decision.accepted {
        return Err(format!(
            "Генерация заблокирована лицензией: {} (план {}, запрошено {}, лимит за запуск {}, использовано за месяц {}/{}, осталось {})",
            decision.reason,
            decision.plan,
            requested_documents,
            decision.max_documents_per_run,
            decision.documents_used_month,
            decision.document_limit_month,
            decision.documents_left_month
        ));
    }
    if requested_documents > decision.max_documents_per_run {
        return Err(format!(
            "За один запуск разрешено не более {} документов.",
            decision.max_documents_per_run
        ));
    }
    let month_key = current_month_key(OffsetDateTime::now_utc());
    let trial = decision.plan == "trial";
    let db_path = default_state_db_path(app)?;
    let mut repo = repository_for(&db_path)?;
    let reservation = repo
        .reserve_usage_with_publication_recovery(
            &month_key,
            requested_documents,
            trial,
            decision.document_limit_month,
            if trial {
                TRIAL_DOCUMENT_LIMIT_MONTH
            } else {
                u32::MAX
            },
        )
        .map_err(|error| format!("Не удалось атомарно зарезервировать лимит: {error}"))?;
    Ok(GenerationPermit {
        reservation,
        watermark: decision.watermark,
    })
}

fn commit_generation_access(
    app: &tauri::AppHandle,
    permit: &GenerationPermit,
) -> Result<(), String> {
    let db_path = default_state_db_path(app)?;
    let mut repo = repository_for(&db_path)?;
    if !repo
        .finalize_published_usage(&permit.reservation.reservation_id)
        .map_err(|error| error.to_string())?
    {
        return Err("Резервация опубликованной генерации потеряна.".into());
    }
    Ok(())
}

fn rollback_generation_access(
    app: &tauri::AppHandle,
    _state: &AppState,
    permit: &GenerationPermit,
) {
    if let Ok(db_path) = default_state_db_path(app) {
        if let Ok(mut repo) = repository_for(&db_path) {
            let _ = repo.rollback_usage(&permit.reservation);
        }
    }
}

#[derive(Debug)]
struct HydratedTemplateCase {
    case: SemanticCase,
    counter_reservations: Vec<CounterValue>,
}

fn hydrate_case_with_persistent_template_data(
    app: &tauri::AppHandle,
    base: &SemanticCase,
    template_texts: &[String],
    reserve_counters: bool,
) -> Result<HydratedTemplateCase, String> {
    let db_path = default_state_db_path(app)?;
    let mut repo = repository_for(&db_path)?;
    let mut case = base.clone();
    let mut counter_reservations = Vec::new();
    merge_persistent_clause_blocks(
        &mut case,
        repo.clause_blocks_map().map_err(|e| e.to_string())?,
    );
    let year = current_year_utc();
    let mut requests = std::collections::BTreeMap::new();
    for text in template_texts {
        for request in template_counter_requests(text) {
            requests.entry(request.key.clone()).or_insert(request);
        }
    }
    for request in requests.into_values() {
        let value = if reserve_counters {
            let reservation = match repo.next_counter(&request.key, year) {
                Ok(value) => value,
                Err(error) => {
                    for previous in counter_reservations.iter().rev() {
                        let _ = repo.rollback_counter(previous);
                    }
                    return Err(error.to_string());
                }
            };
            let value = reservation.value;
            counter_reservations.push(reservation);
            value
        } else {
            repo.peek_counter(&request.key, year)
                .map_err(|e| e.to_string())?
                .value
                .saturating_add(1)
        };
        let formatted = format_counter_value(&request.format, value, year);
        let id = format!("counter.{}", request.key);
        case.values.insert(
            id.clone(),
            dokkomplekt_core::SemanticValue::new(id, formatted, ValueSource::SafeDefault, 1.0),
        );
    }
    Ok(HydratedTemplateCase {
        case,
        counter_reservations,
    })
}

fn rollback_counter_reservations(app: &tauri::AppHandle, reservations: &[CounterValue]) {
    if reservations.is_empty() {
        return;
    }
    let Ok(db_path) = default_state_db_path(app) else {
        return;
    };
    let Ok(mut repo) = repository_for(&db_path) else {
        return;
    };
    for reservation in reservations.iter().rev() {
        let _ = repo.rollback_counter(reservation);
    }
}

fn ensure_persistence_available(state: &AppState) -> Result<(), String> {
    if state.persistence_blocked.load(Ordering::SeqCst) {
        let reason = state
            .persistence_error
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| "неизвестная ошибка восстановления состояния".into());
        return Err(format!(
            "Сохранение заблокировано для защиты данных: {reason}. Загрузите исправную резервную базу состояния."
        ));
    }
    Ok(())
}

fn persist_state_to(db_path: &Path, state: &AppState) -> Result<(), String> {
    ensure_persistence_available(state)?;
    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    repository_for(db_path)?
        .save_case_and_pack_atomic("current", "default", &case, &pack)
        .map_err(|error| error.to_string())?;
    *state.db_path.lock().map_err(|_| "state lock failed")? = Some(db_path.to_path_buf());
    Ok(())
}

fn decode_word_payload(file_name: Option<&str>, encoded: &str) -> Result<Vec<u8>, String> {
    if let Some(name) = file_name {
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".docx") && !lower.ends_with(".docm") {
            return Err("Поддерживаются файлы DOCX и DOCM.".into());
        }
    }
    let trimmed = encoded.trim();
    if trimmed.len() > MAX_DOCX_BYTES.saturating_mul(2) {
        return Err("DOCX слишком большой: максимум 50 МБ.".into());
    }
    let bytes = BASE64_STANDARD
        .decode(trimmed)
        .map_err(|_| "Файл повреждён: не удалось декодировать содержимое.".to_string())?;
    if bytes.len() > MAX_DOCX_BYTES {
        return Err("DOCX слишком большой: максимум 50 МБ.".into());
    }
    if !bytes.starts_with(b"PK") {
        return Err("Файл не является DOCX/ZIP-контейнером.".into());
    }
    Ok(bytes)
}

fn numbered_candidate(path: &Path, index: u32) -> PathBuf {
    if index <= 1 {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let name = if extension.is_empty() {
        format!("{stem} ({index})")
    } else {
        format!("{stem} ({index}).{extension}")
    };
    parent.join(name)
}

include!("subsystems/publication_lock.rs");

struct UniqueFileReservation {
    /// Hidden staging file used by the renderer. It is never the user-visible name.
    path: PathBuf,
    desired_path: PathBuf,
    committed: bool,
}

impl UniqueFileReservation {
    fn acquire(path: &Path) -> Result<Self, String> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        for _ in 0..128 {
            let staging = parent.join(format!(
                ".dokkomplekt-file-stage-{}-{}.tmp",
                std::process::id(),
                Uuid::new_v4()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)
            {
                Ok(_) => {
                    return Ok(Self {
                        path: staging,
                        desired_path: path.to_path_buf(),
                        committed: false,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "Не удалось создать скрытый staging-файл результата: {error}"
                    ))
                }
            }
        }
        Err("Не удалось создать уникальный staging-файл результата.".into())
    }

    /// Atomically exposes a fully-rendered file under a unique final name.
    ///
    /// `hard_link` is used as an atomic create-if-absent primitive. If the
    /// destination filesystem cannot provide this guarantee, publication fails
    /// closed instead of leaving a partial/corrupt file under a final DOCX name.
    fn commit(mut self) -> Result<PathBuf, String> {
        for index in 1..=10_000 {
            let candidate = numbered_candidate(&self.desired_path, index);
            match std::fs::hard_link(&self.path, &candidate) {
                Ok(()) => {
                    self.committed = true;
                    let _ = std::fs::remove_file(&self.path);
                    return Ok(candidate);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "Файловая система не поддержала безопасную атомарную публикацию результата: {error}"
                    ))
                }
            }
        }
        Err("Не удалось подобрать уникальное имя после 10000 попыток.".into())
    }
}

impl Drop for UniqueFileReservation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn publish_stage_to_unique_directory(stage: &Path, desired: &Path) -> Result<PathBuf, String> {
    let parent = desired.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    for index in 1..=10_000 {
        let candidate = numbered_candidate(desired, index);
        let lock_name = format!(
            ".dokkomplekt-dir-reservation-{}-{index}.lock",
            sanitize_path_component(
                desired
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("output")
            )
        );
        let lock_path = parent.join(lock_name);
        let Some(reservation) = try_acquire_publication_lock(&lock_path)? else {
            continue;
        };
        if candidate.exists() {
            drop(reservation);
            continue;
        }
        let publish_result = std::fs::rename(stage, &candidate);
        drop(reservation);
        match publish_result {
            Ok(()) => return Ok(candidate),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                continue
            }
            Err(error) => {
                return Err(format!(
                    "Не удалось атомарно опубликовать комплект: {error}"
                ))
            }
        }
    }
    Err("Не удалось подобрать уникальную папку после 10000 попыток.".into())
}

#[cfg(test)]
mod processing_guard_fencing_tests {
    use super::*;

    fn test_source(label: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-processing-guard-{label}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create processing guard test root");
        (root.clone(), root.join("source.docx"))
    }

    fn stop_heartbeat(guard: &mut ProcessingGuard) {
        guard.heartbeat_stop.store(true, Ordering::SeqCst);
        if let Some(thread) = guard.heartbeat_thread.take() {
            thread.join().expect("join processing guard heartbeat");
        }
    }

    fn replace_owner(marker: &Path, nonce: &str) {
        let _ = std::fs::remove_dir_all(marker);
        std::fs::create_dir_all(marker).expect("recreate processing marker");
        std::fs::write(
            marker.join("owner"),
            format!(
                "schema=3\nhost=replacement-host\npid=424242\ncreated_unix={}\nnonce={nonce}\n",
                unix_now_seconds()
            ),
        )
        .expect("write replacement owner");
        std::fs::write(
            processing_heartbeat_path(marker, nonce),
            unix_now_seconds().to_string(),
        )
        .expect("write replacement heartbeat");
    }

    #[test]
    fn processing_guard_drop_does_not_delete_replacement_owner() {
        let (root, source) = test_source("drop-fencing");
        let mut guard = ProcessingGuard::acquire(&source, "same-job")
            .expect("acquire first guard")
            .expect("first guard must be acquired");
        stop_heartbeat(&mut guard);
        let marker = guard.marker.clone();
        replace_owner(&marker, "replacement-nonce");
        drop(guard);
        assert!(marker.is_dir(), "old guard deleted successor marker");
        assert!(processing_owner_matches(&marker, "replacement-nonce"));
        std::fs::remove_dir_all(root).expect("cleanup processing guard test root");
    }

    #[test]
    fn processing_guard_detects_lost_ownership_before_publish() {
        let (root, source) = test_source("lost-owner");
        let mut guard = ProcessingGuard::acquire(&source, "same-job")
            .expect("acquire first guard")
            .expect("first guard must be acquired");
        stop_heartbeat(&mut guard);
        let marker = guard.marker.clone();
        replace_owner(&marker, "replacement-nonce");
        assert!(guard.ensure_current().is_err());
        drop(guard);
        assert!(processing_owner_matches(&marker, "replacement-nonce"));
        std::fs::remove_dir_all(root).expect("cleanup processing guard test root");
    }

    #[test]
    fn processing_guard_released_claim_can_be_reacquired_immediately() {
        let (root, source) = test_source("released-reacquire");
        let first = ProcessingGuard::acquire(&source, "same-job")
            .expect("acquire first guard")
            .expect("first guard must be acquired");
        let first_marker = first.marker.clone();
        let first_nonce = first.owner_nonce.clone();
        drop(first);
        assert!(processing_release_matches(&first_marker, &first_nonce));
        let second = ProcessingGuard::acquire(&source, "same-job")
            .expect("reacquire released guard")
            .expect("released guard must be immediately reclaimable");
        assert_ne!(second.owner_nonce, first_nonce);
        drop(second);
        std::fs::remove_dir_all(root).expect("cleanup processing guard test root");
    }
}

/// Current UTC year from the system clock, std-only (civil-from-days algorithm).
fn current_year_utc() -> i32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }) as i32
}

include!("subsystems/legacy_template_runtime.rs");
include!("subsystems/profile_case_hydration.rs");
include!("subsystems/profile_sources.rs");
include!("subsystems/output_root_commands.rs");
include!("subsystems/publication_collision.rs");
include!("subsystems/source_identity_runtime.rs");
include!("subsystems/source_intake_commands.rs");
include!("subsystems/startup_state.rs");
include!("subsystems/document_commands.rs");
include!("subsystems/created_documents_intake.rs");
include!("subsystems/business_registry.rs");
#[cfg(test)]
include!("subsystems/dedup_guard_tests.rs");
include!("subsystems/knowledge_registry.rs");
include!("subsystems/quality_telemetry.rs");
include!("subsystems/process_blueprints.rs");
include!("subsystems/clause_block_commands.rs");

include!("subsystems/processing_guard.rs");
include!("subsystems/shared_completion_guards.rs");
include!("subsystems/automation_dedup.rs");
include!("subsystems/automation_runtime.rs");

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let background_watch = args.iter().any(|arg| arg == "--background-watch");
    let e2e_install_watch_folder = args.iter().find_map(|arg| {
        arg.strip_prefix("--e2e-install-watcher=")
            .map(str::to_string)
    });
    let e2e_uninstall_watcher = args.iter().any(|arg| arg == "--e2e-uninstall-watcher");
    let e2e_evidence_path = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--e2e-evidence=").map(PathBuf::from));

    let run_result = tauri::Builder::default()
        .manage(AppState::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            if let Ok(data_dir) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&data_dir);
                if let Err(error) = reference_data_update::load_cached(&data_dir) {
                    eprintln!("Подписанный производственный календарь не активирован: {error}");
                }
                if reference_data_update::automatic_feed_configured() {
                    let update_dir = data_dir.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = reference_data_update::maybe_auto_update(&update_dir) {
                            eprintln!(
                                "Автообновление производственного календаря пропущено: {error}"
                            );
                        }
                    });
                }
                if let Err(error) = cleanup_intake_workspace(&handle) {
                    eprintln!("Очистка временных рабочих данных при запуске пропущена: {error}");
                }
                let _ = std::fs::remove_dir_all(data_dir.join("word-scanner-work"));
                start_periodic_intake_cleanup(handle.clone());
                if let Ok(db_path) = default_state_db_path(&handle) {
                    if let Ok(mut repo) = repository_for(&db_path) {
                        generation_publication::recover_startup_generation_state(
                            &handle,
                            &mut repo,
                        );
                        if let Ok(cases) = repo.list_case_runs(500) {
                            let mut roots = BTreeSet::new();
                            for case in cases {
                                if !case.output_root.trim().is_empty() {
                                    roots.insert(PathBuf::from(case.output_root));
                                }
                            }
                            for root in roots {
                                let _ = cleanup_stale_stage_directories(
                                    &root,
                                    Duration::from_secs(24 * 60 * 60),
                                );
                            }
                        }
                    }
                }
            }
            let state = app.state::<AppState>();
            match acquire_instance_lock(&handle, background_watch)
                .map_err(std::io::Error::other)?
            {
                InstanceLockOutcome::Acquired(instance_path) => {
                    *state
                        .instance_lock
                        .lock()
                        .map_err(|_| std::io::Error::other("instance state lock failed"))? =
                        Some(instance_path);

                }
                InstanceLockOutcome::AlreadyRunning => {
                    if !background_watch {
                        enqueue_activation_request(&handle).map_err(std::io::Error::other)?;
                    }
                    handle.exit(0);
                    return Ok(());
                }
            }

            // Startup and the first UI state request share one serialized restore boundary.
            // The WebView can never observe the default empty pack while a durable pack is
            // still being restored from SQLite on another startup path.
            if let Err(error) = ensure_default_state_loaded(&handle, &state) {
                eprintln!("Восстановление рабочего набора требует внимания: {error}");
            }
            if e2e_uninstall_watcher || e2e_install_watch_folder.is_some() {
                if std::env::var("DOKKOMPLEKT_RUN_HARDWARE_E2E").ok().as_deref() != Some("1") {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "E2E watcher commands require DOKKOMPLEKT_RUN_HARDWARE_E2E=1",
                    )
                    .into());
                }
                let evidence_path = e2e_evidence_path.clone().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "--e2e-evidence=<absolute path> is required",
                    )
                })?;
                if let Some(parent) = evidence_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let payload = if e2e_uninstall_watcher {
                    let (removed, warnings) = remove_autostart_entries();
                    if let Ok(config_path) = watcher_config_path(&handle) {
                        if config_path.exists() {
                            std::fs::remove_file(&config_path)?;
                        }
                    }
                    serde_json::json!({
                        "schema": "dokkomplekt.watcher-e2e-command.v1",
                        "action": "uninstall",
                        "removed_files": removed.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                        "warnings": warnings,
                    })
                } else {
                    let watch_folder = e2e_install_watch_folder
                        .clone()
                        .ok_or_else(|| std::io::Error::other("watch folder missing"))?;
                    let (removed, cleanup_warnings) = remove_autostart_entries();
                    let install_result = install_background_watcher(
                        WatcherInstallRequest {
                            watch_folder: watch_folder.clone(),
                            output_root: canonical_default_output_root(&handle)?.display().to_string(),
                            default_year: Some(current_year_utc()),
                            sick_leave_enabled: false,
                            folder_parts: vec![
                                FolderNamePart::DocumentNumber,
                                FolderNamePart::DocumentDate,
                            ],
                            auto_print: false,
                            print_copies_by_document: BTreeMap::new(),
                            max_parallel_cases: 2,
                        },
                        handle.clone(),
                    )
                    .map_err(std::io::Error::other)?;
                    serde_json::json!({
                        "schema": "dokkomplekt.watcher-e2e-command.v1",
                        "action": "install",
                        "watch_folder": watch_folder,
                        "stale_entries_removed": removed.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                        "cleanup_warnings": cleanup_warnings,
                        "install_result": install_result,
                    })
                };
                std::fs::write(
                    &evidence_path,
                    serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?,
                )?;
                handle.exit(0);
                return Ok(());
            }
            let window_config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or_else(|| std::io::Error::other("main window config missing"))?;
            let main_window = tauri::WebviewWindowBuilder::from_config(&handle, &window_config)
                .map_err(std::io::Error::other)?
                .build()
                .map_err(std::io::Error::other)?;

            if background_watch {
                let _ = main_window.hide();
                let started = watcher_config_path(&handle)
                    .and_then(|config_path| std::fs::read(config_path).map_err(|e| e.to_string()))
                    .and_then(|bytes| {
                        serde_json::from_slice::<WatcherRuntimeConfig>(&bytes)
                            .map_err(|e| e.to_string())
                    })
                    .and_then(|config| start_watcher_thread(handle.clone(), config, true))
                    .is_ok();
                if !started {
                    handle.exit(0);
                }
            } else {
                start_activation_listener(handle.clone()).map_err(std::io::Error::other)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            first_run_state,
            get_default_output_root,
            ensure_output_root,
            get_output_preferences,
            save_output_preferences,
            analyze_template,
            analyze_template_file,
            prepare_template_setup,
            import_learning_example_file,
            learn_template_from_examples_command,
            apply_template_learning_map,
            register_learned_template,
            confirm_template_setup,
            rename_document_button,
            remove_document_button,
            update_document_popup_fields,
            reset_case,
            set_field,
            parse_source,
            pick_source_file,
            parse_source_path,
            parse_source_file,
            get_intake_capabilities,
            get_sidecar_status,
            get_component_statuses,
            refresh_component_catalog,
            install_component,
            remove_component,
            parse_web_source,
            get_document_template_text,
            get_workflow_plan,
            get_workflow_plan_batch,
            apply_popup,
            apply_popup_batch,
            render_preview,
            render_docx,
            render_docx_batch,
            get_privacy_preferences,
            update_privacy_preferences,
            run_workspace_hygiene,
            list_automation_exceptions,
            resolve_automation_exception,
            confirm_risk_exception_and_retry,
            confirm_bundle_exception_and_retry,
            get_automation_metrics,
            get_daily_automation_dashboard,
            get_queue_status,
            get_corpus_status,
            get_learned_kit_decision,
            export_corpus,
            list_case_runs,
            retry_case_run,
            list_audit_events,
            list_clause_blocks,
            save_clause_block,
            replace_clause_blocks,
            delete_clause_block,
            suggest_template_markup_command,
            apply_template_markup_command,
            preview_mail_merge,
            prepare_mail_merge_file,
            render_mail_merge,
            apply_scanner,
            start_word_scanner,
            activate_word_scanner,
            capture_word_scanner,
            apply_word_scanner_selection,
            close_word_scanner,
            save_learned_scanner_rule,
            list_learned_scanner_rules,
            delete_learned_scanner_rule,
            check_template_regression,
            update_document_template,
            list_template_versions,
            rollback_template_version,
            get_diary_plan,
            get_record_series_plan,
            icd10_suggest,
            get_output_plan,
            route_intake,
            save_state,
            load_state,
            validate_product_access,
            verify_rust_license_text,
            check_for_updates,
            get_background_watcher_state,
            install_background_watcher,
            update_background_watcher_preferences,
            uninstall_background_watcher,
            run_created_documents_intake,
            get_print_triage,
            list_template_approvals,
            approve_document_template,
            revoke_document_template_approval,
            print_files,
            get_printer_inventory,
            update_print_preferences,
            export_files_to_pdf,
            create_kedo_package,
            pick_template_files,
            pick_folder,
            open_in_file_manager,
            get_semantic_model_config,
            update_semantic_model_config,
            test_semantic_model,
            get_calibrated_threshold_status,
            import_calibrated_thresholds,
            get_reference_data_status,
            update_reference_data,
            import_reference_data,
            semantic_extract,
            import_business_registry,
            lookup_business_registry,
            apply_business_registry_record,
            export_one_c_counterparties,
            list_organization_knowledge,
            upsert_organization_knowledge,
            delete_organization_knowledge,
            apply_organization_knowledge,
            get_quality_telemetry,
            get_process_blueprints,
            select_process_blueprint,
            import_template_file
        ])
        .run(tauri::generate_context!());
    if let Err(error) = run_result {
        eprintln!("Dokkomplekt Universal завершён из-за ошибки Tauri: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_json_bytes, current_year_utc, is_forbidden_public_download_host,
        is_forbidden_public_download_ip, load_or_create_local_data_key,
        local_trial_access_decision, normalized_picker_output, parse_semver, pdf_print_settings,
        plan_label, reject_parent_traversal, safe_update_file_name, signed_plan_to_product_plan,
        validate_printable_file, validate_update_url, write_trust_report, SourceProvenance,
        TrustReportContext, TRIAL_DOCUMENT_LIMIT_MONTH,
    };
    use base64::Engine as _;

    #[test]
    fn folder_picker_output_is_cancel_safe_and_requires_a_real_directory() {
        assert_eq!(normalized_picker_output(b"").unwrap(), None);
        let path = std::env::temp_dir().join(format!(
            "dokkomplekt-folder-picker-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let selected = normalized_picker_output(path.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(selected.as_deref(), Some(path.to_string_lossy().as_ref()));
        std::fs::remove_dir_all(&path).unwrap();
        assert!(normalized_picker_output(path.to_string_lossy().as_bytes()).is_err());
        #[cfg(unix)]
        assert_eq!(
            normalized_picker_output(b"/").unwrap().as_deref(),
            Some("/")
        );
    }

    #[test]
    fn update_semver_comparison_is_strict_and_prerelease_aware() {
        assert!(parse_semver("18.1.0").unwrap() > parse_semver("18.0.8").unwrap());
        assert!(parse_semver("18.1.0").unwrap() > parse_semver("18.1.0-rc.1").unwrap());
        assert!(parse_semver("18.0").is_err());
    }

    #[test]
    fn update_network_guard_rejects_private_and_service_addresses() {
        for raw in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "100.64.0.1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "::1",
            "fc00::1",
        ] {
            let ip = raw.parse().unwrap();
            assert!(
                is_forbidden_public_download_ip(ip),
                "address must be rejected: {raw}"
            );
        }
        assert!(!is_forbidden_public_download_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn update_url_rejects_placeholder_and_non_dns_hosts_before_resolution() {
        for host in [
            "localhost",
            "updates.invalid",
            "updates.test",
            "updates.example",
            "updates.local",
            "example.com",
            "downloads.example.com",
            "single-label",
            "bad_host.dokkomplekt.ru",
        ] {
            assert!(
                is_forbidden_public_download_host(host),
                "host must be rejected: {host}"
            );
        }
        for host in ["updates.dokkomplekt.ru", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(
                !is_forbidden_public_download_host(host),
                "host must be accepted: {host}"
            );
        }
        assert!(validate_update_url("https://downloads.example.com/app.exe").is_err());
    }

    #[test]
    fn update_url_resolution_is_retained_for_dns_pinning() {
        let validated = validate_update_url("https://1.1.1.1/Dokkomplekt-18.1.0.exe").unwrap();
        let public_ip: std::net::IpAddr = "1.1.1.1".parse().unwrap();

        assert_eq!(validated.host, "1.1.1.1");
        assert!(!validated.addresses.is_empty());
        assert!(validated
            .addresses
            .iter()
            .all(|address| address.ip() == public_ip));
    }

    #[test]
    fn update_manifest_canonical_json_sorts_object_keys_recursively() {
        let value = serde_json::json!({"z": 1, "a": {"я": "тест", "b": 2}});
        let encoded = canonical_json_bytes(&value).unwrap();
        assert_eq!(
            String::from_utf8(encoded).unwrap(),
            r#"{"a":{"b":2,"я":"тест"},"z":1}"#
        );
    }

    #[test]
    fn update_file_name_blocks_path_and_windows_ads_tricks() {
        let safe = reqwest::Url::parse("https://example.com/Dokkomplekt-18.1.0.exe").unwrap();
        assert_eq!(
            safe_update_file_name(&safe).unwrap(),
            "Dokkomplekt-18.1.0.exe"
        );
        for raw in [
            "https://example.com/.hidden.exe",
            "https://example.com/update..exe",
            "https://example.com/update.exe:payload",
            "https://example.com/CON.exe",
        ] {
            assert!(safe_update_file_name(&reqwest::Url::parse(raw).unwrap()).is_err());
        }
    }

    #[test]
    fn civil_year_from_unix_clock_is_sane() {
        // The algorithm is pure; sanity-check the range rather than a wall clock.
        let year = current_year_utc();
        assert!((2024..=2124).contains(&year), "unexpected year {year}");
    }

    #[test]
    fn local_data_key_is_created_once_and_reused_without_sqlite_plaintext_fallback() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-local-key-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp dir");
        let db = root.join("state.sqlite");
        let first = load_or_create_local_data_key(&db).expect("create key");
        let second = load_or_create_local_data_key(&db).expect("reuse key");
        assert_eq!(first, second);
        assert_ne!(first, [0u8; 32]);
        let key_path = root.join("state.sqlite.key");
        let stored = std::fs::read(&key_path).expect("stored key");
        #[cfg(windows)]
        {
            assert!(
                stored.starts_with(super::DPAPI_KEY_FILE_MAGIC),
                "Windows key file must use the DPAPI envelope"
            );
            assert_ne!(stored.as_slice(), first.as_slice());
            let decoded =
                super::decode_or_migrate_local_key(&key_path, &stored).expect("decode stored key");
            assert_eq!(decoded, first);
        }
        #[cfg(not(windows))]
        assert_eq!(stored, first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(root.join("state.sqlite.key"))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_local_data_key_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-bad-local-key-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp dir");
        let db = root.join("state.sqlite");
        std::fs::write(root.join("state.sqlite.key"), b"short").expect("bad key");
        assert!(load_or_create_local_data_key(&db).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_local_key_migration_recovers_single_raw_backup() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-key-recovery-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let key_path = root.join("state.sqlite.key");
        let backup = root.join("state.sqlite.key.raw.interrupted.bak");
        let raw_key = [7u8; 32];
        std::fs::write(&backup, raw_key).unwrap();

        super::recover_interrupted_key_migration(&key_path).expect("recover raw backup");

        assert_eq!(std::fs::read(&key_path).unwrap(), raw_key);
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_local_key_backups_fail_closed_without_guessing() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-key-ambiguous-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let key_path = root.join("state.sqlite.key");
        let first = root.join("state.sqlite.key.raw.first.bak");
        let second = root.join("state.sqlite.key.raw.second.bak");
        std::fs::write(&first, [1u8; 32]).unwrap();
        std::fs::write(&second, [2u8; 32]).unwrap();

        let error = super::recover_interrupted_key_migration(&key_path)
            .expect_err("multiple raw backups must not be guessed");

        assert!(error.contains("несколько резервных копий"), "{error}");
        assert!(!key_path.exists());
        assert!(first.is_file());
        assert!(second.is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_primary_key_preserves_raw_backup_for_manual_recovery() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-key-corrupt-primary-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("state.sqlite");
        let key_path = root.join("state.sqlite.key");
        let backup = root.join("state.sqlite.key.raw.recovery.bak");
        std::fs::write(&key_path, b"corrupt-primary").unwrap();
        std::fs::write(&backup, [4u8; 32]).unwrap();

        super::recover_interrupted_key_migration(&key_path).expect("primary path is present");
        assert!(
            backup.is_file(),
            "backup must survive before primary validation"
        );
        assert!(load_or_create_local_data_key(&db).is_err());
        assert!(
            backup.is_file(),
            "failed primary decode must preserve raw backup"
        );
        assert_eq!(std::fs::read(&backup).unwrap(), [4u8; 32]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn existing_local_key_removes_stale_raw_backup_or_fails() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-key-cleanup-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let key_path = root.join("state.sqlite.key");
        let backup = root.join("state.sqlite.key.raw.stale.bak");
        std::fs::write(&key_path, b"protected-key-envelope").unwrap();
        std::fs::write(&backup, [9u8; 32]).unwrap();

        super::recover_interrupted_key_migration(&key_path).expect("preserve valid primary");
        assert!(
            backup.is_file(),
            "backup must survive until primary validation"
        );
        super::cleanup_raw_key_backups(&key_path).expect("remove raw backup after validation");

        assert_eq!(std::fs::read(&key_path).unwrap(), b"protected-key-envelope");
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn local_trial_budget_counts_only_trial_documents() {
        let untouched = local_trial_access_decision(0, 1);
        assert!(untouched.accepted);
        assert_eq!(untouched.documents_used_month, 0);
        assert_eq!(
            untouched.documents_left_month,
            TRIAL_DOCUMENT_LIMIT_MONTH - 1
        );

        let last_allowed = local_trial_access_decision(TRIAL_DOCUMENT_LIMIT_MONTH - 1, 1);
        assert!(last_allowed.accepted);
        assert_eq!(last_allowed.documents_left_month, 0);

        let exhausted = local_trial_access_decision(TRIAL_DOCUMENT_LIMIT_MONTH, 1);
        assert!(!exhausted.accepted);
        assert_eq!(exhausted.reason, "trial_total_limit");
    }

    #[test]
    fn signed_and_desktop_plan_models_have_total_canonical_mapping() {
        use dokkomplekt_core::ProductPlanId;
        use dokkomplekt_license_core::PlanId;
        let cases = [
            (PlanId::Trial, ProductPlanId::Trial, "trial"),
            (
                PlanId::DoctorStart,
                ProductPlanId::DoctorStart,
                "doctor_start",
            ),
            (PlanId::DoctorPro, ProductPlanId::DoctorPro, "doctor_pro"),
            (PlanId::Department, ProductPlanId::Department, "department"),
            (PlanId::Clinic, ProductPlanId::Clinic, "clinic"),
            (PlanId::Enterprise, ProductPlanId::Enterprise, "enterprise"),
            (PlanId::Vip, ProductPlanId::Vip, "vip"),
        ];
        for (signed, product, wire) in cases {
            assert_eq!(signed_plan_to_product_plan(&signed), product);
            assert_eq!(plan_label(&signed), wire);
        }
    }

    #[test]
    fn unique_file_reservation_hides_incomplete_output_until_commit() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-unique-file-reservation-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let desired = root.join("result.docx");
        let reservation = super::UniqueFileReservation::acquire(&desired).unwrap();
        assert!(
            !desired.exists(),
            "final name must stay invisible while rendering"
        );
        assert!(reservation
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with(".dokkomplekt-file-stage-")));
        std::fs::write(&reservation.path, b"complete-docx-bytes").unwrap();
        let published = reservation.commit().unwrap();
        assert_eq!(published, desired);
        assert_eq!(std::fs::read(&published).unwrap(), b"complete-docx-bytes");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn path_resolution_rejects_parent_traversal_components() {
        assert!(reject_parent_traversal(std::path::Path::new("templates/../secret.docx")).is_err());
        assert!(reject_parent_traversal(std::path::Path::new("templates/normal.docx")).is_ok());
    }

    #[test]
    fn pdf_print_settings_preserve_copies_duplex_and_tray() {
        let preferences = super::PrintPreferences {
            printer_name: Some("Office".into()),
            duplex_mode: "long_edge".into(),
            tray: Some(4),
        };
        assert_eq!(
            pdf_print_settings(3, &preferences),
            vec!["3x", "ignore-pdf-print-settings", "duplexlong", "bin=4"]
        );
    }

    #[test]
    fn print_validation_accepts_documents_and_rejects_unsafe_extensions() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-print-validation-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp dir");
        let docx = root.join("contract.docx");
        let malformed_docx = root.join("malformed.docx");
        let executable = root.join("payload.exe");
        dokkomplekt_docx::create_docx_from_text(&docx, "Безопасный документ")
            .expect("create valid docx");
        std::fs::write(&malformed_docx, b"not-a-zip").expect("malformed docx");
        std::fs::write(&executable, b"not-printable").expect("exe");
        assert!(validate_printable_file(&docx).is_ok());
        assert!(validate_printable_file(&malformed_docx).is_err());
        assert!(validate_printable_file(&executable).is_err());
        assert!(validate_printable_file(&root.join("missing.pdf")).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an opt-in self-hosted Windows runner with Word and a dedicated test printer"]
    fn windows_word_print_hardware_e2e() {
        if std::env::var("DOKKOMPLEKT_RUN_HARDWARE_E2E").as_deref() != Ok("1") {
            panic!("set DOKKOMPLEKT_RUN_HARDWARE_E2E=1 on the dedicated hardware runner");
        }
        let printer = std::env::var("DOKKOMPLEKT_TEST_PRINTER")
            .expect("DOKKOMPLEKT_TEST_PRINTER must name the dedicated test printer");
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-windows-print-e2e-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create hardware e2e temp dir");
        let document = root.join("hardware-print.docx");
        super::create_docx_from_text(
            &document,
            "Dokkomplekt Windows hardware E2E\nThis page may be discarded.",
        )
        .expect("create hardware e2e DOCX");
        let preferences = super::PrintPreferences {
            printer_name: Some(printer),
            duplex_mode: std::env::var("DOKKOMPLEKT_TEST_DUPLEX")
                .unwrap_or_else(|_| "simplex".into()),
            tray: std::env::var("DOKKOMPLEKT_TEST_TRAY")
                .ok()
                .and_then(|value| value.parse::<i32>().ok()),
        };
        super::print_word_document_copies(&document, 1, &preferences)
            .expect("Word COM must synchronously submit the print job");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn source_provenance_hashes_exact_bytes_and_sanitizes_name() {
        let provenance =
            SourceProvenance::from_bytes("  patient\nrecord.docx\t", b"exact source bytes");
        assert_eq!(provenance.source_name, "patient record.docx");
        assert_eq!(
            provenance.source_sha256,
            "08df54b6923c9c8ab26e145805e456aac6ee96804d9a0d31d770f4bf8ccfcecf"
        );
    }

    #[test]
    fn source_provenance_rejects_non_sha256_markers() {
        assert!(SourceProvenance::from_sha256("source.docx", "manual-session").is_err());
        assert_eq!(
            SourceProvenance::from_sha256("source.docx", &"A".repeat(64))
                .unwrap()
                .source_sha256,
            "a".repeat(64)
        );
    }

    #[test]
    fn trust_report_is_minimized_and_redacted_by_default() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-trust-report-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp dir");
        let mut semantic_case = dokkomplekt_core::SemanticCase::default();
        semantic_case.values.insert(
            "contract.number".into(),
            dokkomplekt_core::SemanticValue::new(
                "contract.number",
                "A-42",
                dokkomplekt_core::ValueSource::UserConfirmed,
                1.0,
            ),
        );
        semantic_case.values.insert(
            "unused.secret".into(),
            dokkomplekt_core::SemanticValue::new(
                "unused.secret",
                "do-not-export",
                dokkomplekt_core::ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let used = ["contract.number".to_string()].into_iter().collect();
        let generated_names = ["contract.docx".into()];
        let report = write_trust_report(
            &root,
            &semantic_case,
            TrustReportContext {
                source_name: "source.docx",
                source_sha256: &"a".repeat(64),
                generated_names: &generated_names,
                used_field_ids: &used,
                include_values: false,
                source_warnings: &[],
            },
        )
        .expect("report");
        let text = std::fs::read_to_string(report).expect("report text");
        assert!(text.contains(&format!("Источник SHA-256: {}", "a".repeat(64))));
        assert!(text.contains("contract.number: [значение скрыто"));
        assert!(!text.contains("A-42"));
        assert!(!text.contains("unused.secret"));
        assert!(!text.contains("do-not-export"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn trusted_license_key_is_valid_base64_of_32_bytes() {
        let decoded = super::BASE64_STANDARD
            .decode(super::TRUSTED_LICENSE_PUBKEY_B64)
            .expect("embedded key must be valid base64");
        assert_eq!(decoded.len(), 32);
    }
}
