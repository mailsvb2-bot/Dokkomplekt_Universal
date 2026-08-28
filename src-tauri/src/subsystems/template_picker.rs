const MAX_PICKED_TEMPLATE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct PickTemplateFilesRequest {
    #[serde(default)]
    initial_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct PickedTemplateFile {
    file_name: String,
    template_path: String,
    extracted_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    import_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct PickTemplateFilesResponse {
    files: Vec<PickedTemplateFile>,
}

/// Open the operating-system file chooser and import the selected Word templates
/// directly from their real filesystem paths. This deliberately avoids the hidden
/// HTML `<input type=file>` path, which is not reliable enough in packaged Windows
/// WebView2 applications for the product's first-run action.
#[tauri::command]
async fn pick_template_files(
    req: PickTemplateFilesRequest,
    app: tauri::AppHandle,
) -> Result<PickTemplateFilesResponse, String> {
    let selected_paths = tauri::async_runtime::spawn_blocking(move || {
        pick_template_files_blocking(req.initial_path)
    })
    .await
    .map_err(|error| format!("Не удалось открыть выбор шаблонов: {error}"))??;

    if selected_paths.is_empty() {
        return Ok(PickTemplateFilesResponse { files: Vec::new() });
    }

    let templates_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("user-templates");
    std::fs::create_dir_all(&templates_dir)
        .map_err(|error| format!("Не удалось создать папку пользовательских шаблонов: {error}"))?;

    let files = import_picked_templates(selected_paths, &templates_dir);
    Ok(PickTemplateFilesResponse { files })
}

fn import_picked_templates(
    selected_paths: Vec<PathBuf>,
    templates_dir: &Path,
) -> Vec<PickedTemplateFile> {
    selected_paths
        .into_iter()
        .map(|source_path| match import_picked_template(&source_path, templates_dir) {
            Ok(file) => file,
            Err(error) => PickedTemplateFile {
                file_name: source_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Неизвестный шаблон")
                    .to_string(),
                template_path: String::new(),
                extracted_text: String::new(),
                import_error: Some(error),
            },
        })
        .collect()
}

fn import_picked_template(
    source_path: &Path,
    templates_dir: &Path,
) -> Result<PickedTemplateFile, String> {
    let canonical = source_path
        .canonicalize()
        .map_err(|error| format!("Не удалось открыть шаблон «{}»: {error}", source_path.display()))?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|error| format!("Не удалось прочитать шаблон «{}»: {error}", canonical.display()))?;
    if !metadata.is_file() {
        return Err(format!("Выбранный путь не является файлом: {}", canonical.display()));
    }
    if metadata.len() > MAX_PICKED_TEMPLATE_BYTES {
        return Err(format!(
            "Шаблон «{}» слишком большой: максимум 50 МБ.",
            canonical.display()
        ));
    }

    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "У выбранного шаблона нет расширения DOCX/DOCM.".to_string())?;
    if extension != "docx" && extension != "docm" {
        return Err(format!(
            "Поддерживаются только DOCX и DOCM: {}",
            canonical.display()
        ));
    }

    // Validate the original file before copying it into app-data so macros,
    // embedded objects and external relationships are rejected while the
    // original Mark-of-the-Web and provenance are still intact.
    validate_safe_template_file(&canonical).map_err(|error| {
        format!(
            "Шаблон «{}» содержит активное содержимое или внешние связи и заблокирован: {error}. Сохраните безопасную копию как DOCX.",
            canonical.display()
        )
    })?;
    let extracted_text = extract_docx_text(&canonical)
        .map_err(|error| format!("Файл не распознан как DOCX: {error}"))?;

    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Имя выбранного шаблона не поддерживается системой.".to_string())?
        .to_string();
    let stem = canonical
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("template");
    let safe_stem = sanitize_picker_component(stem);
    let target = templates_dir.join(format!(
        "{}_{}.{}",
        Uuid::new_v4(),
        if safe_stem.is_empty() { "template" } else { &safe_stem },
        extension
    ));
    std::fs::copy(&canonical, &target).map_err(|error| {
        format!(
            "Не удалось сохранить безопасную копию шаблона «{}»: {error}",
            canonical.display()
        )
    })?;

    Ok(PickedTemplateFile {
        file_name,
        template_path: target.display().to_string(),
        extracted_text,
        import_error: None,
    })
}

