export type DomainKind = 'Generic' | 'Medical' | 'Legal' | 'Hr' | 'Accounting' | 'Education' | { Custom: string };

export type WorkspaceInferenceLevel = 'high' | 'medium' | 'low';

export interface WorkspaceProfileEvidence {
  document_id: string;
  title: string;
  role_id: string;
  attributed_domain: DomainKind;
  score: number;
  field_ids: string[];
}

export interface WorkspaceProfileInference {
  suggested_domain?: DomainKind | null;
  confidence: number;
  level: WorkspaceInferenceLevel;
  auto_apply: boolean;
  mixed_domains: boolean;
  domain_scores: Record<string, number>;
  evidence: WorkspaceProfileEvidence[];
  reasons: string[];
}


export interface WorkspaceFieldUsage {
  field_id: string;
  title: string;
  document_ids: string[];
}

export interface WorkspaceDocumentRole {
  document_id: string;
  title: string;
  role_id: string;
  role_label: string;
  domain: DomainKind;
  field_ids: string[];
  local_field_ids: string[];
  group_id: string;
}

export interface WorkspaceDocumentGroup {
  group_id: string;
  title: string;
  domain: DomainKind;
  document_ids: string[];
}

export interface WorkspaceDocumentRelation {
  left_document_id: string;
  right_document_id: string;
  kind: string;
  label: string;
}

export interface WorkspaceWorkflowShape {
  primary_object: string;
  common_fields: WorkspaceFieldUsage[];
  local_fields: Record<string, WorkspaceFieldUsage[]>;
  documents: WorkspaceDocumentRole[];
  groups: WorkspaceDocumentGroup[];
  relations: WorkspaceDocumentRelation[];
  mixed_workflows: boolean;
  reasons: string[];
}

export type PromptInputKind = 'text' | 'long_text' | 'date' | 'number' | 'money' | 'inn' | 'kpp' | 'ogrn' | 'snils' | 'passport' | 'vin' | 'icd10' | 'select' | 'yes_no';
export type PromptAskMode = 'if_missing' | 'confirm' | 'always';

export interface PopupFieldConfig {
  field_id: string;
  title: string;
  required: boolean;
  input_kind: PromptInputKind;
  ask_mode: PromptAskMode;
  options: string[];
  allow_custom_option: boolean;
  help_text?: string | null;
  section?: string | null;
  default_value?: string | null;
  linked_to?: string | null;
  order: number;
}

export interface DocumentTemplateSpec {
  id: string;
  button_label: string;
  template_path: string;
  category: DomainKind;
  role_id: string;
  required_fields: string[];
  placeholders: string[];
  is_static_copy: boolean;
  popup_fields?: PopupFieldConfig[];
  popup_configured?: boolean;
}

export interface DocumentPack {
  pack_id: string;
  name: string;
  documents: DocumentTemplateSpec[];
}

export interface PromptSpec {
  field_id: string;
  title: string;
  required: boolean;
  skippable?: boolean;
  current_value?: string | null;
  validation_hint?: string | null;
  input_kind?: PromptInputKind;
  ask_mode?: PromptAskMode;
  options?: string[];
  allow_custom_option?: boolean;
  help_text?: string | null;
  section?: string | null;
  linked_to?: string | null;
  order?: number;
}

export interface WorkflowPlan {
  document_id: string;
  prompts: PromptSpec[];
  blocked: boolean;
  block_reasons: string[];
}

export interface ProcessBlueprint {
  process_id: string;
  domain: string;
  locale: string;
  title: string;
  description: string;
  template_slots: string[];
  high_risk_fields: string[];
  validators: string[];
}
export interface ProcessBlueprintState {
  selected_process_id?: string | null;
  processes: ProcessBlueprint[];
  notice: string;
}

export interface FirstRunStateResponse {
  pack: DocumentPack;
  has_user_buttons: boolean;
  message: string;
}

export interface ValueEvidence {
  source_kind: string;
  source_reference?: string | null;
  excerpt: string;
  page_index?: number | null;
  extractor: string;
  confidence: number;
}

export interface SemanticValue {
  field_id: string;
  value: string;
  source: string;
  confidence: number;
  evidence?: ValueEvidence[];
}

