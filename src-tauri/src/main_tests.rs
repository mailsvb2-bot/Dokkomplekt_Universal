#[cfg(test)]
mod tests {
    use super::{
        canonical_json_bytes, capture_trust_document_evidence, current_year_utc,
        is_forbidden_public_download_host, is_forbidden_public_download_ip,
        load_or_create_local_data_key, local_trial_access_decision, normalized_picker_output,
        parse_semver, pdf_print_settings, plan_label, reject_parent_traversal,
        safe_update_file_name, signed_plan_to_product_plan, validate_printable_file,
        validate_update_url, write_trust_report, SourceProvenance, TrustReportContext,
        TRIAL_DOCUMENT_LIMIT_MONTH,
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
        let used = ["contract.number".to_string()];
        let generated_names = ["contract.docx".into()];
        let document_evidence = [capture_trust_document_evidence(
            "contract.docx",
            &semantic_case,
            used,
        )];
        let report = write_trust_report(
            &root,
            TrustReportContext {
                source_name: "source.docx",
                source_sha256: &"a".repeat(64),
                generated_names: &generated_names,
                document_evidence: &document_evidence,
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
