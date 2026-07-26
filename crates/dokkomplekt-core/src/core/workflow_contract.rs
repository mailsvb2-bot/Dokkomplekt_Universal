use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Button {
    pub id: String,
    pub label: String,
    pub target_template_id: String,
    pub workflow_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub button_id: String,
    pub requires: Vec<String>,
    pub optional: Vec<String>,
    pub produces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPlanCore {
    pub workflow: Workflow,
    pub missing_required_fields: Vec<String>,
}

pub fn build_workflow(
    button: &Button,
    required_fields: Vec<String>,
    optional_fields: Vec<String>,
    output_format: &str,
) -> Workflow {
    Workflow {
        id: format!("workflow:{}", button.id),
        button_id: button.id.clone(),
        requires: required_fields,
        optional: optional_fields,
        produces: vec![output_format.to_string()],
    }
}