export interface SemanticCase {
  values: Record<string, SemanticValue>;
  collections?: Record<string, Array<Record<string, unknown>>>;
  blocks?: Record<string, string>;
  skipped_fields?: string[];
}

export type OrganizationKnowledgeCategory = 'organization' | 'employee' | 'position' | 'signatory' | 'department' | 'counter' | 'print_form' | 'authority' | 'template_rule';
export interface OrganizationKnowledgeRecord {
  record_id: string;
  category: OrganizationKnowledgeCategory;
  label: string;
  fields: Record<string, string>;
  valid_from?: string | null;
  valid_until?: string | null;
  active: boolean;
  note: string;
  updated_at: string;
}

export interface BusinessRegistryRecord {
  inn: string;
  name: string;
  kpp?: string | null;
  ogrn?: string | null;
  legal_address?: string | null;
  director?: string | null;
  status?: string | null;
  source: string;
  source_updated_at?: string | null;
}

export interface BusinessRegistryImportResult {
  total_records: number;
  imported_records: number;
  replaced: boolean;
}

export interface ParsedSourceReport {
  recognized_title?: string | null;
  warnings: string[];
}

export interface DocumentMatch {
  document_id: string;
  button_label: string;
  role_id: string;
  score: number;
  evidence: string[];
}

export interface DocumentRoutingRecommendation {
  domain: DomainKind;
  domain_confidence: number;
  predicted_role?: string | null;
  cluster_id: string;
  cluster_confidence: number;
  recommended_document_ids: string[];
  matches: DocumentMatch[];
  auto_select: boolean;
  review_required: boolean;
  reasons: string[];
}

export type BundleDecisionSource =
  | 'specialist_confirmation'
  | 'promoted_learning_rule'
  | 'deterministic_route'
  | 'review_proposal'
  | 'ambiguous_candidates'
  | 'no_safe_proposal';

export interface BundleDecision {
  document_ids: string[];
  source: BundleDecisionSource;
  confidence: number;
  auto_apply: boolean;
  review_required: boolean;
  question?: string | null;
  reasons: string[];
}

export interface ParseSourceResponse {
  semantic_case: SemanticCase;
  report: ParsedSourceReport;
  routing: DocumentRoutingRecommendation;
  bundle_decision: BundleDecision;
}

export interface LayoutBoundingBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface NormalizedLayoutItem {
  item_kind: 'text_line' | 'table_row' | string;
  page_index: number | null;
  block_index: number | null;
  text: string;
  cells: string[];
  bbox: LayoutBoundingBox | null;
  confidence: number;
  source_reference: string | null;
}

export interface ParseSourceFileResponse extends ParseSourceResponse {
  source_text: string;
  source_path: string;
  source_kind: string;
  layout_items: NormalizedLayoutItem[];
}


export interface IntakeCapability {
  format: string;
  extensions: string[];
  ready: boolean;
  mode: string;
  detail: string;
}

export interface SidecarToolStatus {
  tool: string;
  available: boolean;
  bundled: boolean;
  state: 'bundled' | 'downloaded' | 'system' | 'missing';
  component_id: string | null;
  resolved_path: string;
  purpose: string;
}

export interface ComponentStatus {
  id: string;
  label: string;
  description: string;
  target: string;
  size_bytes: number;
  size_label: string;
  unlocks: string[];
  state: 'bundled' | 'downloaded' | 'system' | 'missing';
  installed: boolean;
  available: boolean;
  catalog_available: boolean;
  message: string;
}

export interface ComponentProgress {
  id: string;
  phase: string;
  downloaded_bytes: number;
  total_bytes: number;
  percent: number;
  message: string;
}

export interface ParseWebSourceResponse extends ParseSourceResponse {
  source_text: string;
  final_url: string;
  content_type: string;
}

export interface DocumentTemplateTextResponse {
  template_text: string;
}

export interface CreatedDocumentOutput {
  document_id: string;
  label: string;
  path: string;
}

export interface RenderDocxBatchResult {
  output_folder: string;
  created_files: string[];
  created_documents?: CreatedDocumentOutput[];
  warnings?: string[];
  backup_folder?: string | null;
}

export interface RenderResult {
  output_text?: string;
  missing_fields?: string[];
  unknown_fields?: string[];
  warnings?: string[];
  template_errors?: string[];
  output_path?: string;
}

