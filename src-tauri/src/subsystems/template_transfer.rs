
const TEMPLATE_TRANSFER_SCHEMA: &str = "dokkomplekt.template-transfer.v1";
const MAX_TRANSFER_TEMPLATE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateTransferDocument {
    id: String,
    button_label: String,
    category: DomainKind,
    role_id: String,
    required_fields: Vec<String>,
    placeholders: Vec<String>,
    is_static_copy: bool,
    popup_fields: Vec<PopupFieldConfig>,
    popup_configured: bool,
    template_entry: String,
    template_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateTransferManifest {
    schema: String,
    pack_name: String,
    documents: Vec<TemplateTransferDocument>,
}

#[derive(Debug, Deserialize)]
struct ExportTemplateTransferRequest {
    #[serde(default)]
    output_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExportTemplateTransferResponse {
    package_path: String,
    document_count: usize,
    package_sha256: String,
}

#[derive(Debug, Deserialize)]
struct ImportTemplateTransferRequest {
    package_path: String,
}

#[derive(Debug, Serialize)]
struct ImportTemplateTransferResponse {
    pack: DocumentPack,
    document_count: usize,
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_transfer_entry_name(entry: &str) -> Result<(), String> {
    if !entry.starts_with("templates/")
        || entry.contains("..")
        || entry.contains('\\')
        || entry.starts_with('/')
    {
        return Err(format!("Недопустимый путь внутри пакета шаблонов: {entry}"));
    }
    Ok(())
}

fn export_template_transfer_to_path(
    pack: &DocumentPack,
    output_path: &Path,
    app: &tauri::AppHandle,
) -> Result<ExportTemplateTransferResponse, String> {
    if pack.documents.is_empty() {
        return Err("Нет шаблонов для переноса.".into());
    }
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Не удалось создать папку экспорта: {error}"))?;
    }

    let mut manifest_documents = Vec::with_capacity(pack.documents.len());
    let mut template_payloads = Vec::with_capacity(pack.documents.len());
    for (index, document) in pack.documents.iter().enumerate() {
        let source = resolve_user_path(app, &document.template_path)?;
        validate_safe_template_file(&source).map_err(|error| {
            format!(
                "Шаблон «{}» нельзя перенести: {error}",
                document.button_label
            )
        })?;
        let bytes = std::fs::read(&source).map_err(|error| {
            format!(
                "Не удалось прочитать шаблон «{}»: {error}",
                document.button_label
            )
        })?;
        if bytes.len() > MAX_TRANSFER_TEMPLATE_BYTES {
            return Err(format!(
                "Шаблон «{}» превышает лимит 50 МБ.",
                document.button_label
            ));
        }
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "docx".into());
        if extension != "docx" && extension != "docm" {
            return Err(format!(
                "Шаблон «{}» имеет неподдерживаемый формат.",
                document.button_label
            ));
        }
        let entry = format!("templates/{index:04}.{extension}");
        let template_sha256 = sha256_bytes(&bytes);
        manifest_documents.push(TemplateTransferDocument {
            id: document.id.clone(),
            button_label: document.button_label.clone(),
            category: document.category.clone(),
            role_id: document.role_id.clone(),
            required_fields: document.required_fields.clone(),
            placeholders: document.placeholders.clone(),
            is_static_copy: document.is_static_copy,
            popup_fields: document.popup_fields.clone(),
            popup_configured: document.popup_configured,
            template_entry: entry.clone(),
            template_sha256,
        });
        template_payloads.push((entry, bytes));
    }

    let manifest = TemplateTransferManifest {
        schema: TEMPLATE_TRANSFER_SCHEMA.into(),
        pack_name: pack.name.clone(),
        documents: manifest_documents,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;

    let file = std::fs::File::create(output_path)
        .map_err(|error| format!("Не удалось создать пакет переноса: {error}"))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    archive
        .start_file("manifest.json", options)
        .map_err(|error| format!("Не удалось записать manifest пакета: {error}"))?;
    archive
        .write_all(&manifest_bytes)
        .map_err(|error| format!("Не удалось записать manifest пакета: {error}"))?;
    for (entry, bytes) in template_payloads {
        archive
            .start_file(entry, options)
            .map_err(|error| format!("Не удалось добавить шаблон в пакет: {error}"))?;
        archive
            .write_all(&bytes)
            .map_err(|error| format!("Не удалось записать шаблон в пакет: {error}"))?;
    }
    archive
        .finish()
        .map_err(|error| format!("Не удалось завершить пакет переноса: {error}"))?;

    let package_bytes = std::fs::read(output_path)
        .map_err(|error| format!("Не удалось проверить созданный пакет: {error}"))?;
    Ok(ExportTemplateTransferResponse {
        package_path: output_path.display().to_string(),
        document_count: manifest.documents.len(),
        package_sha256: sha256_bytes(&package_bytes),
    })
}

fn import_template_transfer_from_path(
    package_path: &Path,
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<ImportTemplateTransferResponse, String> {
    let file = std::fs::File::open(package_path)
        .map_err(|error| format!("Не удалось открыть пакет шаблонов: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Файл не является пакетом шаблонов: {error}"))?;

    let manifest: TemplateTransferManifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .map_err(|_| "В пакете шаблонов отсутствует manifest.json.".to_string())?;
        if entry.size() > 2 * 1024 * 1024 {
            return Err("Manifest пакета шаблонов слишком большой.".into());
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Не удалось прочитать manifest пакета: {error}"))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Некорректный manifest пакета шаблонов: {error}"))?
    };
    if manifest.schema != TEMPLATE_TRANSFER_SCHEMA {
        return Err(format!(
            "Неподдерживаемая версия пакета шаблонов: {}",
            manifest.schema
        ));
    }
    if manifest.documents.is_empty() {
        return Err("Пакет шаблонов не содержит документов.".into());
    }

    let import_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("user-templates")
        .join("transfers")
        .join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&import_root)
        .map_err(|error| format!("Не удалось создать папку импортированных шаблонов: {error}"))?;

    let mut imported_documents = Vec::with_capacity(manifest.documents.len());
    let mut drafts = Vec::with_capacity(manifest.documents.len());
    let result = (|| -> Result<(), String> {
        for (index, item) in manifest.documents.iter().enumerate() {
            validate_transfer_entry_name(&item.template_entry)?;
            if !is_sha256_hex(&item.template_sha256) {
                return Err(format!(
                    "Некорректный SHA-256 шаблона «{}».",
                    item.button_label
                ));
            }
            let mut entry = archive
                .by_name(&item.template_entry)
                .map_err(|_| format!("В пакете отсутствует шаблон «{}».", item.button_label))?;
            if entry.size() as usize > MAX_TRANSFER_TEMPLATE_BYTES {
                return Err(format!(
                    "Шаблон «{}» превышает лимит 50 МБ.",
                    item.button_label
                ));
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| format!("Не удалось прочитать шаблон из пакета: {error}"))?;
            let actual_sha256 = sha256_bytes(&bytes);
            if actual_sha256 != item.template_sha256 {
                return Err(format!(
                    "SHA-256 шаблона «{}» не совпадает с manifest.",
                    item.button_label
                ));
            }
            let extension = Path::new(&item.template_entry)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| "docx".into());
            if extension != "docx" && extension != "docm" {
                return Err(format!(
                    "Шаблон «{}» имеет неподдерживаемое расширение.",
                    item.button_label
                ));
            }
            // The manifest id is logical data, not a filesystem component. Keep the physical\n            // storage name entirely application-owned so a hostile transfer package cannot\n            // smuggle path separators or parent components through document.id.\n            let target = import_root.join(format!("{index:04}.{extension}"));
            std::fs::write(&target, &bytes)
                .map_err(|error| format!("Не удалось сохранить импортированный шаблон: {error}"))?;
            validate_safe_template_file(&target).map_err(|error| {
                format!(
                    "Импортированный шаблон «{}» заблокирован проверкой безопасности: {error}",
                    item.button_label
                )
            })?;

            let document = DocumentTemplateSpec {
                id: item.id.clone(),
                button_label: item.button_label.clone(),
                template_path: target.display().to_string(),
                category: item.category.clone(),
                role_id: item.role_id.clone(),
                required_fields: item.required_fields.clone(),
                placeholders: item.placeholders.clone(),
                is_static_copy: item.is_static_copy,
                popup_fields: item.popup_fields.clone(),
                popup_configured: item.popup_configured,
            };
            let draft = prepare_template_version_draft(
                app,
                &document.id,
                &target,
                &actual_sha256,
                "Импорт из переносимого пакета шаблонов.",
                None,
            )?;
            imported_documents.push(document);
            drafts.push(draft);
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&import_root);
        return Err(error);
    }

    let imported_name = if manifest.pack_name.trim().is_empty() {
        "Импортированные шаблоны".to_string()
    } else {
        manifest.pack_name.trim().to_string()
    };
    let (pack, versions) = publish_pack_with_template_versions(app, state, &drafts, |pack| {
        pack.name = imported_name.clone();
        pack.documents = imported_documents.clone();
        Ok(())
    })?;
    if versions.len() != pack.documents.len() {
        return Err("Не все импортированные шаблоны получили опубликованную версию.".into());
    }
    Ok(ImportTemplateTransferResponse {
        document_count: pack.documents.len(),
        pack,
    })
}

