fn validate_printable_file(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("Файл для печати не найден: {}", path.display()));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "doc" | "docx" | "docm" | "pdf" | "rtf") {
        Ok(())
    } else {
        Err(format!(
            "Печать файла .{extension} не поддерживается: {}",
            path.display()
        ))
    }
}

#[cfg(target_os = "windows")]
fn shell_execute_path(path: &Path, verb: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt as _;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation = OsStr::new(verb)
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let file = path
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows ShellExecute({verb}) не запустил обработчик файла {} (код {code})",
            path.display()
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn shell_execute_path(path: &Path, verb: &str) -> Result<(), String> {
    let program = match (std::env::consts::OS, verb) {
        (_, "print") => "lp",
        ("macos", _) => "open",
        _ => "xdg-open",
    };
    let status = std::process::Command::new(program)
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| format!("Не удалось запустить {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} завершился с кодом {status}"))
    }
}

#[cfg(target_os = "windows")]
fn print_word_document_copies(
    path: &Path,
    copies: u16,
    preferences: &PrintPreferences,
) -> Result<(), String> {
    let expected = powershell_quote(&path.display().to_string());
    let printer = powershell_quote(
        preferences
            .printer_name
            .as_deref()
            .unwrap_or_default()
            .trim(),
    );
    let tray_setup = preferences.tray.map_or_else(String::new, |tray| {
        format!(
            "$doc.PageSetup.FirstPageTray = {tray}\n    $doc.PageSetup.OtherPagesTray = {tray}"
        )
    });
    let manual_duplex_ps = if preferences.duplex_mode == "manual" {
        "$true"
    } else {
        "$false"
    };
    let duplex_mode = powershell_quote(&preferences.duplex_mode);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$expected = [IO.Path]::GetFullPath('{expected}')
$printer = '{printer}'
$duplexMode = '{duplex_mode}'
$word = $null
$printServer = $null
$printQueue = $null
$previousTicket = $null
try {{
  # For hardware duplex, temporarily set the current user's queue ticket and
  # restore it after Word synchronously submits the job. This avoids relying on
  # an undocumented Word-only flag for long/short-edge duplex.
  if ($duplexMode -in @('long_edge', 'short_edge')) {{
    if ([string]::IsNullOrWhiteSpace($printer)) {{ throw 'Для аппаратного duplex нужно выбрать конкретный принтер.' }}
    Add-Type -AssemblyName System.Printing
    $printServer = New-Object System.Printing.LocalPrintServer
    $printQueue = $printServer.GetPrintQueue($printer)
    $previousTicket = $printQueue.UserPrintTicket.Clone()
    $ticket = $printQueue.UserPrintTicket.Clone()
    if ($duplexMode -eq 'long_edge') {{
      $ticket.Duplexing = [System.Printing.Duplexing]::TwoSidedLongEdge
    }} else {{
      $ticket.Duplexing = [System.Printing.Duplexing]::TwoSidedShortEdge
    }}
    $printQueue.UserPrintTicket = $ticket
    $printQueue.Commit()
  }}

  # Printing uses an isolated hidden Word instance so an already-open user
  # session is never hidden, reconfigured or closed.
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  if (-not [string]::IsNullOrWhiteSpace($printer)) {{ $word.ActivePrinter = $printer }}
  $doc = $word.Documents.Open($expected, $false, $true)
  try {{
    {tray_setup}
    # One COM call means one spooler job with the requested copy count instead
    # of N ShellExecute launches racing each other. ManualDuplexPrint is used
    # only for the explicit manual mode; hardware duplex uses PrintTicket above.
    $doc.PrintOut($false, $false, 0, '', '', '', 0, {copies}, '', 0, $false, $true, $null, {manual_duplex_ps})
  }} finally {{
    $doc.Close(0)
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($doc)
  }}
}} finally {{
  if ($null -ne $word) {{
    $word.Quit()
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($word)
  }}
  if ($null -ne $printQueue -and $null -ne $previousTicket) {{
    try {{
      $printQueue.UserPrintTicket = $previousTicket
      $printQueue.Commit()
    }} catch {{ }}
  }}
  if ($null -ne $printQueue) {{ $printQueue.Dispose() }}
  if ($null -ne $printServer) {{ $printServer.Dispose() }}
  [GC]::Collect()
  [GC]::WaitForPendingFinalizers()
}}
'{{"queued":true,"copies":{copies}}}'
"#
    );
    run_hidden_powershell(&script).map(|_| ())
}


#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
fn pdf_print_settings(copies: u16, preferences: &PrintPreferences) -> Vec<String> {
    let mut settings = vec![format!("{copies}x"), "ignore-pdf-print-settings".into()];
    match preferences.duplex_mode.as_str() {
        "long_edge" => settings.push("duplexlong".into()),
        "short_edge" => settings.push("duplexshort".into()),
        _ => settings.push("simplex".into()),
    }
    if let Some(tray) = preferences.tray {
        settings.push(format!("bin={tray}"));
    }
    settings
}