export interface PopupAnswerDto {
  field_id: string;
  value: string;
  continue_without_value?: boolean;
}

export interface PopupApplyResult {
  accepted: boolean;
  semantic_case: SemanticCase;
  still_missing: PromptSpec[];
  message: string;
  errors?: string[];
}

export interface UpdateCheckResponse {
  available: boolean;
  current_version: string;
  latest_version: string;
  platform: string;
  message: string;
  notes?: string | null;
  verified_package_path?: string | null;
  sha256?: string | null;
  size_bytes?: number | null;
}

export interface ProductAccessResponse {
  accepted: boolean;
  mode: string;
  plan: string;
  reason: string;
  watermark?: string | null;
  document_limit_month: number;
  max_documents_per_run: number;
  documents_used_month: number;
  documents_left_month: number;
}


export type FieldRisk = 'low' | 'medium' | 'high' | 'critical';

export interface AutomationBlockerDto {
  field_id: string;
  value: string;
  risk: FieldRisk;
  confidence: number;
  required_confidence: number;
  reason: string;
}

export interface PrintFieldDiff {
  field_id: string;
  value: string;
  source: string;
  confidence: number;
  risk: FieldRisk;
  evidence: string[];
  status: string;
}

export interface PrintTriageReport {
  decision: 'auto_print' | 'review_fields' | 'hold_for_review' | string;
  auto_print_allowed: boolean;
  confidence_score: number;
  checked_document_ids: string[];
  unapproved_document_ids: string[];
  missing_fields: string[];
  blockers: AutomationBlockerDto[];
  diff: PrintFieldDiff[];
  reasons: string[];
}

export interface TemplateApprovalRecord {
  document_id: string;
  template_sha256: string;
  jurisdiction: string;
  approved_by: string;
  approved_at: string;
  note: string;
}

export interface PrintJobDto {
  path: string;
  copies: number;
}

export interface PrintPreferences {
  printer_name?: string | null;
  duplex_mode: 'simplex' | 'long_edge' | 'short_edge' | 'manual' | string;
  tray?: number | null;
}

export interface PrinterInfo {
  name: string;
  is_default: boolean;
  driver: string;
  port: string;
}

export interface PrinterInventory {
  platform: string;
  printers: PrinterInfo[];
  preferences: PrintPreferences;
  discovery_error?: string | null;
  advanced_options_note: string;
}

export interface PrintFailure {
  path: string;
  requested_copies: number;
  queued_copies: number;
  error: string;
}

export interface PrintFilesResponse {
  queued_files: string[];
  queued_copies: number;
  failed_files: PrintFailure[];
}

export interface PdfExportFailure { path: string; error: string; }
export interface ExportPdfResponse {
  created_files: string[];
  failed_files: PdfExportFailure[];
  pdfa_1_requested: boolean;
  conformance_note: string;
}

export interface KedoPackageDocument {
  file_name: string;
  sha256: string;
  size_bytes: number;
  detached_signature_name: string;
}

export interface CreateKedoPackageResponse {
  package_folder: string;
  manifest_path: string;
  checksum_path: string;
  documents: KedoPackageDocument[];
  conformance_note: string;
}

export interface GeneratedPrintItem {
  document_id: string;
  label: string;
  path: string;
}

export interface GeneratedOutput {
  folder: string | null;
  files: string[];
  source: 'single' | 'batch' | 'zero_touch' | 'watcher';
  print_items?: GeneratedPrintItem[];
}

export interface OutputPreferences {
  output_root: string;
  folder_parts: FolderNamePartDto[];
  naming_confirmed: boolean;
}

export interface BackgroundWatcherPlan {
  platform: string;
  installed: boolean;
  watch_folder?: string | null;
  output_root?: string | null;
  folder_parts?: FolderNamePartDto[];
  auto_print?: boolean;
  print_copies_by_document?: Record<string, number>;
  max_parallel_cases?: number;
  migration_required?: boolean;
  executable?: string;
  args?: string[];
  autostart_files?: string[];
  removed_files?: string[];
  commands?: string[];
  warnings?: string[];
}

export interface ImportTemplateFileResult {
  template_path: string;
  extracted_text: string;
}

export interface Icd10Suggestion {
  code: string;
  title: string;
  category?: string;
}

