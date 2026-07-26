use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalStorageSnapshot {
    pub user_profiles: Vec<String>,
    pub template_mappings: BTreeMap<String, String>,
    pub remembered_fields: BTreeMap<String, String>,
}