#[cfg(target_os = "windows")]
fn print_pdf_with_sumatra(
    path: &Path,
    copies: u16,
    preferences: &PrintPreferences,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let executable = universal_intake::resolve_tool("sumatrapdf");
    if !executable.is_file() {
        return Err(
            "Для полностью управляемой печати PDF установите или упакуйте SumatraPDF sidecar; системный PDF-handler не гарантирует duplex и лоток.".into(),
        );
    }
    let settings = pdf_print_settings(copies, preferences);
    let mut command = std::process::Command::new(&executable);
    command.arg("-silent");
    if let Some(printer) = preferences
        .printer_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        command.arg("-print-to").arg(printer);
    } else {
        command.arg("-print-to-default");
    }
    let output = command
        .arg("-print-settings")
        .arg(settings.join(","))
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|error| format!("Не удалось запустить SumatraPDF: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let code = output.status.code().unwrap_or(-1);
    let category = match code {
        2 => "PDF не открыт или формат не поддержан",
        3 => "PDF запрещает печать",
        4 => "принтер не найден",
        5 => "ошибка принтера или драйвера",
        6 => "печать запрещена политикой",
        _ => "неизвестная ошибка печати",
    };
    Err(if details.is_empty() {
        format!("SumatraPDF: {category} (код {code}).")
    } else {
        format!("SumatraPDF: {category} (код {code}): {details}")
    })
}

#[cfg(not(target_os = "windows"))]
fn print_pdf_with_lp(
    path: &Path,
    copies: u16,
    preferences: &PrintPreferences,
) -> Result<(), String> {
    let mut command = std::process::Command::new("lp");
    command.arg("-n").arg(copies.to_string());
    if let Some(printer) = preferences
        .printer_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        command.arg("-d").arg(printer);
    }
    match preferences.duplex_mode.as_str() {
        "long_edge" => {
            command.args(["-o", "sides=two-sided-long-edge"]);
        }
        "short_edge" => {
            command.args(["-o", "sides=two-sided-short-edge"]);
        }
        _ => {}
    }
    if let Some(tray) = preferences.tray {
        command.arg("-o").arg(format!("media-source={tray}"));
    }
    let output = command
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|error| format!("Не удалось запустить lp: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if details.is_empty() {
            Err(format!("lp завершился с кодом {}", output.status))
        } else {
            Err(format!("lp не принял PDF: {details}"))
        }
    }
}

fn convert_office_document_to_pdf(
    path: &Path,
    pdfa_1: bool,
) -> Result<(PathBuf, PathBuf), String> {
    validate_printable_file(path)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "doc" | "docx" | "docm" | "rtf" | "odt") {
        return Err(format!(
            "Экспорт в PDF поддерживается для DOC, DOCX, DOCM, RTF и ODT: {}",
            path.display()
        ));
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let output_dir =
        std::env::temp_dir().join(format!("dokkomplekt-pdf-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Не удалось создать временную папку PDF: {error}"))?;
    let converter = universal_intake::resolve_tool("soffice");
    let filter = if pdfa_1 {
        r#"pdf:writer_pdf_Export:{"SelectPdfVersion":{"type":"long","value":"1"},"UseTaggedPDF":{"type":"boolean","value":"true"}}"#
    } else {
        "pdf"
    };
    let mut command = std::process::Command::new(&converter);
    command
        .args([
            "--headless",
            "--nologo",
            "--nodefault",
            "--nolockcheck",
            "--nofirststartwizard",
            "--convert-to",
            filter,
            "--outdir",
        ])
        .arg(&output_dir)
        .arg(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().map_err(|error| {
        let _ = std::fs::remove_dir_all(&output_dir);
        format!("Для экспорта нужен LibreOffice/soffice: {error}")
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() { stderr } else { stdout };
        let _ = std::fs::remove_dir_all(&output_dir);
        return Err(if details.is_empty() {
            format!("LibreOffice не преобразовал {} в PDF.", path.display())
        } else {
            format!("LibreOffice не преобразовал документ в PDF: {details}")
        });
    }
    let pdf = std::fs::read_dir(&output_dir)
        .map_err(|error| format!("Не удалось прочитать результат PDF-конвертации: {error}"))?
        .flatten()
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        })
        .ok_or_else(|| {
            let _ = std::fs::remove_dir_all(&output_dir);
            "LibreOffice завершился без создания PDF.".to_string()
        })?;
    let header = std::fs::read(&pdf)
        .map_err(|error| format!("Не удалось проверить созданный PDF: {error}"))?;
    if !header.starts_with(b"%PDF-") {
        let _ = std::fs::remove_dir_all(&output_dir);
        return Err("Конвертер создал файл без корректной PDF-сигнатуры.".into());
    }
    Ok((pdf, output_dir))
}

#[cfg(not(target_os = "windows"))]
fn print_word_document_copies(
    path: &Path,
    copies: u16,
    preferences: &PrintPreferences,
) -> Result<(), String> {
    // CUPS does not universally understand DOC/DOCX/DOCM/RTF. Sending an Office
    // ZIP container directly to `lp` can print binary garbage or silently fail.
    // Convert to PDF first, then submit the PDF to the spooler.
    let (pdf, output_dir) = convert_office_document_to_pdf(path, false)?;
    let result = print_pdf_with_lp(&pdf, copies, preferences);
    let _ = std::fs::remove_dir_all(&output_dir);
    result
}

fn print_path_copies(
    path: &Path,
    copies: u16,
    preferences: &PrintPreferences,
) -> Result<(), String> {
    validate_printable_file(path)?;
    if copies == 0 {
        return Ok(());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "doc" | "docx" | "docm" | "rtf") {
        return print_word_document_copies(path, copies, preferences);
    }

    #[cfg(not(target_os = "windows"))]
    if extension == "pdf" {
        return print_pdf_with_lp(path, copies, preferences);
    }

    #[cfg(target_os = "windows")]
    if extension == "pdf" {
        return print_pdf_with_sumatra(path, copies, preferences);
    }

    // Other registered printable formats do not have a universal copies-aware
    // shell contract, so retain the OS print verb as a fallback.
    for _ in 0..copies {
        shell_execute_path(path, "print")?;
    }
    Ok(())
}