export interface TemplateCandidateDto {
  document_id: string;
  template_path: string;
  extracted_text: string;
  preferred_button_label?: string | null;
  domain_override?: DomainKind | null;
}

export interface TemplateConfirmationRowDto {
  document_id: string;
  template_path: string;
  detected_title: string;
  suggested_button_label: string;
  editable_button_label: string;
  role_id: string;
  is_static_copy: boolean;
  analysis: unknown;
  popup_fields?: PopupFieldConfig[];
  popup_fields_edited?: boolean;
  domain_override?: DomainKind | null;
  domain_override_is_explicit?: boolean;
  workspace_inference?: WorkspaceProfileInference;
  workspace_shape?: WorkspaceWorkflowShape;
}

export interface ScannerMarkDto {
  field_id: string;
  selected_text: string;
  page_index: number;
  confidence: number;
}

export interface DiaryEntryPlanDto {
  day_number: number;
  date: string;
  month: number;
  year: number;
}

export type SeriesCadenceDto =
  | { kind: 'daily' }
  | { kind: 'day_offsets'; value: number[] }
  | { kind: 'fixed_times'; value: string[] }
  | { kind: 'minute_interval'; value: number }
  | { kind: 'day_offsets_fixed_times'; value: { day_offsets: number[]; times: string[] } }
  | { kind: 'day_offsets_minute_interval'; value: { day_offsets: number[]; minutes: number } };

export interface SeriesPlanRequestDto {
  start_date: string;
  end_date: string;
  default_year: number;
  start_offset_days: number;
  cadence: SeriesCadenceDto;
  day_start_time?: string | null;
  day_end_time?: string | null;
  skip_weekdays: number[];
  excluded_dates: string[];
}

export interface SeriesEntryPlanDto {
  sequence: number;
  offset_days: number;
  date: string;
  time?: string | null;
  datetime: string;
}

export type FolderNamePartDto =
  | 'FullSubjectName'
  | 'ShortInitials'
  | 'SurnameGivenName'
  | 'OrganizationName'
  | 'DocumentNumber'
  | 'DocumentDate'
  | 'PeriodStartDate'
  | 'PeriodEndDate'
  | 'PeriodRange'
  | 'PeriodStartMonth'
  | 'PeriodEndMonth'
  | 'ShortPeriodStartDate'
  | 'ShortPeriodEndDate'
  | 'ShortPeriodRange'
  | 'PeriodStartMonthName'
  | 'PeriodEndMonthName'
  | 'AdmissionDate'
  | 'DischargeDate'
  | 'AdmissionAndDischargeDates'
  | 'AdmissionMonth'
  | 'DischargeMonth';

export interface OutputPlanDto {
  root_folder: string;
  patient_folder: string;
  files: string[];
  warnings: string[];
  exists: boolean;
}

export interface IntakeRouteResponse {
  should_start_ui: boolean;
  should_raise_existing_window: boolean;
  reason: string;
}

export interface ScannerApplyReportDto {
  applied_fields: string[];
  rejected_fields: string[];
}


export interface CreatedDocumentsIntakeResult {
  status: 'processed' | 'attention' | 'setup_needed' | 'ignored';
  patient_folder: string | null;
  created_files: string[];
  created_documents?: CreatedDocumentOutput[];
  missing: string[];
  attention_file: string | null;
  message: string;
  print_triage?: PrintTriageReport | null;
}

export interface SemanticExtractedField {
  field_id: string;
  value: string;
  confidence: number;
  method: string;
  source?: string;
  evidence?: string[];
}

export interface SemanticExtractResult {
  fields: SemanticExtractedField[];
  warnings: string[];
  model_applied: boolean;
  prompt: string;
}


