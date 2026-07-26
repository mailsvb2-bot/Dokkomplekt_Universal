use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SemanticAtom {
    Text(String),
    Integer(i64),
    Decimal(String),
    Date(String),
    Boolean(bool),
}
impl SemanticAtom {
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(v) | Self::Decimal(v) | Self::Date(v) => v.clone(),
            Self::Integer(v) => v.to_string(),
            Self::Boolean(v) => {
                if *v {
                    "true".into()
                } else {
                    "false".into()
                }
            }
        }
    }
}
pub type SemanticRecord = BTreeMap<String, SemanticAtom>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum DomainKind {
    #[default]
    Generic,
    Medical,
    Legal,
    Hr,
    Education,
    Accounting,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ValueSource {
    SafeDefault = 10,
    /// Proposed by a local semantic model and still subject to evidence,
    /// confidence and type-validation gates.
    Model = 15,
    Scanner = 20,
    SessionSelection = 30,
    UserConfirmed = 40,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueEvidence {
    pub source_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<String>,
    pub excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_index: Option<usize>,
    pub extractor: String,
    pub confidence: f32,
}

impl ValueEvidence {
    pub fn new(
        source_kind: impl Into<String>,
        excerpt: impl Into<String>,
        extractor: impl Into<String>,
        confidence: f32,
    ) -> Self {
        Self {
            source_kind: source_kind.into(),
            source_reference: None,
            excerpt: excerpt.into(),
            page_index: None,
            extractor: extractor.into(),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticValue {
    pub field_id: String,
    pub value: String,
    pub source: ValueSource,
    pub confidence: f32,
    #[serde(default)]
    pub evidence: Vec<ValueEvidence>,
}

impl SemanticValue {
    pub fn new(
        field_id: impl Into<String>,
        value: impl Into<String>,
        source: ValueSource,
        confidence: f32,
    ) -> Self {
        Self {
            field_id: field_id.into(),
            value: value.into(),
            source,
            confidence,
            evidence: Vec::new(),
        }
    }

    pub fn with_evidence(mut self, evidence: ValueEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticCase {
    pub values: BTreeMap<String, SemanticValue>,
    pub active_domains: Vec<DomainKind>,
    #[serde(default)]
    pub collections: BTreeMap<String, Vec<SemanticRecord>>,
    #[serde(default)]
    pub blocks: BTreeMap<String, String>,
}

impl SemanticCase {
    pub fn value(&self, field_id: &str) -> Option<&SemanticValue> {
        let canonical = crate::canonical_storage_field_id(field_id);
        let equivalents = crate::storage_equivalent_field_ids(&canonical);
        if !equivalents.is_empty() {
            let mut best = equivalents
                .iter()
                .filter_map(|candidate| self.values.get(*candidate))
                .filter(|value| !value.value.trim().is_empty())
                .collect::<Vec<_>>();
            best.sort_by(|left, right| {
                left.source
                    .cmp(&right.source)
                    .then_with(|| left.confidence.total_cmp(&right.confidence))
            });
            if let Some(value) = best.last() {
                return Some(value);
            }
        } else if let Some(value) = self
            .values
            .get(field_id)
            .filter(|value| !value.value.trim().is_empty())
        {
            return Some(value);
        }

        crate::contextual_fallback_field_ids(field_id)
            .iter()
            .filter_map(|candidate| self.values.get(*candidate))
            .filter(|value| !value.value.trim().is_empty())
            .max_by(|left, right| {
                left.source
                    .cmp(&right.source)
                    .then_with(|| left.confidence.total_cmp(&right.confidence))
            })
    }

    pub fn get(&self, field_id: &str) -> Option<&str> {
        self.value(field_id).map(|value| value.value.as_str())
    }

    pub fn has(&self, field_id: &str) -> bool {
        self.get(field_id).is_some()
    }
    pub fn collection(&self, id: &str) -> Option<&[SemanticRecord]> {
        self.collections.get(id).map(Vec::as_slice)
    }
    pub fn set_collection(&mut self, id: impl Into<String>, rows: Vec<SemanticRecord>) {
        self.collections.insert(id.into(), rows);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub id: String,
    pub title_ru: String,
    pub aliases: Vec<String>,
    pub domain: DomainKind,
    pub required_by_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainProfile {
    pub id: String,
    pub title: String,
    pub kind: DomainKind,
    pub fields: Vec<FieldDefinition>,
    pub workflow_rules: Vec<WorkflowRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowRule {
    RequireField {
        document_role: String,
        field_id: String,
    },
    RequireFieldWhenFlag {
        document_role: String,
        field_id: String,
        flag: String,
    },
    RequireFieldUnlessPresent {
        document_role: String,
        field_id: String,
        unless_field: String,
    },
    SkipForRole {
        document_role: String,
        field_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateAnalysis {
    pub title: String,
    pub suggested_button_label: String,
    pub placeholders: Vec<String>,
    pub unknown_placeholders: Vec<String>,
    pub domain_scores: BTreeMap<String, usize>,
    pub role_id: String,
    pub is_static: bool,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub template_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptInputKind {
    #[default]
    Text,
    LongText,
    Date,
    Number,
    Money,
    Inn,
    Kpp,
    Ogrn,
    Snils,
    Passport,
    Vin,
    Icd10,
    Select,
    YesNo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptAskMode {
    #[default]
    IfMissing,
    Confirm,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupFieldConfig {
    pub field_id: String,
    pub title: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub input_kind: PromptInputKind,
    #[serde(default)]
    pub ask_mode: PromptAskMode,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub allow_custom_option: bool,
    #[serde(default)]
    pub help_text: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
    /// Optional source field whose value is copied into this field while the specialist
    /// has not edited the linked field independently. This ports the donor behavior where
    /// a commission date initially fills protocol/related dates but each remains editable.
    #[serde(default)]
    pub linked_to: Option<String>,
    #[serde(default)]
    pub order: i32,
}

impl PopupFieldConfig {
    pub fn new(field_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            field_id: field_id.into(),
            title: title.into(),
            required: false,
            input_kind: PromptInputKind::Text,
            ask_mode: PromptAskMode::IfMissing,
            options: Vec::new(),
            allow_custom_option: false,
            help_text: None,
            section: None,
            default_value: None,
            linked_to: None,
            order: 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentTemplateSpec {
    pub id: String,
    pub button_label: String,
    pub template_path: String,
    pub category: DomainKind,
    pub role_id: String,
    pub required_fields: Vec<String>,
    pub placeholders: Vec<String>,
    pub is_static_copy: bool,
    #[serde(default)]
    pub popup_fields: Vec<PopupFieldConfig>,
    /// True after the specialist explicitly saved the popup designer. When enabled,
    /// optional profession defaults may be removed; strict required fields are still restored.
    #[serde(default)]
    pub popup_configured: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentPack {
    pub pack_id: String,
    pub name: String,
    pub documents: Vec<DocumentTemplateSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateButton {
    pub document_id: String,
    pub label: String,
    pub ready: bool,
    pub blocked_reason: Option<String>,
    pub role_id: String,
    pub category: DomainKind,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowFlags {
    pub sick_leave_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptSpec {
    pub field_id: String,
    pub title: String,
    pub required: bool,
    pub current_value: Option<String>,
    pub validation_hint: Option<String>,
    #[serde(default)]
    pub input_kind: PromptInputKind,
    #[serde(default)]
    pub ask_mode: PromptAskMode,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub allow_custom_option: bool,
    #[serde(default)]
    pub help_text: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub linked_to: Option<String>,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowPlan {
    pub document_id: String,
    pub prompts: Vec<PromptSpec>,
    pub blocked: bool,
    pub block_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderResult {
    pub output_text: String,
    pub missing_fields: Vec<String>,
    pub unknown_fields: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub template_errors: Vec<String>,
}
