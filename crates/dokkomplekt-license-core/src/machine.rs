use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineFacts {
    pub os: String,
    pub hostname: Option<String>,
    pub machine_guid: Option<String>,
    pub install_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineFingerprint(pub String);

impl MachineFingerprint {
    pub fn from_facts(facts: &MachineFacts) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(facts.os.trim().to_lowercase().as_bytes());
        hasher.update(b"|");
        if let Some(value) = &facts.hostname {
            hasher.update(value.trim().to_lowercase().as_bytes());
        }
        hasher.update(b"|");
        if let Some(value) = &facts.machine_guid {
            hasher.update(value.trim().to_lowercase().as_bytes());
        }
        hasher.update(b"|");
        if let Some(value) = &facts.install_id {
            hasher.update(value.trim().to_lowercase().as_bytes());
        }
        Self(hex::encode(hasher.finalize()))
    }

    pub fn matches_any(&self, allowed: &[String]) -> bool {
        allowed
            .iter()
            .any(|item| constant_time_eq(self.0.as_bytes(), item.as_bytes()))
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}