#[tauri::command]
fn export_template_transfer(
    req: ExportTemplateTransferRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ExportTemplateTransferResponse, String> {
    let output_path = if let Some(raw) = req.output_path.as_deref().filter(|value| !value.trim().is_empty()) {
        resolve_user_visible_absolute_path(raw, "Файл переноса шаблонов")?
    } else {
        app.path()
            .desktop_dir()
            .map_err(|error| error.to_string())?
            .join(format!("Доккомплект-шаблоны-{}.dktpack", OffsetDateTime::now_utc().unix_timestamp()))
    };
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    export_template_transfer_to_path(&pack, &output_path, &app)
}


#[derive(Debug, Serialize)]
struct PickTemplateTransferResponse {
    selected_path: Option<String>,
}

fn pick_template_transfer_file_blocking() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Выберите пакет шаблонов Доккомплект'
$dialog.Filter = 'Пакет шаблонов Доккомплект (*.dktpack)|*.dktpack'
$dialog.Multiselect = $false
$dialog.CheckFileExists = $true
$dialog.CheckPathExists = $true
$dialog.RestoreDirectory = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::Out.Write($dialog.FileName)
}
"#;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-STA", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| format!("Не удалось открыть выбор пакета шаблонов: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Выбор пакета шаблонов завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!raw.is_empty()).then_some(raw));
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "try",
                "-e",
                "set f to choose file with prompt \"Выберите пакет шаблонов Доккомплект\"",
                "-e",
                "POSIX path of f",
                "-e",
                "on error number -128",
                "-e",
                "return \"\"",
                "-e",
                "end try",
            ])
            .output()
            .map_err(|error| format!("Не удалось открыть выбор пакета шаблонов: {error}"))?;
        if !output.status.success() {
            return Err("Выбор пакета шаблонов завершился с ошибкой.".into());
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!raw.is_empty()).then_some(raw));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(None)
    }
}