fn render_docx_with_assets(
    app: &tauri::AppHandle,
    template_path: &Path,
    output_path: &Path,
    case: &SemanticCase,
    strict: bool,
    watermark: Option<&str>,
) -> Result<dokkomplekt_core::RenderResult, String> {
    let template_text = extract_docx_text(template_path).map_err(|error| error.to_string())?;
    let image_fields = template_image_requests(&template_text);
    let result = render_docx_file_with_watermark(
        template_path,
        output_path,
        case,
        strict,
        watermark,
    )
    .map_err(|error| error.to_string())?;
    if image_fields.is_empty() {
        return Ok(result);
    }
    let mut assets = Vec::new();
    for field_id in image_fields {
        let value = case
            .values
            .get(&field_id)
            .map(|value| value.value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("Для изображения не заполнено поле {field_id}."))?;
        let path = resolve_user_path(app, value)?;
        if !path.is_file() {
            return Err(format!(
                "Изображение для поля {field_id} не найдено: {}",
                path.display()
            ));
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tif" | "tiff") {
            return Err(format!(
                "Поле {field_id}: поддерживаются PNG, JPEG, BMP, GIF и TIFF."
            ));
        }
        assets.push((field_id, path));
    }
    inject_word_image_assets(output_path, &assets)?;
    Ok(result)
}

fn inject_word_image_assets(document: &Path, assets: &[(String, PathBuf)]) -> Result<(), String> {
    inject_docx_images(document, assets).map_err(|error| {
        format!(
            "Не удалось встроить изображения непосредственно в DOCX {}: {error}",
            document.display()
        )
    })
}

fn open_path_in_file_manager(path: &Path) -> Result<(), String> {
    let target = if path.is_dir() {
        path
    } else if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        return Err(format!("Путь не найден: {}", path.display()));
    };
    shell_execute_path(target, "open")
}

