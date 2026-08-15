//! Pure business core for Dokkomplekt Universal.
//!
//! This crate must stay UI-free and filesystem-light. It is safe to test without Tauri.

pub mod approval_blocks;
pub mod automation_quality;
pub mod bundle_decision;
pub mod button_registry;
pub mod case_segmentation;
pub mod conflicts;
pub mod content_matching;
pub mod core;
pub mod corpus_recorder;
pub mod created_documents;
pub mod data_schema_engine;
pub mod date_parser;
pub mod diary_engine;
pub mod document_generation;
pub mod document_routing;
pub mod domain_plugin_layer;
pub mod domain_profiles;
pub mod domains;
pub mod field_aliases;
pub mod field_registry;
pub mod functional_port;
pub mod icd10_catalog;
pub mod intake_agent;
pub mod kit_learning;
mod label_search;
pub mod legacy_parity;
pub mod legacy_template_inference;
pub mod mail_merge;
pub mod medical_profile;
pub mod output_engine;
pub mod output_naming;
pub mod popup_engine;
pub mod popup_profiles;
pub mod print_triage;
pub mod priority;
pub mod product_access;
pub mod professional_records;
pub mod record_series;
pub mod required_blocks;
pub mod rewrite_audit;
pub mod scanner_engine;
pub mod semantic_engine;
pub mod semantic_llm;
pub mod source_classification;
pub mod source_parser;
pub mod template_engine;
pub mod template_intelligence;
pub mod template_intelligence_engine;
pub mod template_wizard;
pub mod types;
pub mod universal_behavior_port;
pub mod universal_pipeline;
pub mod validators;
pub mod workflow_engine;
pub mod workflow_scenario_engine;

pub use approval_blocks::*;
pub use automation_quality::*;
pub use bundle_decision::*;
pub use button_registry::*;
pub use case_segmentation::*;
pub use conflicts::*;
pub use content_matching::*;
pub use data_schema_engine::{
    is_safe_field_id as is_safe_unified_field_id, normalize_field_id as normalize_unified_field_id,
    set_unified_value, UnifiedConflict, UnifiedDataSchema, UnifiedFieldDefinition,
    UnifiedFieldKind, UnifiedFieldValue, UnifiedValueSource,
};
pub use date_parser::*;
pub use diary_engine::*;
pub use document_generation::*;
pub use document_routing::*;
pub use domain_plugin_layer::{
    builtin_domain_plugins_v2, plugin_by_id, DomainPluginId, DomainPluginV2, RequiredFieldRuleV2,
};
pub use domain_profiles::*;
pub use domains::accounting::AccountingProfile;
pub use domains::custom::CustomProfile;
pub use domains::education::EducationProfile;
pub use domains::hr::HrProfile;
pub use domains::legal::LegalProfile;
pub use domains::medical::MedicalProfile;
pub use field_aliases::*;
pub use field_registry::*;
pub use functional_port::{
    append_diary_signatures, build_diary_schedule, build_ported_output_folder_name,
    create_button_from_template_text, format_rvk_district, parse_legacy_source_text,
    ported_detect_field_conflict, ported_workflow_plan, render_ported_template,
    select_diary_text_by_diagnosis, title_for_ported_field, validate_prompt_answers,
    OutputNamingOptions, PortedFieldConflict, PortedParseReport,
};
pub use icd10_catalog::*;
pub use intake_agent::*;
pub use kit_learning::*;
pub use legacy_parity::*;
pub use legacy_template_inference::*;
pub use mail_merge::*;
pub use medical_profile::*;
pub use output_engine::*;
pub use output_naming::*;
pub use popup_engine::*;
pub use popup_profiles::*;
pub use print_triage::*;
pub use priority::*;
pub use professional_records::*;
pub use record_series::*;
pub use rewrite_audit::*;
pub use scanner_engine::*;
pub use source_classification::*;
pub use source_parser::*;
pub use template_engine::*;
pub use template_intelligence::*;
pub use template_intelligence_engine::{
    analyze_template_structure_v2, TemplateInputZone, TemplateSignatureInfo,
    TemplateStructureAnalysisV2, TemplateTableInfo,
};
pub use template_wizard::*;
pub use types::*;
pub use universal_behavior_port::{
    apply_semantic_date, build_construction_contract, candidate_signature, case_get, case_set,
    clinical_calendar_diary_schedule, confirm_semantic_date, decide_agent_launch,
    default_calendar_diary_schedule, diary_minute_schedule_from_choice, scan_universal_text,
    semantic_date_key_from_prompt, CandidateSignatureInput, ConstructionContract, DateConflict,
    DiaryScheduleSpec, LaunchDecision, PrimaryDirection, SemanticDateStore, UniversalScan,
    UNIVERSAL_BEHAVIOR_PORT_VERSION,
};
pub use universal_pipeline::{
    required_fields_for_domain, run_universal_constructor_pipeline, UniversalDomain,
    UniversalPipelineFlags, UniversalPipelineInput, UniversalPipelineResult,
};
pub use validators::*;
pub use workflow_engine::*;
pub use workflow_scenario_engine::{
    build_button_scenario_v2, validate_scenario_answers_v2, ButtonScenarioV2,
    WorkflowFieldRequirementV2, WorkflowFlagSetV2,
};

pub use corpus_recorder::*;
pub use created_documents::{
    attention_file_name, build_attention_text, plan_created_documents_batch, ConfiguredDocument,
    CreatedDocumentsBatch, PlannedOutput, ATTENTION_SUFFIX, ATTENTION_TITLE,
};
pub use domains::medical_document_plan::{
    build_deep_diary_calendar, build_medical_render_plan, normalize_institution_text,
    DeepDiaryEntry, DiaryCalendarOptions, MedicalDocumentRole, MedicalRenderPlan,
};
pub use product_access::{
    evaluate_entitlement, no_patient_data_keys_in_license_state, validate_vip_access_code,
    AccessDecision, AccessMode, LicenseEntitlement, PlanLimits, ProductPlanId,
    EXPIRED_DEMO_WATERMARK_TEXT, PRODUCT_ACCESS_CONTRACT_VERSION, TRIAL_WATERMARK_TEXT,
};
pub use required_blocks::{required_blocks_for, unmet_blocks, BlockRequirement, RequiredBlock};
pub use semantic_engine::{extract_semantic, ExtractedField, ExtractionReport, FieldType};
pub use semantic_llm::{
    apply_model_consensus_with_source, apply_model_output, apply_model_output_with_source,
    build_extraction_prompt, build_extraction_prompt_for_domain,
    build_extraction_prompt_for_domain_and_language, extract_understanding, extract_with_model,
    parse_model_extraction, parse_model_extraction_with_source, SemanticModel,
};