export interface ClauseBlockRecord { block_id: string; title: string; content: string; updated_at: string; }
export interface TemplateVersionRecord {
  version_id: string;
  document_id: string;
  version_number: number;
  template_path: string;
  template_sha256: string;
  note: string;
  status: 'published' | 'archived' | string;
  created_at: string;
}
export interface LearnedTemplateField {
  field_id: string;
  title: string;
  line_index: number;
  label_prefix: string;
  blank_line: string;
  common_prefix: string;
  common_suffix: string;
  example_values: string[];
  source_matches: string[];
  placeholder: string;
  confidence: number;
  required: boolean;
  condition?: string | null;
}
export interface TemplateDiffHunk {
  line_index: number;
  blank_line: string;
  example_lines: string[];
  common_prefix: string;
  common_suffix: string;
  variable_values: string[];
}
export interface ImportLearningExampleFileResult {
  source_path: string;
  source_kind: string;
  extracted_text: string;
  warnings: string[];
}
export interface TemplateLearningReport {
  locale: string;
  fields: LearnedTemplateField[];
  immutable_lines: number[];
  conditional_lines: number[];
  repeated_line_groups: number[][];
  structure: unknown;
  diff: TemplateDiffHunk[];
  confidence: number;
  requires_confirmation: boolean;
  warnings: string[];
}
export type TemplateRegressionSeverity = 'info' | 'warning' | 'critical';
export interface TemplateRegressionIssue {
  code: string;
  severity: TemplateRegressionSeverity;
  message: string;
}
export interface DocxStructuralFingerprint {
  placeholders: string[];
  text_parts: string[];
  part_sha256: Record<string, string>;
  table_count: number;
  row_count: number;
  cell_count: number;
  section_count: number;
  page_break_count: number;
  content_control_count: number;
  field_count: number;
  header_count: number;
  footer_count: number;
}
export interface TemplateRegressionReport {
  previous: DocxStructuralFingerprint;
  candidate: DocxStructuralFingerprint;
  issues: TemplateRegressionIssue[];
  critical: boolean;
}
export interface TemplateLearningMapField {
  field_id: string;
  line_index: number;
  blank_line: string;
  common_prefix: string;
  common_suffix: string;
}
export interface TemplateLearningMapReport {
  output_path: string;
  applied_field_ids: string[];
  skipped_field_ids: string[];
  warnings: string[];
}
export interface TemplateMarkupCandidate { field_id: string; title: string; value: string; confidence: number; occurrences: number; selected_by_default: boolean; }
export type TemplateMarkupAction = 'replace' | 'insert_after';
export interface TemplateMarkupReplacement { field_id: string; value: string; action?: TemplateMarkupAction; }
export interface TemplateMarkupReport { output_path: string; replacement_count: number; replaced_occurrences: number; skipped_values: string[]; }
export interface MailMergeTable { delimiter: string; headers: string[]; canonical_headers: string[]; rows: string[][]; warnings: string[]; }
export interface PrepareMailMergeFileResult { delimited_text: string; table: MailMergeTable; }
export interface RenderMailMergeResult { output_folder: string; row_count: number; created_files: string[]; warnings?: string[]; }

export type GuidedScannerMode = 'source' | 'template';
export type GuidedScannerMarkupAction = 'replace' | 'insert_after';

export interface WordScannerSession {
  session_id: string;
  mode: GuidedScannerMode;
  original_path: string;
  opened_path: string;
  working_copy: boolean;
  word_was_running: boolean;
  automation_available: boolean;
  message: string;
}

export interface WordScannerCapture {
  session_id: string;
  selected_text: string;
  context_text: string;
  before_text: string;
  after_text: string;
  selection_start: number;
  selection_end: number;
  expanded_from_cursor: boolean;
  document_path: string;
  document_closed: boolean;
}

export interface WordScannerApplyResult {
  session_id: string;
  output_path: string;
  selected_text: string;
  placeholder: string;
  extracted_text: string;
  document_closed: boolean;
}

export interface LearnedScannerRule {
  rule_id: string;
  field_id: string;
  title: string;
  label_hint: string;
  before_text: string;
  after_text: string;
  sample_value: string;
  input_kind: PromptInputKind;
  created_at: string;
  layout_fingerprint?: string | null;
  successful_applications?: number;
  last_applied_at?: string | null;
  learning_status?: 'shadow' | 'promoted' | 'rejected';
  shadow_observations?: number;
  shadow_agreements?: number;
  shadow_conflicts?: number;
  promoted_at?: string | null;
}


export interface PrivacyPreferences {
  copy_source_to_output: boolean;
  write_trust_report: boolean;
  include_values_in_trust_report: boolean;
  temp_retention_hours: number;
  archive_processed_sources: boolean;
  archive_folder_name: string;
  service_note_retention_days: number;
  processed_marker_retention_days: number;
  archived_source_retention_days: number;
}

