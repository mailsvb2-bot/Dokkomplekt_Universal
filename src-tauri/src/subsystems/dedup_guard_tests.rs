#[cfg(test)]
mod dedup_guard_contract_tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dokkomplekt-dedup-{label}-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    #[test]
    fn shared_completion_guard_distinguishes_absence_validity_and_corruption() {
        let root = root("shared");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("Исходник.docx");
        std::fs::write(&source, b"source").unwrap();
        let job = "a".repeat(64);

        assert!(!shared_completion_receipt_matches(&source, &job).unwrap());
        let path = mark_shared_completion(&source, &job).unwrap();
        assert!(shared_completion_receipt_matches(&source, &job).unwrap());

        std::fs::write(&path, b"schema=1\nsha256=wrong\n").unwrap();
        assert!(shared_completion_receipt_matches(&source, &job).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shared_completion_guard_rejects_wrong_filesystem_type() {
        let root = root("wrong-type");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("Исходник.docx");
        std::fs::write(&source, b"source").unwrap();
        let job = "b".repeat(64);
        let path = shared_completion_receipt(&source, &job);
        std::fs::create_dir_all(&path).unwrap();

        assert!(shared_completion_receipt_matches(&source, &job).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
