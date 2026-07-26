//! Reusable, profession-neutral signature and approval blocks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalLine {
    pub role: String,
    pub name: String,
}

pub fn append_approval_block(text: &str, lines: &[ApprovalLine]) -> String {
    let mut out = text.trim_end().to_string();
    for line in lines {
        let rendered = format!(
            "{} __________________ / {} /",
            line.role.trim(),
            line.name.trim()
        );
        if !out.contains(&rendered) {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&rendered);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_approval_is_idempotent_and_not_medical_only() {
        let lines = vec![ApprovalLine {
            role: "Руководитель проекта".into(),
            name: "А. А. Автор".into(),
        }];
        let once = append_approval_block("Отчёт", &lines);
        assert_eq!(append_approval_block(&once, &lines), once);
    }
}