#[cfg(target_os = "windows")]
fn powershell_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn run_hidden_powershell(script: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let encoded_bytes = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect::<Vec<_>>();
    let encoded = BASE64_STANDARD.encode(encoded_bytes);
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Не удалось запустить автоматизацию Microsoft Word: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if details.is_empty() {
            "Microsoft Word не ответил сканеру. Убедитесь, что Word установлен и документ открыт."
                .into()
        } else {
            format!("Microsoft Word не ответил сканеру: {details}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
fn word_process_running() -> bool {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("tasklist.exe")
        .args(["/FI", "IMAGENAME eq WINWORD.EXE", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_uppercase()
                .contains("WINWORD.EXE")
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn scanner_copy_path(app: &tauri::AppHandle, path: &Path) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("user-templates")
        .join("scanner-copies");
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("Не удалось подготовить папку безопасных копий: {error}"))?;
    let stem = sanitize_path_component(
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("document"),
    );
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx");
    Ok(base.join(format!(
        "{stem}.guided-{}.{}",
        &Uuid::new_v4().to_string()[..8],
        extension
    )))
}

fn clean_word_selection(value: &str) -> String {
    value
        .replace(['\r', '\n', '\u{0007}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|character: char| character.is_whitespace() || character == '\u{0007}')
        .to_string()
}

#[cfg(target_os = "windows")]
fn activate_word_document(path: &Path, timeout_seconds: u32) -> Result<(), String> {
    let expected = powershell_quote(&path.display().to_string());
    let timeout = timeout_seconds.max(1);
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$expected = [IO.Path]::GetFullPath('{expected}')
$deadline = (Get-Date).AddSeconds({timeout})
do {{
  try {{
    $word = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application')
    $target = $null
    for ($i = 1; $i -le $word.Documents.Count; $i++) {{
      $candidate = $word.Documents.Item($i)
      if ([String]::Equals([IO.Path]::GetFullPath([string]$candidate.FullName), $expected, [StringComparison]::OrdinalIgnoreCase)) {{
        $target = $candidate
        break
      }}
    }}
    if ($null -ne $target) {{
      $word.Visible = $true
      $target.Activate()
      '{{"ready":true}}'
      exit 0
    }}
  }} catch {{
    # Word may still be starting and registering its COM object.
  }}
  Start-Sleep -Milliseconds 250
}} while ((Get-Date) -lt $deadline)
throw 'Word не успел открыть документ. Закройте окно защищённого просмотра или повторите попытку.'
"#
    );
    run_hidden_powershell(&script).map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn activate_word_document(_path: &Path, _timeout_seconds: u32) -> Result<(), String> {
    Err("Автоматический сканер Word доступен только в Windows.".into())
}

fn close_word_document(path: &Path, word_was_running: bool, save: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let expected = powershell_quote(&path.display().to_string());
        let save_flag = if save { "-1" } else { "0" };
        let quit_flag = if word_was_running { "$false" } else { "$true" };
        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
$expected = [IO.Path]::GetFullPath('{expected}')
try {{
  $word = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application')
}} catch {{
  '{{"closed":true,"already_closed":true}}'
  exit 0
}}
$target = $null
for ($i = 1; $i -le $word.Documents.Count; $i++) {{
  $candidate = $word.Documents.Item($i)
  if ([String]::Equals([IO.Path]::GetFullPath([string]$candidate.FullName), $expected, [StringComparison]::OrdinalIgnoreCase)) {{ $target = $candidate; break }}
}}
if ($null -ne $target) {{ $target.Close({save_flag}) }}
if ({quit_flag} -and $word.Documents.Count -eq 0) {{ $word.Quit() }}
'{{"closed":true}}'
"#
        );
        run_hidden_powershell(&script).map(|_| ())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (path, word_was_running, save);
        Ok(())
    }
}

fn default_state_repository(app: &tauri::AppHandle) -> Result<LocalRepository, String> {
    repository_for(&default_state_db_path(app)?)
}

// Parallel intake workers must never perform a read-modify-write cycle over the
// global learned-rule set concurrently. The desktop process is single-instance,
// therefore this process-wide lock closes lost updates without serialising the
// rest of document generation.
static LEARNED_SCANNER_RULES_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_learned_scanner_rules() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    LEARNED_SCANNER_RULES_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "learned scanner rules lock failed".to_string())
}

fn load_learned_scanner_rules(app: &tauri::AppHandle) -> Result<Vec<LearnedScannerRule>, String> {
    default_state_repository(app)?
        .load_state_value(LEARNED_SCANNER_RULES_STATE_KEY)
        .map_err(|error| error.to_string())
        .map(|rules| rules.unwrap_or_default())
}

fn persist_learned_scanner_rules(
    app: &tauri::AppHandle,
    rules: &[LearnedScannerRule],
) -> Result<(), String> {
    default_state_repository(app)?
        .save_state_value(LEARNED_SCANNER_RULES_STATE_KEY, &rules)
        .map_err(|error| error.to_string())
}

fn infer_scanner_label(context: &str, selected_text: &str) -> String {
    let selected = selected_text.trim();
    let position = context.find(selected).unwrap_or(context.len());
    let left = &context[..position.min(context.len())];
    left.rsplit(['\n', '\r', '|', ';'])
        .next()
        .unwrap_or(left)
        .trim()
        .trim_end_matches(|character: char| {
            matches!(character, ':' | '-' | '–' | '—' | '=' | '№' | '#' | ' ')
        })
        .chars()
        .rev()
        .take(80)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .trim()
        .to_string()
}

fn source_layout_fingerprint(source_text: &str) -> String {
    let mut normalized = String::new();
    for (index, raw_line) in source_text.lines().take(800).enumerate() {
        let line = raw_line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            normalized.push_str("<blank>\n");
            continue;
        }
        let delimiter = line
            .char_indices()
            .find(|(_, character)| matches!(character, ':' | '=' | '\t'))
            .map(|(position, _)| position);
        if let Some(position) = delimiter.filter(|position| *position <= 120) {
            let label = line[..position].trim().to_lowercase();
            normalized.push_str(&normalize_layout_fragment(&label));
            normalized.push_str(":<value>\n");
        } else if index < 8 {
            normalized.push_str(&normalize_layout_fragment(&line.to_lowercase()));
            normalized.push('\n');
        } else {
            normalized.push_str(&layout_shape(&line));
            normalized.push('\n');
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hex::encode(hasher.finalize())
}

fn normalize_layout_fragment(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut digit_run = false;
    for character in value.chars().take(160) {
        if character.is_ascii_digit() {
            if !digit_run {
                output.push('#');
                digit_run = true;
            }
        } else {
            digit_run = false;
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '/' | '.' | '№') {
                output.push(character);
            }
        }
    }
    output
}

fn layout_shape(value: &str) -> String {
    let mut output = String::new();
    let mut previous = '\0';
    for character in value.chars().take(240) {
        let class = if character.is_alphabetic() {
            'A'
        } else if character.is_ascii_digit() {
            '#'
        } else if character.is_whitespace() {
            ' '
        } else {
            character
        };
        if class != previous || !matches!(class, 'A' | '#' | ' ') {
            output.push(class);
        }
        previous = class;
    }
    output
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack.to_lowercase().find(&needle.to_lowercase())
}

fn learned_rule_value(source_text: &str, rule: &LearnedScannerRule) -> Option<String> {
    let before = rule.before_text.trim();
    let after = rule.after_text.trim();
    if !before.is_empty() {
        if let Some(before_pos) = find_case_insensitive(source_text, before) {
            let start = before_pos + before.len();
            let tail = source_text.get(start..)?;
            let end = if !after.is_empty() {
                find_case_insensitive(tail, after)
                    .unwrap_or_else(|| tail.find(['\n', '\r', '|']).unwrap_or(tail.len()))
            } else {
                tail.find(['\n', '\r', '|']).unwrap_or(tail.len())
            };
            let value = clean_word_selection(&tail[..end]);
            if !value.is_empty() && value.len() <= 500 {
                return Some(value);
            }
        }
    }
    let label = rule.label_hint.trim();
    if label.is_empty() {
        return None;
    }
    let position = find_case_insensitive(source_text, label)?;
    let mut tail = source_text
        .get(position + label.len()..)?
        .trim_start_matches(|character: char| {
            matches!(
                character,
                ':' | '-' | '–' | '—' | '=' | '№' | '#' | ' ' | '\t'
            )
        });
    if let Some(end) = tail.find(['\n', '\r', '|']) {
        tail = &tail[..end];
    }
    let value = clean_word_selection(tail);
    (!value.is_empty() && value.len() <= 500).then_some(value)
}

fn apply_learned_scanner_rules(
    app: &tauri::AppHandle,
    source_text: &str,
    case: &mut SemanticCase,
) -> Result<Vec<(String, String)>, String> {
    let _rules_guard = lock_learned_scanner_rules()?;
    let mut rules = load_learned_scanner_rules(app)?;
    let fingerprint = source_layout_fingerprint(source_text);
    let mut applied = Vec::new();
    let mut rules_changed = false;
    for rule in &mut rules {
        let exact_layout = rule
            .layout_fingerprint
            .as_deref()
            .is_some_and(|expected| expected == fingerprint);
        if rule.layout_fingerprint.is_some() && !exact_layout {
            continue;
        }
        let Some(value) = learned_rule_value(source_text, rule) else {
            continue;
        };
        if validate_field_value(&rule.field_id, &value).is_err() {
            continue;
        }

        if rule.learning_status == "shadow" {
            if let Some(reference) = case.value(&rule.field_id) {
                rule.shadow_observations = rule.shadow_observations.saturating_add(1);
                if normalized_learning_value(&reference.value) == normalized_learning_value(&value) {
                    rule.shadow_agreements = rule.shadow_agreements.saturating_add(1);
                } else {
                    rule.shadow_conflicts = rule.shadow_conflicts.saturating_add(1);
                }
                rules_changed = true;
                if learning_rule_ready_for_promotion(rule) {
                    rule.learning_status = "promoted".into();
                    rule.promoted_at = Some(OffsetDateTime::now_utc().unix_timestamp().to_string());
                } else if learning_rule_should_be_rejected(rule) {
                    rule.learning_status = "rejected".into();
                }
            }
            if rule.learning_status != "promoted" {
                continue;
            }
        }
        if rule.learning_status != "promoted" {
            continue;
        }

        let report = apply_scanner_marks(
            case,
            &[ScannerMark {
                field_id: rule.field_id.clone(),
                selected_text: value.clone(),
                page_index: 0,
                confidence: if exact_layout { 0.999 } else { 0.88 },
            }],
        );
        if report
            .applied_fields
            .iter()
            .any(|field| field == &rule.field_id)
        {
            rule.successful_applications = rule.successful_applications.saturating_add(1);
            rule.last_applied_at = Some(OffsetDateTime::now_utc().unix_timestamp().to_string());
            rules_changed = true;
            applied.push((rule.field_id.clone(), value));
        }
    }
    if rules_changed {
        persist_learned_scanner_rules(app, &rules)?;
    }
    Ok(applied)
}

fn normalized_learning_value(value: &str) -> String {
    value
        .to_lowercase()
        .replace('ё', "е")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn learning_rule_ready_for_promotion(rule: &LearnedScannerRule) -> bool {
    rule.shadow_observations >= 8
        && rule.shadow_conflicts == 0
        && rule.shadow_agreements == rule.shadow_observations
}

fn learning_rule_should_be_rejected(rule: &LearnedScannerRule) -> bool {
    if rule.shadow_observations < 5 {
        return false;
    }
    let agreement_rate = rule.shadow_agreements as f32 / rule.shadow_observations as f32;
    rule.shadow_conflicts >= 3 || agreement_rate < 0.80
}


const TEMPLATE_APPROVALS_STATE_KEY: &str = "template_revision_approvals_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateApprovalRecord {
    document_id: String,
    template_sha256: String,
    jurisdiction: String,
    approved_by: String,
    approved_at: String,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
struct ApproveDocumentTemplateRequest {
    document_id: String,
    jurisdiction: String,
    approved_by: String,
    #[serde(default)]
    note: String,
    acknowledgement: bool,
}

#[derive(Debug, Deserialize)]
struct PrintTriageRequest {
    document_ids: Vec<String>,
    #[serde(default)]
    output_folder: Option<String>,
}

fn load_template_approvals(app: &tauri::AppHandle) -> Result<Vec<TemplateApprovalRecord>, String> {
    repository_for(&default_state_db_path(app)?)?
        .load_state_value::<Vec<TemplateApprovalRecord>>(TEMPLATE_APPROVALS_STATE_KEY)
        .map_err(|error| error.to_string())
        .map(|records| records.unwrap_or_default())
}

fn save_template_approvals(
    app: &tauri::AppHandle,
    records: &[TemplateApprovalRecord],
) -> Result<(), String> {
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(TEMPLATE_APPROVALS_STATE_KEY, records)
        .map_err(|error| error.to_string())
}

fn template_revision_sha256(
    app: &tauri::AppHandle,
    document: &DocumentTemplateSpec,
) -> Result<String, String> {
    let path = resolve_user_path(app, &document.template_path)?;
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("Не удалось прочитать шаблон «{}»: {error}", document.button_label))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn currently_approved_document_ids(
    app: &tauri::AppHandle,
    documents: &[DocumentTemplateSpec],
) -> Result<BTreeSet<String>, String> {
    let records = load_template_approvals(app)?;
    let by_id = records
        .into_iter()
        .map(|record| (record.document_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut approved = BTreeSet::new();
    for document in documents {
        let Some(record) = by_id.get(&document.id) else {
            continue;
        };
        if record.template_sha256 == template_revision_sha256(app, document)? {
            approved.insert(document.id.clone());
        }
    }
    Ok(approved)
}

#[tauri::command]
fn list_template_approvals(
    app: tauri::AppHandle,
) -> Result<Vec<TemplateApprovalRecord>, String> {
    load_template_approvals(&app)
}

#[tauri::command]
fn approve_document_template(
    req: ApproveDocumentTemplateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<TemplateApprovalRecord, String> {
    if !req.acknowledgement {
        return Err("Утверждение требует явного подтверждения ответственности организации за форму.".into());
    }
    let approved_by = req.approved_by.trim();
    let jurisdiction = req.jurisdiction.trim();
    if approved_by.len() < 2 || approved_by.len() > 160 {
        return Err("Укажите ФИО или роль утверждающего (2–160 символов).".into());
    }
    if jurisdiction.len() < 2 || jurisdiction.len() > 120 {
        return Err("Укажите применимую юрисдикцию (2–120 символов).".into());
    }
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let document = pack
        .documents
        .iter()
        .find(|document| document.id == req.document_id)
        .ok_or_else(|| "Документ не найден в текущем наборе.".to_string())?;
    let record = TemplateApprovalRecord {
        document_id: document.id.clone(),
        template_sha256: template_revision_sha256(&app, document)?,
        jurisdiction: jurisdiction.to_string(),
        approved_by: approved_by.to_string(),
        approved_at: OffsetDateTime::now_utc().to_string(),
        note: req.note.trim().chars().take(500).collect(),
    };
    let mut records = load_template_approvals(&app)?;
    records.retain(|existing| existing.document_id != record.document_id);
    records.push(record.clone());
    records.sort_by(|left, right| left.document_id.cmp(&right.document_id));
    save_template_approvals(&app, &records)?;
    append_audit_event(
        &app,
        "template_revision_approved",
        &record.template_sha256,
        &serde_json::json!({
            "document_id": &record.document_id,
            "jurisdiction": &record.jurisdiction,
            "approved_by": &record.approved_by,
            "approved_at": &record.approved_at,
        }),
    )?;
    Ok(record)
}

#[tauri::command]
fn revoke_document_template_approval(
    document_id: String,
    app: tauri::AppHandle,
) -> Result<Vec<TemplateApprovalRecord>, String> {
    let mut records = load_template_approvals(&app)?;
    let before = records.len();
    records.retain(|record| record.document_id != document_id);
    if records.len() != before {
        save_template_approvals(&app, &records)?;
        append_audit_event(
            &app,
            "template_revision_approval_revoked",
            "",
            &serde_json::json!({ "document_id": document_id }),
        )?;
    }
    Ok(records)
}

fn build_print_triage(
    app: &tauri::AppHandle,
    case: &SemanticCase,
    pack: &DocumentPack,
    document_ids: &[String],
) -> Result<PrintTriageReport, String> {
    if document_ids.is_empty() {
        return Err("Не передан ни один документ для проверки автопечати.".into());
    }
    let requested = document_ids.iter().cloned().collect::<BTreeSet<_>>();
    let documents = pack
        .documents
        .iter()
        .filter(|document| requested.contains(&document.id))
        .collect::<Vec<_>>();
    if documents.len() != requested.len() {
        let found = documents
            .iter()
            .map(|document| document.id.clone())
            .collect::<BTreeSet<_>>();
        let unknown = requested.difference(&found).cloned().collect::<Vec<_>>();
        return Err(format!("Неизвестные документы: {}.", unknown.join(", ")));
    }
    let owned = documents.iter().map(|document| (*document).clone()).collect::<Vec<_>>();
    let approved = currently_approved_document_ids(app, &owned)?;
    let selection = threshold_calibration::thresholds_for_case(app, case);
    let mut report = evaluate_print_triage_with_thresholds(
        case,
        documents,
        &approved,
        &selection.thresholds,
    );
    if let Some(warning) = selection.warning {
        report.reasons.insert(0, warning);
        report.auto_print_allowed = false;
        if report.decision == "auto_print" {
            report.decision = "review_fields".into();
        }
    }
    Ok(report)
}

fn persist_print_review_record(
    app: &tauri::AppHandle,
    output_folder: Option<&str>,
    report: &PrintTriageReport,
) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("print-review-queue");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }

    let timestamp = OffsetDateTime::now_utc().unix_timestamp_nanos();
    let document_key = report.checked_document_ids.join("|");
    let digest = Sha256::digest(format!("{timestamp}|{document_key}").as_bytes());
    let suffix = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let review_id = format!("review-{timestamp}-{suffix}");
    let state_key = format!("print_review_record_v2:{review_id}");
    let created_at = OffsetDateTime::now_utc().to_string();

    // Values, evidence excerpts and output paths may contain professional or
    // personal data. Keep the complete review record only inside the encrypted,
    // authenticated SQLite state repository. The filesystem marker is an
    // intentionally non-sensitive queue index for operators and support tools.
    let encrypted_payload = serde_json::json!({
        "schema": 2,
        "review_id": review_id.clone(),
        "created_at": created_at.clone(),
        "output_folder": output_folder,
        "status": "pending_review",
        "report": report,
    });
    default_state_repository(app)?
        .save_state_value(&state_key, &encrypted_payload)
        .map_err(|error| error.to_string())?;

    let path = root.join(format!("{review_id}.json"));
    let marker = serde_json::json!({
        "schema": 2,
        "review_id": review_id,
        "created_at": created_at,
        "status": "pending_review",
        "checked_document_ids": &report.checked_document_ids,
        "confidence_score": report.confidence_score,
        "encrypted_payload": true,
        "state_key": state_key,
    });
    let bytes = serde_json::to_vec_pretty(&marker).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(path)
}

#[tauri::command]
fn get_print_triage(
    req: PrintTriageRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PrintTriageReport, String> {
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let report = build_print_triage(&app, &case, &pack, &req.document_ids)?;
    let review_record = if report.auto_print_allowed {
        None
    } else {
        Some(persist_print_review_record(
            &app,
            req.output_folder.as_deref(),
            &report,
        )?)
    };
    let mut details = serde_json::to_value(&report).map_err(|error| error.to_string())?;
    if let Some(path) = review_record.as_ref() {
        details["review_record"] = serde_json::Value::String(path.display().to_string());
    }
    append_audit_event(
        &app,
        if report.auto_print_allowed {
            "print_triage_passed"
        } else {
            "print_triage_review_required"
        },
        "",
        &details,
    )?;
    Ok(report)
}

const MAX_PRINT_COPIES: u16 = 99;
const PRINT_PREFERENCES_STATE_KEY: &str = "print_preferences_v1";

fn default_print_copies() -> u16 {
    1
}

#[derive(Debug, Clone, Deserialize)]
struct PrintJobRequest {
    path: String,
    #[serde(default = "default_print_copies")]
    copies: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct PrintPreferences {
    printer_name: Option<String>,
    duplex_mode: String,
    tray: Option<i32>,
}

impl Default for PrintPreferences {
    fn default() -> Self {
        Self {
            printer_name: None,
            duplex_mode: "simplex".into(),
            tray: None,
        }
    }
}

impl PrintPreferences {
    fn validate(&self) -> Result<(), String> {
        if let Some(name) = &self.printer_name {
            let name = name.trim();
            if name.len() > 256 || name.chars().any(char::is_control) {
                return Err("Имя принтера содержит недопустимые символы.".into());
            }
        }
        if !matches!(
            self.duplex_mode.as_str(),
            "simplex" | "long_edge" | "short_edge" | "manual"
        ) {
            return Err("Неизвестный режим двусторонней печати.".into());
        }
        if self.tray.is_some_and(|tray| {
            !matches!(tray, 0..=11 | 14 | 15)
        }) {
            return Err("Неизвестный код лотка Word.".into());
        }
        Ok(())
    }
}

fn load_print_preferences(app: &tauri::AppHandle) -> Result<PrintPreferences, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let preferences = repo
        .load_state_value::<PrintPreferences>(PRINT_PREFERENCES_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    preferences.validate()?;
    Ok(preferences)
}

fn persist_print_preferences(
    app: &tauri::AppHandle,
    preferences: &PrintPreferences,
) -> Result<(), String> {
    preferences.validate()?;
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(PRINT_PREFERENCES_STATE_KEY, preferences)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrinterInfo {
    name: String,
    is_default: bool,
    driver: String,
    port: String,
}

#[derive(Debug, Serialize)]
struct PrinterInventory {
    platform: String,
    printers: Vec<PrinterInfo>,
    preferences: PrintPreferences,
    advanced_options_note: String,
}

#[cfg(target_os = "windows")]
fn discover_printers() -> Result<Vec<PrinterInfo>, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$default = (Get-CimInstance Win32_Printer | Where-Object { $_.Default } | Select-Object -First 1 -ExpandProperty Name)
@((Get-Printer | Sort-Object Name | ForEach-Object {
  [pscustomobject]@{
    name = [string]$_.Name
    is_default = ([string]$_.Name -eq [string]$default)
    driver = [string]$_.DriverName
    port = [string]$_.PortName
  }
})) | ConvertTo-Json -Compress
"#;
    let output = run_hidden_powershell(script)?;
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&output)
        .map_err(|error| format!("Не удалось прочитать список принтеров Windows: {error}"))
}

#[cfg(not(target_os = "windows"))]
fn discover_printers() -> Result<Vec<PrinterInfo>, String> {
    let default_output = std::process::Command::new("lpstat")
        .arg("-d")
        .output()
        .ok();
    let default_name = default_output
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .and_then(|text| text.split_once(':').map(|(_, name)| name.trim().to_string()))
        .unwrap_or_default();
    let output = std::process::Command::new("lpstat")
        .arg("-p")
        .output()
        .map_err(|error| format!("Не удалось получить список принтеров CUPS: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("printer "))
        .filter_map(|tail| tail.split_whitespace().next())
        .map(|name| PrinterInfo {
            name: name.to_string(),
            is_default: name == default_name,
            driver: "CUPS".into(),
            port: String::new(),
        })
        .collect())
}

#[tauri::command]
fn get_printer_inventory(app: tauri::AppHandle) -> Result<PrinterInventory, String> {
    let printers = discover_printers().unwrap_or_default();
    Ok(PrinterInventory {
        platform: std::env::consts::OS.into(),
        printers,
        preferences: load_print_preferences(&app)?,
        advanced_options_note: if cfg!(target_os = "windows") {
            "Для Word используются COM и PrintTicket. Для PDF принтер, copies, duplex и лоток передаются проверяемому SumatraPDF sidecar; без него PDF auto-print блокируется fail-closed, а не уходит в неизвестный системный обработчик.".into()
        } else {
            "Для CUPS передаются printer, sides и media-source через lp.".into()
        },
    })
}

#[derive(Debug, Deserialize)]
struct UpdatePrintPreferencesRequest {
    preferences: PrintPreferences,
}

#[tauri::command]
fn update_print_preferences(
    req: UpdatePrintPreferencesRequest,
    app: tauri::AppHandle,
) -> Result<PrinterInventory, String> {
    let mut preferences = req.preferences;
    preferences.printer_name = preferences
        .printer_name
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    persist_print_preferences(&app, &preferences)?;
    append_audit_event(
        &app,
        "print_preferences_updated",
        "",
        &serde_json::json!({
            "printer_configured": preferences.printer_name.is_some(),
            "duplex_mode": &preferences.duplex_mode,
            "tray": preferences.tray,
        }),
    )?;
    Ok(PrinterInventory {
        platform: std::env::consts::OS.into(),
        printers: discover_printers().unwrap_or_default(),
        preferences,
        advanced_options_note: if cfg!(target_os = "windows") {
            "Для Word используются COM и PrintTicket. Для PDF принтер, copies, duplex и лоток передаются проверяемому SumatraPDF sidecar; без него PDF auto-print блокируется fail-closed, а не уходит в неизвестный системный обработчик.".into()
        } else {
            "Для CUPS передаются printer, sides и media-source через lp.".into()
        },
    })
}

#[derive(Debug, Clone, Serialize)]
struct PrintFailure {
    path: String,
    requested_copies: u16,
    queued_copies: u16,
    error: String,
}

#[derive(Debug, Clone, Serialize)]
struct PrintFilesResponse {
    queued_files: Vec<String>,
    queued_copies: u32,
    failed_files: Vec<PrintFailure>,
}

fn print_resolved_jobs(
    jobs: &[(PathBuf, u16)],
    preferences: &PrintPreferences,
) -> PrintFilesResponse {
    let mut queued_files = Vec::new();
    let mut queued_copies = 0u32;
    let mut failed_files = Vec::new();
    for (path, requested_copies) in jobs {
        if *requested_copies == 0 {
            continue;
        }
        if *requested_copies > MAX_PRINT_COPIES {
            failed_files.push(PrintFailure {
                path: path.display().to_string(),
                requested_copies: *requested_copies,
                queued_copies: 0,
                error: format!("Количество копий не может превышать {MAX_PRINT_COPIES}."),
            });
            continue;
        }
        match print_path_copies(path, *requested_copies, preferences) {
            Ok(()) => {
                queued_files.push(path.display().to_string());
                queued_copies += u32::from(*requested_copies);
            }
            Err(error) => {
                failed_files.push(PrintFailure {
                    path: path.display().to_string(),
                    requested_copies: *requested_copies,
                    queued_copies: 0,
                    error,
                });
            }
        }
    }
    PrintFilesResponse {
        queued_files,
        queued_copies,
        failed_files,
    }
}

fn value_source_label(source: ValueSource) -> &'static str {
    match source {
        ValueSource::SafeDefault => "распознано автоматически",
        ValueSource::Model => "предложено локальной SemanticModel",
        ValueSource::Scanner => "разметка специалиста",
        ValueSource::SessionSelection => "выбрано в текущей сессии",
        ValueSource::UserConfirmed => "подтверждено специалистом",
    }
}

struct TrustReportContext<'a> {
    source_name: &'a str,
    source_sha256: &'a str,
    generated_names: &'a [String],
    used_field_ids: &'a BTreeSet<String>,
    include_values: bool,
    source_warnings: &'a [String],
}

fn write_trust_report(
    folder: &Path,
    semantic_case: &SemanticCase,
    context: TrustReportContext<'_>,
) -> Result<PathBuf, String> {
    let TrustReportContext {
        source_name,
        source_sha256,
        generated_names,
        used_field_ids,
        include_values,
        source_warnings,
    } = context;
    let report_path = folder.join("ПРОВЕРИТЬ_КОМПЛЕКТ.txt");
    let mut report = String::new();
    report.push_str("ДОККОМПЛЕКТ — ОТЧЁТ ПРОВЕРЯЕМОСТИ\n");
    report.push_str("======================================\n\n");
    if include_values {
        report.push_str(&format!("Источник: {source_name}\n"));
    } else {
        report.push_str(&format!("Источник SHA-256: {source_sha256}\n"));
    }
    report.push_str(&format!("Создано документов: {}\n", generated_names.len()));
    for name in generated_names {
        report.push_str(&format!("- {name}\n"));
    }
    if !source_warnings.is_empty() {
        report.push_str("\nПредупреждения нормализации:\n");
        for warning in source_warnings {
            report.push_str(&format!("- {}\n", warning.replace(['\r', '\n', '\t'], " ")));
        }
    }
    report.push_str("\nПоля, реально использованные выбранными шаблонами:\n");
    let mut written = 0usize;
    for field_id in used_field_ids {
        let Some(value) = semantic_case.values.get(field_id) else {
            continue;
        };
        written += 1;
        let confidence = (value.confidence.clamp(0.0, 1.0) * 100.0).round() as u32;
        if include_values {
            let safe_value = value.value.replace(['\r', '\n', '\t'], " ");
            report.push_str(&format!(
                "- {field_id}: {safe_value} | {} | {confidence}%\n",
                value_source_label(value.source)
            ));
        } else {
            report.push_str(&format!(
                "- {field_id}: [значение скрыто политикой конфиденциальности] | {} | {confidence}%\n",
                value_source_label(value.source)
            ));
        }
    }
    if written == 0 {
        report.push_str("- динамические поля отсутствуют\n");
    }
    report.push_str("\nВсе перечисленные значения прошли типовые, межполевые и риск-зависимые проверки до публикации комплекта.\n");
    report.push_str("Отчёт создаётся локально и не отправляется наружу.\n");
    std::fs::write(&report_path, report)
        .map_err(|error| format!("Не удалось записать отчёт проверяемости: {error}"))?;
    Ok(report_path)
}