export interface WorkspaceHygieneReport {
  archived_processed_sources: string[];
  archived_service_files: string[];
  removed_orphan_markers: string[];
  removed_expired_archived_files: string[];
  warnings: string[];
}

export interface LocalSemanticModelConfig {
  enabled: boolean;
  provider: 'ollama' | 'llama_cpp' | string;
  endpoint: string;
  model: string;
  preferred_language: string;
  timeout_seconds: number;
  shadow_mode: boolean;
  corpus_recording_enabled: boolean;
  auto_apply_zero_touch: boolean;
  consistency_passes: number;
}

export interface CorpusStatus {
  recording_enabled: boolean;
  entry_count: number;
  privacy_mode: string;
  message: string;
}

export interface CorpusExportResponse {
  output_path: string;
  entry_count: number;
  schema: string;
}

export interface CalibratedThresholdStatus {
  installed: boolean;
  domain: string;
  generated_at: string;
  imported_at: string;
  corpus_sha256: string;
  auto_min_confidence: number;
  review_min_confidence: number;
  max_auto_error_rate: number;
  training_observations: number;
  holdout_observations: number;
  message: string;
}

export interface LocalSemanticModelStatus {
  configured: boolean;
  reachable: boolean;
  provider: string;
  endpoint: string;
  model: string;
  available_models: string[];
  message: string;
}

export interface SemanticModelConfigurationResponse {
  config: LocalSemanticModelConfig;
  status: LocalSemanticModelStatus;
}

export interface ReferenceDataStatus {
  installed: boolean;
  cached: boolean;
  restart_required: boolean;
  source: string;
  published_at: string | null;
  complete_years: number[];
  listed_years: number[];
  message: string;
}

export interface CaseRunRecord {
  case_id: string;
  source_sha256: string;
  source_path: string;
  status: 'received' | 'normalizing' | 'recognizing' | 'checking' | 'attention' | 'ready' | 'generating' | 'publishing' | 'completed' | 'failed' | 'cancelled' | string;
  request_json: string;
  output_root: string;
  patient_folder?: string | null;
  created_files_json: string;
  missing_json: string;
  last_error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface AutomationExceptionRecord {
  exception_id: string;
  category: string;
  source_path: string;
  message: string;
  details_json: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface QualityTelemetryBucket { key: string; count: number; }
export interface QualityRuleSuggestion {
  suggestion_id: string;
  title: string;
  reason: string;
  observations: number;
  auto_enabled: boolean;
  requires_specialist_confirmation: boolean;
}
export interface QualityTelemetryReport {
  generated_at: string;
  stop_reasons: QualityTelemetryBucket[];
  unrecognized_fields: QualityTelemetryBucket[];
  broken_templates: QualityTelemetryBucket[];
  excluded_documents: QualityTelemetryBucket[];
  repeated_confirmations: QualityTelemetryBucket[];
  suggestions: QualityRuleSuggestion[];
  privacy_mode: string;
}

export interface DailyAutomationDashboard {
  date_utc: string;
  processed_cases: number;
  automatically_completed_cases: number;
  attention_cases: number;
  failed_cases: number;
  generated_documents: number;
  printed_documents: number;
  measured_processing_milliseconds: number;
}

export interface AutomationMetrics {
  processed_sources: number;
  generated_documents: number;
  blocked_sources: number;
  failed_sources: number;
  print_failures: number;
  user_confirmations: number;
  zero_touch_sources: number;
  attention_resolutions: number;
  model_grounding_rejections: number;
  shadow_model_runs: number;
  shadow_model_proposals: number;
  shadow_model_agreements: number;
  reused_documents?: number;
  rerendered_documents?: number;
  processing_milliseconds?: number;
  print_review_queued?: number;
  automatic_print_approved?: number;
}

export interface QueueStatus {
  mode: 'central_mtls' | 'central_postgres_local' | 'shared_filesystem' | string;
  configured: boolean;
  reachable: boolean;
  message: string;
}

export interface AuditEventRecord {
  event_id: string;
  event_type: string;
  object_hash: string;
  detail_json: string;
  previous_hash: string;
  event_hash: string;
  created_at: string;
}