fn sanitize_picker_component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_alphanumeric() || matches!(ch, '-' | '_' | ' ') {
            output.push(ch);
        } else {
            output.push('_');
        }
    }
    output.trim().trim_matches('.').to_string()
}

fn pick_template_files_blocking(initial_path: Option<String>) -> Result<Vec<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    let _ = initial_path;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Выберите шаблоны Word'
$dialog.Filter = 'Шаблоны Word (*.docx;*.docm)|*.docx;*.docm'
$dialog.Multiselect = $true
$dialog.CheckFileExists = $true
$dialog.CheckPathExists = $true
$dialog.RestoreDirectory = $true
if ($env:DOKKOMPLEKT_PICK_TEMPLATE_INITIAL -and (Test-Path -LiteralPath $env:DOKKOMPLEKT_PICK_TEMPLATE_INITIAL -PathType Container)) {
  $dialog.InitialDirectory = $env:DOKKOMPLEKT_PICK_TEMPLATE_INITIAL
}
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  foreach ($path in $dialog.FileNames) { [Console]::Out.WriteLine($path) }
}
"#;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-STA", "-Command", script])
            .env(
                "DOKKOMPLEKT_PICK_TEMPLATE_INITIAL",
                initial_path.as_deref().unwrap_or_default(),
            )
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| format!("Не удалось запустить системный выбор шаблонов: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Системный выбор шаблонов завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_picker_paths(&output.stdout)
    }

    #[cfg(target_os = "macos")]
    {
        let script = r#"
set chosenFiles to choose file with prompt "Выберите шаблоны Word" with multiple selections allowed
set outputText to ""
repeat with chosenFile in chosenFiles
  set outputText to outputText & (POSIX path of chosenFile) & linefeed
end repeat
return outputText
"#;
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|error| format!("Не удалось открыть системный выбор шаблонов: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("User canceled") || stderr.contains("-128") {
                return Ok(Vec::new());
            }
            return Err(format!(
                "Системный выбор шаблонов завершился с ошибкой: {}",
                stderr.trim()
            ));
        }
        parse_picker_paths(&output.stdout)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let initial = initial_path.filter(|value| Path::new(value).is_dir());
        let output = if picker_command_exists("zenity") {
            let mut command = std::process::Command::new("zenity");
            command.args([
                "--file-selection",
                "--multiple",
                "--separator=\n",
                "--title=Выберите шаблоны Word",
                "--file-filter=Шаблоны Word | *.docx *.docm",
            ]);
            if let Some(path) = initial.as_deref() {
                command.arg(format!("--filename={}/", path.trim_end_matches('/')));
            }
            command.output()
        } else if picker_command_exists("kdialog") {
            let mut command = std::process::Command::new("kdialog");
            command.args([
                "--getopenfilename",
                initial.as_deref().unwrap_or("."),
                "*.docx *.docm|Шаблоны Word",
                "--multiple",
                "--separate-output",
            ]);
            command.output()
        } else {
            return Err(
                "Системный выбор шаблонов недоступен: установите zenity или kdialog.".into(),
            );
        }
        .map_err(|error| format!("Не удалось открыть системный выбор шаблонов: {error}"))?;
        if !output.status.success() {
            if output.status.code() == Some(1) {
                return Ok(Vec::new());
            }
            return Err(format!(
                "Системный выбор шаблонов завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_picker_paths(&output.stdout)
    }
}


fn pick_source_file_blocking(initial_path: Option<String>) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    let _ = initial_path;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Выберите исходный документ'
$dialog.Filter = 'Поддерживаемые документы|*.docx;*.docm;*.doc;*.ppt;*.pptx;*.pdf;*.jpg;*.jpeg;*.png;*.tif;*.tiff;*.bmp;*.webp;*.xlsx;*.xls;*.ods;*.odt;*.rtf;*.txt;*.md;*.csv;*.tsv;*.json;*.xml;*.html;*.htm;*.eml;*.msg;*.zip;*.7z;*.rar|Все файлы (*.*)|*.*'
$dialog.Multiselect = $false
$dialog.CheckFileExists = $true
$dialog.CheckPathExists = $true
$dialog.RestoreDirectory = $true
if ($env:DOKKOMPLEKT_PICK_SOURCE_INITIAL -and (Test-Path -LiteralPath $env:DOKKOMPLEKT_PICK_SOURCE_INITIAL -PathType Container)) {
  $dialog.InitialDirectory = $env:DOKKOMPLEKT_PICK_SOURCE_INITIAL
}
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::Out.WriteLine($dialog.FileName)
}
"#;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-STA", "-Command", script])
            .env(
                "DOKKOMPLEKT_PICK_SOURCE_INITIAL",
                initial_path.as_deref().unwrap_or_default(),
            )
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| format!("Не удалось запустить системный выбор исходника: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Системный выбор исходника завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_source_picker_path(&output.stdout)
    }

    #[cfg(target_os = "macos")]
    {
        let script = r#"
try
  set chosenFile to choose file with prompt "Выберите исходный документ"
  return POSIX path of chosenFile
on error number -128
  return ""
end try
"#;
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|error| format!("Не удалось открыть системный выбор исходника: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Системный выбор исходника завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_source_picker_path(&output.stdout)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let initial = initial_path.filter(|value| Path::new(value).is_dir());
        let output = if picker_command_exists("zenity") {
            let mut command = std::process::Command::new("zenity");
            command.args([
                "--file-selection",
                "--title=Выберите исходный документ",
                "--file-filter=Документы | *.docx *.docm *.doc *.ppt *.pptx *.pdf *.jpg *.jpeg *.png *.tif *.tiff *.bmp *.webp *.xlsx *.xls *.ods *.odt *.rtf *.txt *.md *.csv *.tsv *.json *.xml *.html *.htm *.eml *.msg *.zip *.7z *.rar",
            ]);
            if let Some(path) = initial.as_deref() {
                command.arg(format!("--filename={}/", path.trim_end_matches('/')));
            }
            command.output()
        } else if picker_command_exists("kdialog") {
            let mut command = std::process::Command::new("kdialog");
            command.args([
                "--getopenfilename",
                initial.as_deref().unwrap_or("."),
                "*.docx *.docm *.doc *.ppt *.pptx *.pdf *.jpg *.jpeg *.png *.tif *.tiff *.bmp *.webp *.xlsx *.xls *.ods *.odt *.rtf *.txt *.md *.csv *.tsv *.json *.xml *.html *.htm *.eml *.msg *.zip *.7z *.rar|Документы",
            ]);
            command.output()
        } else {
            return Err(
                "Системный выбор исходника недоступен: установите zenity или kdialog.".into(),
            );
        }
        .map_err(|error| format!("Не удалось открыть системный выбор исходника: {error}"))?;
        if !output.status.success() {
            if output.status.code() == Some(1) {
                return Ok(None);
            }
            return Err(format!(
                "Системный выбор исходника завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_source_picker_path(&output.stdout)
    }
}

fn parse_source_picker_path(output: &[u8]) -> Result<Option<PathBuf>, String> {
    let text = String::from_utf8(output.to_vec())
        .map_err(|_| "Системный выбор исходника вернул некорректный UTF-8.".to_string())?;
    let raw = text.lines().find(|line| !line.trim().is_empty());
    let Some(raw) = raw else { return Ok(None); };
    let value = raw.trim().trim_matches('\u{feff}').trim();
    if value.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(value);
    let extension = path
        .extension()
        .and_then(|part| part.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    const SUPPORTED: &[&str] = &[
        "docx", "docm", "doc", "ppt", "pptx", "pdf", "jpg", "jpeg", "png", "tif",
        "tiff", "bmp", "webp", "xlsx", "xls", "ods", "odt", "rtf", "txt", "md", "csv",
        "tsv", "json", "xml", "html", "htm", "eml", "msg", "zip", "7z", "rar",
    ];
    if !SUPPORTED.contains(&extension.as_str()) {
        return Err(format!(
            "Системный выбор вернул неподдерживаемый исходник: {}",
            path.display()
        ));
    }
    Ok(Some(path))
}

fn parse_picker_paths(output: &[u8]) -> Result<Vec<PathBuf>, String> {
    let text = String::from_utf8(output.to_vec())
        .map_err(|_| "Системный выбор шаблонов вернул некорректный UTF-8.".to_string())?;
    let mut paths = Vec::new();
    for raw in text.lines() {
        let value = raw.trim().trim_matches('\u{feff}').trim();
        if value.is_empty() {
            continue;
        }
        let path = PathBuf::from(value);
        let extension = path
            .extension()
            .and_then(|part| part.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(extension.as_deref(), Some("docx" | "docm")) {
            return Err(format!(
                "Системный выбор вернул неподдерживаемый файл: {}",
                path.display()
            ));
        }
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn picker_command_exists(name: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v -- {name} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod template_picker_tests {
    use super::*;

    #[test]
    fn parses_multiple_picker_paths_and_preserves_spaces() {
        let paths = parse_picker_paths("C:/Шаблоны/Акт работ.docx\r\nC:/Шаблоны/Счёт.docm\r\n".as_bytes());
        assert_eq!(paths.as_ref().map(Vec::len), Ok(2));
        assert!(paths.as_ref().is_ok_and(|items| {
            items.first().is_some_and(|path| path.ends_with("Акт работ.docx"))
                && items.get(1).is_some_and(|path| path.ends_with("Счёт.docm"))
        }));
    }

    #[test]
    fn rejects_non_word_picker_output() {
        let result = parse_picker_paths(b"C:/tmp/template.pdf\n");
        assert!(result.as_ref().is_err_and(|error| error.contains("неподдерживаемый")));
    }

    #[test]
    fn parses_supported_source_picker_path_and_preserves_spaces() {
        let path = parse_source_picker_path("\u{feff}C:/Работа/Исходный документ.docx\r\n".as_bytes())
            .expect("source picker output must parse")
            .expect("source path must be present");
        assert!(path.ends_with("Исходный документ.docx"));
    }

    #[test]
    fn source_picker_cancel_is_not_an_error() {
        assert_eq!(parse_source_picker_path(b"\r\n"), Ok(None));
    }

    #[test]
    fn source_picker_rejects_unsupported_extension() {
        let result = parse_source_picker_path(b"C:/tmp/source.exe\n");
        assert!(result
            .as_ref()
            .is_err_and(|error| error.contains("неподдерживаемый исходник")));
    }

    #[test]
    fn one_broken_template_does_not_discard_other_selected_templates() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-template-picker-partial-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let templates_dir = root.join("imported");
        std::fs::create_dir_all(&templates_dir).unwrap();
        let good = root.join("Хороший шаблон.docx");
        let broken = root.join("Повреждённый шаблон.docx");
        dokkomplekt_docx::create_docx_from_text(&good, "АКТ ВЫПОЛНЕННЫХ РАБОТ").unwrap();
        std::fs::write(&broken, b"not-a-docx").unwrap();

        let files = import_picked_templates(vec![good, broken], &templates_dir);

        assert_eq!(files.len(), 2);
        let good = files.iter().find(|file| file.file_name == "Хороший шаблон.docx").unwrap();
        assert!(good.import_error.is_none());
        assert!(!good.template_path.is_empty());
        assert!(Path::new(&good.template_path).is_file());
        let broken = files
            .iter()
            .find(|file| file.file_name == "Повреждённый шаблон.docx")
            .unwrap();
        assert!(broken.import_error.as_deref().is_some_and(|error| !error.is_empty()));
        assert!(broken.template_path.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