#[tauri::command]
async fn pick_template_transfer_file() -> Result<PickTemplateTransferResponse, String> {
    let selected_path = tauri::async_runtime::spawn_blocking(pick_template_transfer_file_blocking)
        .await
        .map_err(|error| format!("Не удалось открыть выбор пакета шаблонов: {error}"))??;
    Ok(PickTemplateTransferResponse { selected_path })
}

#[tauri::command]
fn import_template_transfer(
    req: ImportTemplateTransferRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ImportTemplateTransferResponse, String> {
    let package_path = resolve_user_visible_absolute_path(&req.package_path, "Пакет шаблонов")?;
    import_template_transfer_from_path(&package_path, &app, &state)
}

#[cfg(test)]
mod template_transfer_tests {
    use super::*;

    #[test]
    fn transfer_entry_rejects_zip_slip_shapes() {
        for value in ["../x.docx", "templates/../x.docx", "/templates/x.docx", "templates\\x.docx"] {
            assert!(validate_transfer_entry_name(value).is_err(), "{value}");
        }
        assert!(validate_transfer_entry_name("templates/0000.docx").is_ok());
    }

    #[test]
    fn manifest_shape_contains_no_semantic_case_or_source_paths() {
        let document = TemplateTransferDocument {
            id: "invoice".into(),
            button_label: "Счёт".into(),
            category: DomainKind::Accounting,
            role_id: "invoice".into(),
            required_fields: vec!["document.number".into()],
            placeholders: vec!["document.number".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
            template_entry: "templates/0000.docx".into(),
            template_sha256: "a".repeat(64),
        };
        let json = serde_json::to_string(&TemplateTransferManifest {
            schema: TEMPLATE_TRANSFER_SCHEMA.into(),
            pack_name: "Рабочий набор".into(),
            documents: vec![document],
        })
        .unwrap();
        assert!(!json.contains("semantic_case"));
        assert!(!json.contains("source_path"));
        assert!(!json.contains("template_path"));
        assert!(!json.contains("patient"));
        assert!(json.contains("templates/0000.docx"));
    }
}
