use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use dokkomplekt_core::{SemanticAtom, SemanticCase, SemanticRecord};
use dokkomplekt_docx::extract_docx_text;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Cursor, Read as _};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;
use zip::ZipArchive;

mod archive;
mod msg;
mod source_snapshot;
mod web;

use archive::{normalize_external_archive, normalize_zip};
#[cfg(test)]
use archive::{parse_7z_technical_listing, validate_archive_relative_path};
pub(crate) use source_snapshot::StableSourceSnapshot;
pub use source_snapshot::{capture_stable_source, current_source_matches};
pub use web::fetch_web_source;
#[cfg(test)]
use web::is_public_ip;

const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_SOURCE_FILE_BYTES: u64 = MAX_UPLOAD_BYTES as u64;
const MAX_NORMALIZED_TEXT_BYTES: usize = 32 * 1024 * 1024;
const MAX_CONTAINER_XML_BYTES: usize = 32 * 1024 * 1024;
const MAX_XLSX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_XLSX_ENTRIES: usize = 4_096;
const MAX_XLSX_SHEETS: usize = 256;
const MAX_XLSX_SHARED_STRINGS: usize = 250_000;
const MAX_XLSX_CELLS: usize = 1_000_000;
const MAX_XLSX_ROWS: usize = 250_000;
const MAX_XLSX_COLUMNS: usize = 16_384;
const MAX_XLSX_CELL_BYTES: usize = 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 256;
const MAX_ARCHIVE_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_DEPTH: usize = 3;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_LAYOUT_ITEMS: usize = 20_000;
const ACTIVE_SESSION_MARKER: &str = ".active";
const ACTIVE_SESSION_GRACE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct IntakeCapability {
    pub format: String,
    pub extensions: Vec<String>,
    pub ready: bool,
    pub mode: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SidecarToolStatus {
    pub tool: String,
    pub available: bool,
    pub bundled: bool,
    pub state: String,
    pub component_id: Option<String>,
    pub resolved_path: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutBoundingBox {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedLayoutItem {
    pub item_kind: String,
    pub page_index: Option<usize>,
    pub block_index: Option<usize>,
    pub text: String,
    pub cells: Vec<String>,
    pub bbox: Option<LayoutBoundingBox>,
    pub confidence: f32,
    pub source_reference: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NormalizedSource {
    pub text: String,
    pub source_kind: String,
    pub warnings: Vec<String>,
    pub processed_files: Vec<PathBuf>,
    pub layout_items: Vec<NormalizedLayoutItem>,
}

#[derive(Debug)]
pub struct UploadedSourceSession {
    source: Option<NormalizedSource>,
    root: PathBuf,
}

impl UploadedSourceSession {
    pub fn original_path(&self) -> Result<PathBuf, String> {
        self.source
            .as_ref()
            .and_then(|source| source.processed_files.first().cloned())
            .or_else(|| {
                std::fs::read_dir(&self.root)
                    .ok()?
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name()
                            .and_then(|value| value.to_str())
                            .is_some_and(|name| name != ACTIVE_SESSION_MARKER)
                    })
            })
            .ok_or_else(|| "Временная сессия не содержит исходный файл.".to_string())
    }

    pub fn take_source(&mut self) -> Result<NormalizedSource, String> {
        self.source
            .take()
            .ok_or_else(|| "Временный источник уже был извлечён из сессии.".to_string())
    }

    #[cfg(test)]
    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for UploadedSourceSession {
    fn drop(&mut self) {
        let _ = remove_sensitive_session(&self.root);
    }
}

#[derive(Debug)]
pub struct RetainedUploadedSource {
    file_name: String,
    bytes: Vec<u8>,
}

impl RetainedUploadedSource {
    pub fn new(file_name: &str, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err("Источник превышает безопасный предел 100 МБ.".to_string());
        }
        Ok(Self {
            file_name: safe_file_name(file_name),
            bytes: bytes.to_vec(),
        })
    }

    pub fn virtual_path(&self) -> String {
        format!("dokkomplekt-upload://current/{}", self.file_name)
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn materialize(&self, workspace: &Path) -> Result<UploadedSourceSession, String> {
        materialize_sensitive_file(&self.file_name, &self.bytes, workspace)
    }
}

impl Drop for RetainedUploadedSource {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WebIntakeResult {
    pub source_text: String,
    pub final_url: String,
    pub content_type: String,
    pub warnings: Vec<String>,
    #[serde(skip_serializing)]
    pub source_sha256: String,
}

pub fn supported_extensions() -> &'static [&'static str] {
    &[
        "docx", "docm", "doc", "ppt", "pptx", "pdf", "jpg", "jpeg", "png", "tif", "tiff", "bmp",
        "webp", "xlsx", "xls", "ods", "odt", "rtf", "txt", "md", "csv", "tsv", "json", "xml",
        "html", "htm", "eml", "msg", "zip", "7z", "rar",
    ]
}

pub fn is_supported_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            supported_extensions()
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

pub fn is_temporary_source(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    name.starts_with("~$")
        || name.ends_with(".part")
        || name.ends_with(".crdownload")
        || name.ends_with(".tmp")
        || name.contains(".dokkomplekt-processing")
        || name.contains(".dokkomplekt-processed")
        || name.contains(".dokkomplekt-finalizing-")
}

pub fn validate_source_file_size(path: &Path) -> Result<u64, String> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        format!(
            "Не удалось проверить размер источника {}: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!("Источник не является файлом: {}", path.display()));
    }
    if metadata.len() > MAX_SOURCE_FILE_BYTES {
        return Err(format!(
            "Источник превышает безопасный предел {} МБ: {}",
            MAX_SOURCE_FILE_BYTES / (1024 * 1024),
            path.display()
        ));
    }
    Ok(metadata.len())
}

fn read_file_limited(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Не удалось прочитать {label}: {error}"))?;
    if bytes.len() > limit {
        return Err(format!(
            "{label} превышает безопасный предел {} МБ.",
            limit / (1024 * 1024)
        ));
    }
    Ok(bytes)
}

fn read_text_limited(
    reader: &mut impl std::io::Read,
    limit: usize,
    label: &str,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Не удалось прочитать {label}: {error}"))?;
    if bytes.len() > limit {
        return Err(format!(
            "{label} превышает безопасный распакованный предел {} МБ.",
            limit / (1024 * 1024)
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("{label} не является корректным UTF-8 XML: {error}"))
}

fn validate_xlsx_archive<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<(), String> {
    if archive.len() > MAX_XLSX_ENTRIES {
        return Err(format!(
            "XLSX содержит слишком много частей: {} > {MAX_XLSX_ENTRIES}",
            archive.len()
        ));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| "Размер XLSX переполнен.".to_string())?;
        if total > MAX_XLSX_UNPACKED_BYTES {
            return Err(format!(
                "Распакованное содержимое XLSX превышает безопасный предел {} МБ.",
                MAX_XLSX_UNPACKED_BYTES / (1024 * 1024)
            ));
        }
        let name = entry.name().to_ascii_lowercase();
        if name.ends_with(".xml") && entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
            return Err(format!("XML-часть XLSX слишком велика: {}", entry.name()));
        }
    }
    Ok(())
}

pub fn decode_uploaded_payload(file_name: &str, encoded: &str) -> Result<Vec<u8>, String> {
    if !is_supported_path(Path::new(file_name)) {
        return Err(format!(
            "Формат файла не поддерживается: {file_name}. Поддерживаются: {}.",
            supported_extensions().join(", ")
        ));
    }
    let trimmed = encoded.trim();
    if trimmed.len() > MAX_UPLOAD_BYTES.saturating_mul(2) {
        return Err("Файл слишком большой: максимум 100 МБ для ручной загрузки.".into());
    }
    let bytes = BASE64_STANDARD
        .decode(trimmed)
        .map_err(|_| "Файл повреждён: не удалось декодировать содержимое.".to_string())?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("Файл слишком большой: максимум 100 МБ для ручной загрузки.".into());
    }
    Ok(bytes)
}

pub fn capabilities() -> Vec<IntakeCapability> {
    let pdftotext = command_available("pdftotext");
    let pdftoppm = command_available("pdftoppm");
    let tesseract = command_available("tesseract");
    let soffice = command_available("soffice");
    let seven_zip = command_available("7z");
    vec![
        capability("Word DOCX/DOCM", &["docx", "docm"], true, "встроенно", "Текст, таблицы, колонтитулы и сноски; удалённые правки, комментарии и инструкции полей исключаются."),
        capability("Старый Word DOC", &["doc"], soffice, "LibreOffice", if soffice { "Готово через безоконную конвертацию в DOCX." } else { "Нужен LibreOffice/soffice или упакованный sidecar; без него DOC отклоняется fail-closed." }),
        capability("PowerPoint PPT/PPTX", &["ppt", "pptx"], soffice && pdftotext, "LibreOffice + Poppler", if soffice && pdftotext { "Готово через безоконную конвертацию в PDF с извлечением текста слайдов." } else { "Нужны LibreOffice/soffice и Poppler/pdftotext; без них PPT/PPTX отклоняются fail-closed." }),
        capability("PDF с текстовым слоем", &["pdf"], pdftotext, "Poppler/pdftotext", if pdftotext { "Готово." } else { "Установите Poppler или положите pdftotext в папку tools приложения." }),
        capability("Сканированный PDF", &["pdf"], pdftotext && pdftoppm && tesseract, "Poppler + OCR", if pdftotext && pdftoppm && tesseract { "Готово: PDF без текста автоматически распознаётся OCR." } else { "Нужны pdftotext, pdftoppm и Tesseract с языками rus+eng." }),
        capability("Сканы и фотографии", &["jpg", "jpeg", "png", "tif", "tiff", "bmp", "webp"], tesseract, "Tesseract OCR", if tesseract { "Готово." } else { "Установите Tesseract OCR или положите его в папку tools приложения." }),
        capability("XLSX / CSV / TSV", &["xlsx", "csv", "tsv"], true, "встроенно", "XLSX читается напрямую без запуска Excel."),
        capability("XLS / ODS", &["xls", "ods"], soffice, "LibreOffice", if soffice { "Готово через безоконную конвертацию." } else { "Нужен LibreOffice/soffice или упакованный sidecar." }),
        capability("ODT / RTF", &["odt", "rtf"], true, "встроенно", "Нормализация без запуска офисного приложения."),
        capability("EML", &["eml"], true, "встроенно", "Заголовки, текст/HTML и поддерживаемые вложения."),
        capability("MSG", &["msg"], true, "встроенно", "Outlook MSG читается нативно в Rust без внешнего конвертера; поддерживаемые вложения проходят тот же безопасный recursive intake."),
        capability("ZIP", &["zip"], true, "встроенно", "Рекурсивная распаковка с защитой от zip-slip, архивных бомб и чрезмерной вложенности."),
        capability("7Z / RAR", &["7z", "rar"], seven_zip, "7-Zip", if seven_zip { "Готово." } else { "Нужен 7z sidecar или установленный 7-Zip." }),
        capability("Сайты и API", &["https"], true, "HTTPS", "Публичные HTTPS-адреса, ограничение размера, проверка перенаправлений и нормализация HTML/JSON/XML/файлов."),
    ]
}

pub fn sidecar_tool_statuses() -> Vec<SidecarToolStatus> {
    [
        ("tesseract", "OCR изображений и сканированных PDF"),
        ("pdftotext", "извлечение текстового слоя PDF"),
        ("pdftoppm", "преобразование страниц PDF для OCR"),
        ("soffice", "XLS/ODS, PDF-экспорт и печать Office-документов"),
        ("7z", "распаковка 7Z/RAR"),
        ("sumatrapdf", "управляемая печать PDF на Windows"),
    ]
    .into_iter()
    .map(|(tool, purpose)| {
        let resolved = resolve_tool(tool);
        let component_root = crate::component_manager::user_components_dir();
        let downloaded = component_root
            .as_ref()
            .is_some_and(|root| resolved.starts_with(root));
        let bundled = !downloaded && resolved.is_file() && resolved != Path::new(tool);
        let probe_argument = if tool == "sumatrapdf" {
            "-list-printers"
        } else {
            "--version"
        };
        let available = Command::new(&resolved)
            .arg(probe_argument)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        SidecarToolStatus {
            tool: tool.into(),
            available,
            bundled,
            state: if downloaded {
                "downloaded"
            } else if bundled {
                "bundled"
            } else if available {
                "system"
            } else {
                "missing"
            }
            .into(),
            component_id: component_id_for_tool(tool).map(str::to_string),
            resolved_path: resolved.display().to_string(),
            purpose: purpose.into(),
        }
    })
    .collect()
}

fn component_id_for_tool(tool: &str) -> Option<&'static str> {
    match tool {
        "tesseract" | "pdftotext" | "pdftoppm" => Some("ocr"),
        "soffice" | "sumatrapdf" => Some("office"),
        "llama_cpp" | "semantic_model" => Some("semantic"),
        _ => None,
    }
}

fn capability(
    format: &str,
    extensions: &[&str],
    ready: bool,
    mode: &str,
    detail: &str,
) -> IntakeCapability {
    IntakeCapability {
        format: format.into(),
        extensions: extensions
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        ready,
        mode: mode.into(),
        detail: detail.into(),
    }
}

pub fn cleanup_workspace(workspace: &Path, max_age: Duration) -> Result<usize, String> {
    if !workspace.exists() {
        return Ok(0);
    }
    let now = std::time::SystemTime::now();
    let mut removed = 0usize;
    for entry in std::fs::read_dir(workspace).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_dir() && active_session_is_recent(&path, now) {
            continue;
        }
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if !old_enough {
            continue;
        }
        let result = if metadata.file_type().is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if result.is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn active_session_is_recent(path: &Path, now: std::time::SystemTime) -> bool {
    let marker = path.join(ACTIVE_SESSION_MARKER);
    std::fs::symlink_metadata(marker)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age < ACTIVE_SESSION_GRACE)
}

pub fn create_retained_workspace_session(workspace: &Path) -> Result<PathBuf, String> {
    create_sensitive_session(workspace)
}

pub fn refresh_retained_workspace_session(workspace: &Path, path: &Path) -> Result<bool, String> {
    let Ok(relative) = path.strip_prefix(workspace) else {
        return Ok(false);
    };
    let Some(Component::Normal(session_name)) = relative.components().next() else {
        return Ok(false);
    };
    let session_name = session_name.to_string_lossy();
    if !session_name.starts_with("session-") {
        return Ok(false);
    }
    let session_root = workspace.join(session_name.as_ref());
    let metadata = std::fs::symlink_metadata(&session_root)
        .map_err(|error| format!("Учебная сессия недоступна: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("Учебная сессия имеет небезопасный тип файла.".into());
    }
    let marker = session_root.join(ACTIVE_SESSION_MARKER);
    std::fs::write(&marker, b"active")
        .map_err(|error| format!("Не удалось продлить учебную сессию: {error}"))?;
    restrict_file_permissions(&marker)?;
    Ok(true)
}

fn create_sensitive_session(workspace: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(workspace).map_err(|error| error.to_string())?;
    let root = workspace.join(format!("session-{}", Uuid::new_v4()));
    std::fs::create_dir(&root)
        .map_err(|error| format!("Не удалось создать защищённую временную сессию: {error}"))?;
    restrict_directory_permissions(&root)?;
    let marker = root.join(ACTIVE_SESSION_MARKER);
    std::fs::write(&marker, b"active")
        .map_err(|error| format!("Не удалось создать маркер временной сессии: {error}"))?;
    restrict_file_permissions(&marker)?;
    Ok(root)
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Не удалось ограничить доступ к временной папке: {error}"))
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Не удалось ограничить доступ к временному файлу: {error}"))
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn remove_sensitive_session(root: &Path) -> Result<(), String> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        std::fs::remove_file(root).map_err(|error| error.to_string())
    } else {
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }
}

pub fn materialize_sensitive_file(
    file_name: &str,
    bytes: &[u8],
    workspace: &Path,
) -> Result<UploadedSourceSession, String> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("Источник превышает безопасный предел 100 МБ.".to_string());
    }
    let root = create_sensitive_session(workspace)?;
    let session = UploadedSourceSession { source: None, root };
    let safe_name = safe_file_name(file_name);
    let path = session.root.join(safe_name);
    std::fs::write(&path, bytes)
        .map_err(|error| format!("Не удалось сохранить источник во временную папку: {error}"))?;
    restrict_file_permissions(&path)?;
    Ok(session)
}

pub fn normalize_uploaded_bytes(
    file_name: &str,
    bytes: &[u8],
    workspace: &Path,
) -> Result<UploadedSourceSession, String> {
    let mut session = materialize_sensitive_file(file_name, bytes, workspace)?;
    let path = session.original_path()?;
    let mut normalized = normalize_path(&path, &session.root, 0)?;
    normalized.processed_files.insert(0, path);
    session.source = Some(normalized);
    Ok(session)
}

pub fn normalize_path(
    path: &Path,
    workspace: &Path,
    depth: usize,
) -> Result<NormalizedSource, String> {
    if depth > MAX_ARCHIVE_DEPTH {
        return Err("Превышена допустимая глубина вложенных архивов (3).".into());
    }
    if is_temporary_source(path) {
        return Err("Временный или служебный файл не обрабатывается.".into());
    }
    validate_source_file_size(path)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut result = match extension.as_str() {
        "docx" | "docm" => NormalizedSource {
            text: extract_docx_text(path)
                .map_err(|error| format!("Word-документ не читается: {error}"))?,
            source_kind: "word".into(),
            warnings: Vec::new(),
            processed_files: vec![path.to_path_buf()],
            layout_items: Vec::new(),
        },
        "doc" => {
            let mut converted = normalize_office_via_libreoffice(path, workspace, "docx")?;
            converted.source_kind = "legacy_word_converted".into();
            converted.warnings.push(
                "Старый DOC преобразован локальным LibreOffice в DOCX перед извлечением.".into(),
            );
            converted
        }
        "ppt" | "pptx" => {
            let mut converted = normalize_office_via_libreoffice(path, workspace, "pdf")?;
            converted.source_kind = "presentation_converted".into();
            converted.warnings.push(
                "Презентация преобразована локальным LibreOffice в PDF; извлечён текст слайдов."
                    .into(),
            );
            converted
        }
        "pdf" => normalize_pdf(path, workspace)?,
        "jpg" | "jpeg" | "png" | "tif" | "tiff" | "bmp" | "webp" => normalize_image(path)?,
        "xlsx" => normalize_xlsx(path)?,
        "xls" | "ods" => normalize_office_via_libreoffice(path, workspace, "xlsx")?,
        "odt" => normalize_odt(path)?,
        "rtf" => normalize_rtf(path)?,
        "txt" | "md" | "csv" | "tsv" | "json" | "xml" => normalize_plain_text(path, &extension)?,
        "html" | "htm" => normalize_html(path)?,
        "eml" => normalize_eml(path, workspace, depth)?,
        "msg" => msg::normalize_msg(path, workspace, depth)?,
        "zip" => normalize_zip(path, workspace, depth)?,
        "7z" | "rar" => normalize_external_archive(path, workspace, depth)?,
        _ => return Err(format!("Неподдерживаемый формат: .{extension}")),
    };
    if result.text.len() > MAX_NORMALIZED_TEXT_BYTES {
        return Err(
            "После распознавания получено больше 32 МБ текста; источник нужно разделить.".into(),
        );
    }
    result.text = normalize_text(&result.text);
    normalize_layout_items(&mut result.layout_items);
    if result.layout_items.len() > MAX_LAYOUT_ITEMS {
        result.layout_items.truncate(MAX_LAYOUT_ITEMS);
        result.warnings.push(format!(
            "Структурный слой ограничен {MAX_LAYOUT_ITEMS} строками для защиты памяти; текст источника сохранён полностью."
        ));
    }
    if result.layout_items.is_empty() {
        let source_reference = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string);
        result.layout_items = layout_items_from_text(&result.text, None, source_reference);
    }
    if result.text.trim().is_empty() {
        return Err("Из источника не удалось получить содержательный текст.".into());
    }
    Ok(result)
}

#[derive(Debug, Clone)]
pub struct NormalizedSourceFragment {
    pub source_reference: String,
    pub text: String,
    pub layout_items: Vec<NormalizedLayoutItem>,
}

/// Split only compound containers. Ordinary documents remain one case even when
/// their layout has page references. The root source prefix is written by the
/// archive/e-mail normalizers and survives nested page/layout references.
pub fn compound_source_fragments(
    source_kind: &str,
    source_text: &str,
    items: &[NormalizedLayoutItem],
) -> Vec<NormalizedSourceFragment> {
    if !matches!(source_kind, "archive" | "email" | "spreadsheet") {
        return vec![NormalizedSourceFragment {
            source_reference: source_kind.to_string(),
            text: source_text.to_string(),
            layout_items: items.to_vec(),
        }];
    }
    let mut grouped = BTreeMap::<String, Vec<NormalizedLayoutItem>>::new();
    for item in items {
        let Some(reference) = item.source_reference.as_deref() else {
            continue;
        };
        let root = reference.split(';').next().unwrap_or(reference).trim();
        if root.is_empty() {
            continue;
        }
        grouped
            .entry(root.to_string())
            .or_default()
            .push(item.clone());
    }
    let mut fragments = grouped
        .into_iter()
        .map(|(source_reference, layout_items)| {
            let mut text = String::new();
            for item in &layout_items {
                if item.text.trim().is_empty() {
                    continue;
                }
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(item.text.trim());
            }
            NormalizedSourceFragment {
                source_reference,
                text,
                layout_items,
            }
        })
        .filter(|fragment| !fragment.text.trim().is_empty())
        .collect::<Vec<_>>();
    fragments.sort_by(|left, right| left.source_reference.cmp(&right.source_reference));
    if fragments.is_empty() {
        fragments.push(NormalizedSourceFragment {
            source_reference: source_kind.to_string(),
            text: source_text.to_string(),
            layout_items: items.to_vec(),
        });
    }
    fragments
}

pub fn apply_layout_to_case(
    source_kind: &str,
    items: &[NormalizedLayoutItem],
    case: &mut SemanticCase,
) {
    case.blocks
        .insert("source.kind".into(), source_kind.trim().to_string());
    case.blocks
        .insert("source.layout_item_count".into(), items.len().to_string());
    let table_rows = items
        .iter()
        .filter(|item| item.item_kind == "table_row")
        .count();
    case.blocks
        .insert("source.table_row_count".into(), table_rows.to_string());

    let mut records = Vec::<SemanticRecord>::with_capacity(items.len().min(MAX_LAYOUT_ITEMS));
    for item in items.iter().take(MAX_LAYOUT_ITEMS) {
        let mut record = SemanticRecord::new();
        record.insert("kind".into(), SemanticAtom::Text(item.item_kind.clone()));
        record.insert("text".into(), SemanticAtom::Text(item.text.clone()));
        record.insert(
            "cells_json".into(),
            SemanticAtom::Text(serde_json::to_string(&item.cells).unwrap_or_else(|_| "[]".into())),
        );
        record.insert(
            "confidence".into(),
            SemanticAtom::Decimal(format!("{:.4}", item.confidence.clamp(0.0, 1.0))),
        );
        if let Some(page_index) = item.page_index {
            record.insert(
                "page_index".into(),
                SemanticAtom::Integer(i64::try_from(page_index).unwrap_or(i64::MAX)),
            );
        }
        if let Some(block_index) = item.block_index {
            record.insert(
                "block_index".into(),
                SemanticAtom::Integer(i64::try_from(block_index).unwrap_or(i64::MAX)),
            );
        }
        if let Some(reference) = item.source_reference.as_ref() {
            record.insert(
                "source_reference".into(),
                SemanticAtom::Text(reference.clone()),
            );
        }
        if let Some(bbox) = item.bbox.as_ref() {
            for (key, value) in [
                ("bbox_left", bbox.left),
                ("bbox_top", bbox.top),
                ("bbox_width", bbox.width),
                ("bbox_height", bbox.height),
            ] {
                record.insert(key.into(), SemanticAtom::Integer(i64::from(value)));
            }
        }
        records.push(record);
    }
    case.collections
        .insert("source.layout_items".into(), records);
}

pub fn attach_layout_evidence(items: &[NormalizedLayoutItem], case: &mut SemanticCase) {
    let source_kind = case
        .blocks
        .get("source.kind")
        .cloned()
        .unwrap_or_else(|| "normalized_source".into());
    for semantic_value in case.values.values_mut() {
        let value_needle = evidence_needle(&semantic_value.value);
        for evidence in &mut semantic_value.evidence {
            if evidence.page_index.is_some() && evidence.source_reference.is_some() {
                continue;
            }
            let excerpt_needle = evidence_needle(&evidence.excerpt);
            let matched = items.iter().find(|item| {
                let haystack = evidence_needle(&item.text);
                (!value_needle.is_empty() && haystack.contains(&value_needle))
                    || (!excerpt_needle.is_empty() && haystack.contains(&excerpt_needle))
                    || (!haystack.is_empty() && excerpt_needle.contains(&haystack))
            });
            if let Some(item) = matched {
                if evidence.page_index.is_none() {
                    evidence.page_index = item.page_index;
                }
                if evidence.source_reference.is_none() {
                    evidence.source_reference = item.source_reference.clone();
                }
                if evidence.source_kind.trim().is_empty() {
                    evidence.source_kind = source_kind.clone();
                }
                evidence.confidence = evidence.confidence.min(item.confidence).clamp(0.0, 1.0);
            }
        }
    }
}

fn evidence_needle(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Debug, Clone, Copy)]
struct PdfTextQuality {
    meaningful: usize,
    alphabetic_ratio: f32,
    replacement_ratio: f32,
    score: f32,
}

fn pdf_text_quality(text: &str) -> PdfTextQuality {
    let characters = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<Vec<_>>();
    let total = characters.len().max(1) as f32;
    let meaningful = meaningful_character_count(text);
    let alphabetic = characters
        .iter()
        .filter(|character| character.is_alphabetic())
        .count() as f32;
    let replacement = characters
        .iter()
        .filter(|character| matches!(character, '\u{fffd}' | '\u{25a1}' | '\u{25a0}'))
        .count() as f32;
    let control = characters
        .iter()
        .filter(|character| character.is_control())
        .count() as f32;
    let alphabetic_ratio = alphabetic / total;
    let replacement_ratio = (replacement + control) / total;
    let length_score = (meaningful as f32 / 240.0).clamp(0.0, 1.0);
    let score =
        (length_score * 0.55 + alphabetic_ratio * 0.45 - replacement_ratio * 1.5).clamp(0.0, 1.0);
    PdfTextQuality {
        meaningful,
        alphabetic_ratio,
        replacement_ratio,
        score,
    }
}

fn pdf_page_requires_ocr(text: &str) -> bool {
    let quality = pdf_text_quality(text);
    quality.meaningful < 80
        || quality.score < 0.38
        || quality.alphabetic_ratio < 0.18
        || quality.replacement_ratio > 0.03
}

fn normalize_pdf(path: &Path, workspace: &Path) -> Result<NormalizedSource, String> {
    let output = run_command(
        "pdftotext",
        &["-layout", path.to_string_lossy().as_ref(), "-"],
    )?;
    let extracted = String::from_utf8_lossy(&output.stdout).to_string();
    // pdftotext separates pages with form-feed. Keep that boundary: a single
    // text-heavy page must never hide nine scanned pages from OCR.
    let mut pages = extracted
        .split('\u{000c}')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    while pages.last().is_some_and(|page| page.trim().is_empty()) {
        pages.pop();
    }
    if pages.is_empty() {
        pages.push(String::new());
    }

    let ocr_pages = pages
        .iter()
        .enumerate()
        .filter_map(|(index, text)| pdf_page_requires_ocr(text).then_some(index))
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    let mut source_kind = if ocr_pages.is_empty() {
        "pdf_text".to_string()
    } else if ocr_pages.len() == pages.len() {
        "scanned_pdf_ocr".to_string()
    } else {
        "mixed_pdf_page_ocr".to_string()
    };
    let images = workspace.join(format!("pdf-ocr-{}", Uuid::new_v4()));
    if !ocr_pages.is_empty() {
        std::fs::create_dir_all(&images).map_err(|error| error.to_string())?;
        warnings.push(format!(
            "Постраничный контроль PDF: OCR применён к страницам {} из {}.",
            ocr_pages
                .iter()
                .map(|index| (index + 1).to_string())
                .collect::<Vec<_>>()
                .join(", "),
            pages.len()
        ));
    }

    let mut text = String::new();
    let mut layout_items = Vec::new();
    let mut ocr_modes = Vec::new();
    for (page_index, page_text) in pages.iter().enumerate() {
        let page_number = page_index + 1;
        let (normalized_page_text, mut page_items) = if ocr_pages.contains(&page_index) {
            let prefix = images.join(format!("page-{page_number}"));
            run_command(
                "pdftoppm",
                &[
                    "-f",
                    &page_number.to_string(),
                    "-l",
                    &page_number.to_string(),
                    "-singlefile",
                    "-png",
                    "-r",
                    "300",
                    path.to_string_lossy().as_ref(),
                    prefix.to_string_lossy().as_ref(),
                ],
            )?;
            let image = prefix.with_extension("png");
            let page_layout = ocr_image_layout(&image, page_index)?;
            ocr_modes.push(format!(
                "стр. {}: PSM {}, ориентация {}°",
                page_number, page_layout.psm, page_layout.orientation_degrees
            ));
            (page_layout.text, page_layout.items)
        } else {
            (
                page_text.clone(),
                layout_items_from_text(page_text, Some(page_index), None),
            )
        };
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&format!("[Страница {page_number}]\n{normalized_page_text}"));
        layout_items.append(&mut page_items);
    }
    if !ocr_pages.is_empty() {
        let _ = std::fs::remove_dir_all(images);
        let table_rows = layout_items
            .iter()
            .filter(|item| item.item_kind == "table_row")
            .count();
        warnings.push(format!(
            "OCR сохранил структурных строк: {}; табличных строк: {table_rows}.",
            layout_items.len()
        ));
        if !ocr_modes.is_empty() {
            warnings.push(format!("OCR-стратегия: {}.", ocr_modes.join("; ")));
        }
        warnings.push("Печатный OCR не гарантирует распознавание рукописи; рукописные поля всегда требуют подтверждения.".into());
    } else {
        source_kind = "pdf_text".into();
    }
    Ok(NormalizedSource {
        text,
        source_kind,
        warnings,
        processed_files: vec![path.to_path_buf()],
        layout_items,
    })
}

fn normalize_image(path: &Path) -> Result<NormalizedSource, String> {
    let page = ocr_image_layout(path, 0)?;
    let table_rows = page
        .items
        .iter()
        .filter(|item| item.item_kind == "table_row")
        .count();
    Ok(NormalizedSource {
        text: page.text,
        source_kind: "scanned_image".into(),
        warnings: vec![
            "Результат OCR требует риск-зависимой проверки критических полей.".into(),
            format!(
                "OCR сохранил структурных строк: {}; табличных строк: {table_rows}; режим PSM {}; ориентация {}°.",
                page.items.len(), page.psm, page.orientation_degrees
            ),
            "Печатный OCR не гарантирует распознавание рукописи; рукописные поля всегда требуют подтверждения.".into(),
        ],
        processed_files: vec![path.to_path_buf()],
        layout_items: page.items,
    })
}

#[derive(Debug, Clone)]
struct OcrPageLayout {
    text: String,
    items: Vec<NormalizedLayoutItem>,
    psm: u8,
    orientation_degrees: i32,
}

#[derive(Debug, Clone)]
struct OcrWord {
    page: usize,
    block: usize,
    paragraph: usize,
    line: usize,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    confidence: f32,
    text: String,
}

fn ocr_languages() -> String {
    std::env::var("DOKKOMPLEKT_OCR_LANGUAGES")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "rus+eng".into())
}

fn detect_ocr_orientation(path: &Path) -> i32 {
    let executable = resolve_tool("tesseract");
    let output = Command::new(executable)
        .args([
            path.to_string_lossy().as_ref(),
            "stdout",
            "-l",
            "osd",
            "--psm",
            "0",
        ])
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return 0;
    };
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    combined
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Rotate:")
                .and_then(|value| value.trim().parse::<i32>().ok())
        })
        .unwrap_or(0)
}

fn ocr_layout_score(items: &[NormalizedLayoutItem]) -> f32 {
    if items.is_empty() {
        return 0.0;
    }
    let characters = items
        .iter()
        .map(|item| meaningful_character_count(&item.text))
        .sum::<usize>();
    let confidence = items.iter().map(|item| item.confidence).sum::<f32>() / items.len() as f32;
    let table_bonus = items
        .iter()
        .filter(|item| item.item_kind == "table_row")
        .count() as f32
        / items.len() as f32;
    ((characters as f32 / 500.0).clamp(0.0, 1.0) * 0.35 + confidence * 0.55 + table_bonus * 0.10)
        .clamp(0.0, 1.0)
}

fn ocr_image_layout(path: &Path, forced_page_index: usize) -> Result<OcrPageLayout, String> {
    let languages = ocr_languages();
    let orientation_degrees = detect_ocr_orientation(path);
    // PSM 1 performs orientation/layout analysis; 6 is strong for uniform text;
    // 11 handles sparse forms and stamps. Pick the best grounded TSV result.
    let mut best: Option<(u8, Vec<NormalizedLayoutItem>, f32)> = None;
    for psm in [1_u8, 6_u8, 11_u8] {
        let psm_text = psm.to_string();
        let output = run_command(
            "tesseract",
            &[
                path.to_string_lossy().as_ref(),
                "stdout",
                "-l",
                &languages,
                "--psm",
                &psm_text,
                "tsv",
            ],
        )?;
        let tsv = String::from_utf8_lossy(&output.stdout);
        let items = parse_tesseract_tsv(&tsv, forced_page_index)?;
        let score = ocr_layout_score(&items);
        if best.as_ref().is_none_or(|(_, _, current)| score > *current) {
            best = Some((psm, items, score));
        }
    }
    let (psm, mut items, _) = best.unwrap_or((6, Vec::new(), 0.0));
    if items.is_empty() {
        let psm_text = psm.to_string();
        let fallback = run_command(
            "tesseract",
            &[
                path.to_string_lossy().as_ref(),
                "stdout",
                "-l",
                &languages,
                "--psm",
                &psm_text,
            ],
        )?;
        let text = String::from_utf8_lossy(&fallback.stdout).to_string();
        items = layout_items_from_text(&text, Some(forced_page_index), None);
    }
    let text = layout_text(&items);
    if text.trim().is_empty() {
        return Err("Tesseract не распознал содержательный текст изображения.".into());
    }
    Ok(OcrPageLayout {
        text,
        items,
        psm,
        orientation_degrees,
    })
}

fn parse_tesseract_tsv(
    tsv: &str,
    forced_page_index: usize,
) -> Result<Vec<NormalizedLayoutItem>, String> {
    let mut lines = BTreeMap::<(usize, usize, usize, usize), Vec<OcrWord>>::new();
    for (index, raw) in tsv.lines().enumerate() {
        if index == 0 && raw.to_ascii_lowercase().starts_with("level\t") {
            continue;
        }
        let columns = raw.splitn(12, '\t').collect::<Vec<_>>();
        if columns.len() < 12 || columns[0] != "5" {
            continue;
        }
        let text = columns[11].trim();
        if text.is_empty() {
            continue;
        }
        let parse_usize = |value: &str| value.parse::<usize>().unwrap_or_default();
        let parse_u32 = |value: &str| value.parse::<u32>().unwrap_or_default();
        let confidence = columns[10].parse::<f32>().unwrap_or(-1.0);
        if confidence < 0.0 {
            continue;
        }
        let word = OcrWord {
            page: parse_usize(columns[1]),
            block: parse_usize(columns[2]),
            paragraph: parse_usize(columns[3]),
            line: parse_usize(columns[4]),
            left: parse_u32(columns[6]),
            top: parse_u32(columns[7]),
            width: parse_u32(columns[8]),
            height: parse_u32(columns[9]),
            confidence: (confidence / 100.0).clamp(0.0, 1.0),
            text: text.to_string(),
        };
        lines
            .entry((word.page, word.block, word.paragraph, word.line))
            .or_default()
            .push(word);
    }

    let mut items = Vec::new();
    for ((_, block, _, _), mut words) in lines {
        words.sort_by_key(|word| word.left);
        let left = words.iter().map(|word| word.left).min().unwrap_or_default();
        let top = words.iter().map(|word| word.top).min().unwrap_or_default();
        let right = words
            .iter()
            .map(|word| word.left.saturating_add(word.width))
            .max()
            .unwrap_or(left);
        let bottom = words
            .iter()
            .map(|word| word.top.saturating_add(word.height))
            .max()
            .unwrap_or(top);
        let confidence =
            words.iter().map(|word| word.confidence).sum::<f32>() / words.len().max(1) as f32;
        let mut text = String::new();
        let mut previous_right = None::<u32>;
        let mut previous_char_width = 8.0_f32;
        for word in &words {
            if let Some(right_edge) = previous_right {
                let gap = word.left.saturating_sub(right_edge);
                let table_gap = (previous_char_width * 3.5).max(24.0) as u32;
                text.push(if gap >= table_gap { '\t' } else { ' ' });
            }
            text.push_str(&word.text);
            previous_right = Some(word.left.saturating_add(word.width));
            previous_char_width = word.width as f32 / word.text.chars().count().max(1) as f32;
        }
        let cells = text
            .split('\t')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let item_kind = if cells.len() >= 2 {
            "table_row"
        } else {
            "text_line"
        };
        items.push(NormalizedLayoutItem {
            item_kind: item_kind.into(),
            page_index: Some(forced_page_index),
            block_index: Some(block),
            text: text.trim().to_string(),
            cells,
            bbox: Some(LayoutBoundingBox {
                left,
                top,
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            }),
            confidence,
            source_reference: Some(format!("page:{};block:{}", forced_page_index + 1, block)),
        });
    }
    items.sort_by(|left, right| {
        let left_box = left.bbox.as_ref();
        let right_box = right.bbox.as_ref();
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| {
                left_box
                    .map(|bbox| bbox.top)
                    .cmp(&right_box.map(|bbox| bbox.top))
            })
            .then_with(|| {
                left_box
                    .map(|bbox| bbox.left)
                    .cmp(&right_box.map(|bbox| bbox.left))
            })
    });
    Ok(items)
}

fn layout_items_from_text(
    text: &str,
    page_index: Option<usize>,
    source_reference: Option<String>,
) -> Vec<NormalizedLayoutItem> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let cells = line
                .split('\t')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            NormalizedLayoutItem {
                item_kind: if cells.len() >= 2 {
                    "table_row"
                } else {
                    "text_line"
                }
                .into(),
                page_index,
                block_index: None,
                text: line.trim().to_string(),
                cells,
                bbox: None,
                confidence: 1.0,
                source_reference: source_reference.clone(),
            }
        })
        .collect()
}

fn normalize_layout_items(items: &mut Vec<NormalizedLayoutItem>) {
    for item in items.iter_mut() {
        item.text = normalize_text(&item.text);
        item.cells = item
            .cells
            .iter()
            .map(|cell| normalize_text(cell))
            .filter(|cell| !cell.is_empty())
            .collect();
        item.confidence = item.confidence.clamp(0.0, 1.0);
    }
    items.retain(|item| !item.text.trim().is_empty());
}

fn layout_text(items: &[NormalizedLayoutItem]) -> String {
    items
        .iter()
        .map(|item| item.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_xlsx(path: &Path) -> Result<NormalizedSource, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("XLSX повреждён: {error}"))?;
    validate_xlsx_archive(&mut archive)?;
    let shared = read_xlsx_shared_strings(&mut archive)?;
    let sheets = read_xlsx_sheet_entries(&mut archive)?;
    let mut output = String::new();
    let mut layout_items = Vec::new();
    let mut formula_count = 0usize;
    for (display_name, sheet_path) in sheets {
        let mut entry = archive
            .by_name(&sheet_path)
            .map_err(|error| format!("Не удалось прочитать лист «{display_name}»: {error}"))?;
        if entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
            return Err(format!(
                "Лист «{display_name}» превышает безопасный распакованный предел."
            ));
        }
        let xml = read_text_limited(
            &mut entry,
            MAX_CONTAINER_XML_BYTES,
            &format!("лист «{display_name}»"),
        )?;
        formula_count += xml.matches("<f").count();
        let sheet = xlsx_sheet_to_text(&xml, &shared)?;
        if !sheet.trim().is_empty() {
            if !output.is_empty() {
                output.push_str("\n\n");
            }
            output.push_str(&format!("[Лист: {display_name}]\n{sheet}"));
            layout_items.extend(layout_items_from_text(
                &sheet,
                None,
                Some(format!("xlsx:{display_name}")),
            ));
        }
    }
    let mut warnings = Vec::new();
    if formula_count > 0 {
        warnings.push(format!(
            "В книге найдено формул: {formula_count}. Использованы сохранённые cached values; при давней пересборке книги значения могут быть устаревшими."
        ));
    }
    warnings.push(
        "Листы читаются по пользовательским именам и разделяются источниками; одинаковые поля с разных листов проходят conflict/case gate."
            .into(),
    );
    Ok(NormalizedSource {
        text: output,
        source_kind: "spreadsheet".into(),
        warnings,
        processed_files: vec![path.to_path_buf()],
        layout_items,
    })
}

fn read_xlsx_sheet_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Vec<(String, String)>, String> {
    let workbook_xml = {
        let mut entry = archive
            .by_name("xl/workbook.xml")
            .map_err(|error| format!("XLSX не содержит workbook.xml: {error}"))?;
        if entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
            return Err("workbook.xml превышает безопасный предел.".into());
        }
        read_text_limited(&mut entry, MAX_CONTAINER_XML_BYTES, "workbook.xml")?
    };
    let relationships_xml = {
        let mut entry = archive
            .by_name("xl/_rels/workbook.xml.rels")
            .map_err(|error| format!("XLSX не содержит workbook relationships: {error}"))?;
        if entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
            return Err("workbook.xml.rels превышает безопасный предел.".into());
        }
        read_text_limited(&mut entry, MAX_CONTAINER_XML_BYTES, "workbook.xml.rels")?
    };
    let mut relationships = BTreeMap::<String, String>::new();
    let mut rel_reader = Reader::from_str(&relationships_xml);
    loop {
        match rel_reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == b"Relationship" =>
            {
                let mut id = None;
                let mut target = None;
                for attribute in event.attributes().flatten() {
                    let key = local_name(attribute.key.as_ref());
                    let value = String::from_utf8_lossy(attribute.value.as_ref()).to_string();
                    if key == b"Id" {
                        id = Some(value);
                    } else if key == b"Target" {
                        target = Some(value);
                    }
                }
                if let (Some(id), Some(target)) = (id, target) {
                    let target = target.trim_start_matches('/');
                    let path = if target.starts_with("xl/") {
                        target.to_string()
                    } else {
                        format!("xl/{}", target.trim_start_matches("../"))
                    };
                    relationships.insert(id, path);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("workbook.xml.rels повреждён: {error}")),
            _ => {}
        }
    }
    let mut sheets = Vec::new();
    let mut workbook_reader = Reader::from_str(&workbook_xml);
    loop {
        match workbook_reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == b"sheet" =>
            {
                let mut name = None;
                let mut relationship_id = None;
                for attribute in event.attributes().flatten() {
                    let key = local_name(attribute.key.as_ref());
                    let value = String::from_utf8_lossy(attribute.value.as_ref()).to_string();
                    if key == b"name" {
                        name = Some(value);
                    } else if key == b"id" {
                        relationship_id = Some(value);
                    }
                }
                if let (Some(name), Some(relationship_id)) = (name, relationship_id) {
                    if let Some(path) = relationships.get(&relationship_id) {
                        if sheets.len() >= MAX_XLSX_SHEETS {
                            return Err(format!("XLSX содержит больше {MAX_XLSX_SHEETS} листов."));
                        }
                        sheets.push((name, path.clone()));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("workbook.xml повреждён: {error}")),
            _ => {}
        }
    }
    if sheets.is_empty() {
        return Err("В XLSX не найдено доступных рабочих листов.".into());
    }
    Ok(sheets)
}

pub(crate) fn mail_merge_upload_to_delimited(
    file_name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "xlsx" => xlsx_bytes_first_sheet_to_tsv(bytes),
        "csv" | "tsv" | "txt" => {
            let text = decode_text_bytes(bytes);
            if text.trim().is_empty() {
                Err("Файл таблицы пуст.".into())
            } else {
                Ok(text)
            }
        }
        _ => Err("Пакетная генерация принимает XLSX, CSV, TSV или TXT.".into()),
    }
}

fn xlsx_bytes_first_sheet_to_tsv(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("XLSX превышает допустимый размер 100 МБ.".into());
    }
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| format!("XLSX повреждён: {error}"))?;
    validate_xlsx_archive(&mut archive)?;
    let shared = read_xlsx_shared_strings(&mut archive)?;
    let sheets = read_xlsx_sheet_entries(&mut archive)?;
    for (display_name, sheet_path) in sheets {
        let mut entry = archive
            .by_name(&sheet_path)
            .map_err(|error| format!("Не удалось прочитать лист «{display_name}»: {error}"))?;
        if entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
            return Err(format!(
                "Лист «{display_name}» превышает безопасный распакованный предел."
            ));
        }
        let xml = read_text_limited(
            &mut entry,
            MAX_CONTAINER_XML_BYTES,
            &format!("лист «{display_name}»"),
        )?;
        let text = xlsx_sheet_to_text(&xml, &shared)?;
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    Err("В XLSX не найден непустой рабочий лист.".into())
}

fn read_xlsx_shared_strings<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Vec<String>, String> {
    let Ok(mut entry) = archive.by_name("xl/sharedStrings.xml") else {
        return Ok(Vec::new());
    };
    if entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
        return Err("sharedStrings.xml превышает безопасный распакованный предел.".into());
    }
    let xml = read_text_limited(&mut entry, MAX_CONTAINER_XML_BYTES, "sharedStrings.xml")?;
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(false);
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if local_name(event.name().as_ref()) == b"si" => {
                in_si = true;
                current.clear();
            }
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == b"si" => {
                in_si = false;
                if values.len() >= MAX_XLSX_SHARED_STRINGS {
                    return Err(format!(
                        "XLSX содержит больше {MAX_XLSX_SHARED_STRINGS} строковых значений."
                    ));
                }
                values.push(current.clone());
            }
            Ok(Event::Text(event)) if in_si => {
                current.push_str(&decode_xml_text(&event)?);
                if current.len() > MAX_XLSX_CELL_BYTES {
                    return Err("Одна строка XLSX превышает безопасный предел 1 МБ.".into());
                }
            }
            Ok(Event::GeneralRef(event)) if in_si => {
                current.push_str(&decode_xml_reference(&event)?);
                if current.len() > MAX_XLSX_CELL_BYTES {
                    return Err("Одна строка XLSX превышает безопасный предел 1 МБ.".into());
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("sharedStrings.xml повреждён: {error}")),
            _ => {}
        }
    }
    Ok(values)
}

fn xlsx_sheet_to_text(xml: &str, shared: &[String]) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut output = String::new();
    let mut current_type = String::new();
    let mut current_value = String::new();
    let mut current_column = 0usize;
    let mut current_row = Vec::<String>::new();
    let mut in_value = false;
    let mut in_inline_text = false;
    let mut cell_count = 0usize;
    let mut row_count = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if local_name(event.name().as_ref()) == b"c" => {
                let reference = event
                    .attributes()
                    .flatten()
                    .find(|attribute| local_name(attribute.key.as_ref()) == b"r")
                    .and_then(|attribute| String::from_utf8(attribute.value.into_owned()).ok());
                current_column = match reference {
                    Some(reference) => xlsx_column_index(&reference).ok_or_else(|| {
                        format!("Некорректная или чрезмерная ссылка ячейки XLSX: {reference}")
                    })?,
                    None => current_row.len(),
                };
                current_type = event
                    .attributes()
                    .flatten()
                    .find(|attribute| local_name(attribute.key.as_ref()) == b"t")
                    .and_then(|attribute| String::from_utf8(attribute.value.into_owned()).ok())
                    .unwrap_or_default();
                current_value.clear();
                cell_count = cell_count.saturating_add(1);
                if cell_count > MAX_XLSX_CELLS {
                    return Err(format!("XLSX содержит больше {MAX_XLSX_CELLS} ячеек."));
                }
            }
            Ok(Event::Start(event)) if local_name(event.name().as_ref()) == b"v" => in_value = true,
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == b"v" => in_value = false,
            Ok(Event::Start(event)) if local_name(event.name().as_ref()) == b"t" => {
                in_inline_text = true
            }
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == b"t" => {
                in_inline_text = false
            }
            Ok(Event::Text(event)) if in_value || in_inline_text => {
                current_value.push_str(&decode_xml_text(&event)?);
                if current_value.len() > MAX_XLSX_CELL_BYTES {
                    return Err("Одна ячейка XLSX превышает безопасный предел 1 МБ.".into());
                }
            }
            Ok(Event::GeneralRef(event)) if in_value || in_inline_text => {
                current_value.push_str(&decode_xml_reference(&event)?);
                if current_value.len() > MAX_XLSX_CELL_BYTES {
                    return Err("Одна ячейка XLSX превышает безопасный предел 1 МБ.".into());
                }
            }
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == b"c" => {
                let value = if current_type == "s" {
                    current_value
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| shared.get(index))
                        .cloned()
                        .unwrap_or(current_value.clone())
                } else {
                    current_value.clone()
                };
                if current_column >= MAX_XLSX_COLUMNS {
                    return Err(format!(
                        "Колонка XLSX превышает предел XFD ({MAX_XLSX_COLUMNS})."
                    ));
                }
                if current_row.len() <= current_column {
                    current_row.resize(current_column + 1, String::new());
                }
                current_row[current_column] = value.trim().to_string();
            }
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == b"row" => {
                row_count = row_count.saturating_add(1);
                if row_count > MAX_XLSX_ROWS {
                    return Err(format!("XLSX содержит больше {MAX_XLSX_ROWS} строк."));
                }
                while current_row.last().is_some_and(|value| value.is_empty()) {
                    current_row.pop();
                }
                output.push_str(&current_row.join("\t"));
                output.push('\n');
                if output.len() > MAX_NORMALIZED_TEXT_BYTES {
                    return Err("Текст одного листа XLSX превышает безопасный предел 32 МБ.".into());
                }
                current_row.clear();
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Лист XLSX повреждён: {error}")),
            _ => {}
        }
    }
    Ok(output)
}

fn xlsx_column_index(reference: &str) -> Option<usize> {
    let mut value = 0usize;
    let mut letters = 0usize;
    for character in reference.chars() {
        if !character.is_ascii_alphabetic() {
            break;
        }
        letters += 1;
        if letters > 3 {
            return None;
        }
        let digit = usize::from(character.to_ascii_uppercase() as u8 - b'A' + 1);
        value = value.checked_mul(26)?.checked_add(digit)?;
    }
    if letters == 0 || value == 0 || value > MAX_XLSX_COLUMNS {
        None
    } else {
        Some(value - 1)
    }
}

fn normalize_odt(path: &Path) -> Result<NormalizedSource, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("ODT повреждён: {error}"))?;
    let mut entry = archive
        .by_name("content.xml")
        .map_err(|_| "В ODT отсутствует content.xml".to_string())?;
    if entry.size() > MAX_CONTAINER_XML_BYTES as u64 {
        return Err("content.xml ODT превышает безопасный распакованный предел.".into());
    }
    let xml = read_text_limited(&mut entry, MAX_CONTAINER_XML_BYTES, "content.xml ODT")?;
    Ok(NormalizedSource {
        text: generic_xml_to_text(&xml)?,
        source_kind: "odt".into(),
        warnings: Vec::new(),
        processed_files: vec![path.to_path_buf()],
        layout_items: Vec::new(),
    })
}

fn normalize_rtf(path: &Path) -> Result<NormalizedSource, String> {
    let bytes = read_file_limited(path, MAX_UPLOAD_BYTES, "RTF")?;
    Ok(NormalizedSource {
        text: rtf_to_text(&bytes),
        source_kind: "rtf".into(),
        warnings: Vec::new(),
        processed_files: vec![path.to_path_buf()],
        layout_items: Vec::new(),
    })
}

fn normalize_plain_text(path: &Path, extension: &str) -> Result<NormalizedSource, String> {
    let bytes = read_file_limited(path, MAX_UPLOAD_BYTES, "текстовый источник")?;
    let text = decode_text_bytes(&bytes);
    let normalized = if extension == "json" {
        serde_json::from_str::<serde_json::Value>(&text)
            .map(|value| serde_json::to_string_pretty(&value).unwrap_or(text.clone()))
            .unwrap_or(text)
    } else if extension == "xml" {
        generic_xml_to_text(&text).unwrap_or(text)
    } else {
        text
    };
    Ok(NormalizedSource {
        text: normalized,
        source_kind: extension.into(),
        warnings: Vec::new(),
        processed_files: vec![path.to_path_buf()],
        layout_items: Vec::new(),
    })
}

fn normalize_html(path: &Path) -> Result<NormalizedSource, String> {
    let bytes = read_file_limited(path, MAX_UPLOAD_BYTES, "HTML")?;
    Ok(NormalizedSource {
        text: html_to_text(&decode_text_bytes(&bytes)),
        source_kind: "html".into(),
        warnings: Vec::new(),
        processed_files: vec![path.to_path_buf()],
        layout_items: Vec::new(),
    })
}

fn normalize_eml(path: &Path, workspace: &Path, depth: usize) -> Result<NormalizedSource, String> {
    let raw = decode_text_bytes(&read_file_limited(path, MAX_UPLOAD_BYTES, "EML")?);
    let mut text = String::new();
    let mut warnings = Vec::new();
    let mut attachment_layout = Vec::new();
    let (headers, body) = split_headers_body(&raw);
    for header in ["From", "To", "Cc", "Date", "Subject"] {
        if let Some(value) = header_value(headers, header) {
            text.push_str(&format!("{header}: {value}\n"));
        }
    }
    text.push('\n');
    let content_type = header_value(headers, "Content-Type").unwrap_or_else(|| "text/plain".into());
    if let Some(boundary) = mime_boundary(&content_type) {
        for part in body.split(&format!("--{boundary}")) {
            let (part_headers, part_body) = split_headers_body(part);
            let part_type =
                header_value(part_headers, "Content-Type").unwrap_or_else(|| "text/plain".into());
            let encoding =
                header_value(part_headers, "Content-Transfer-Encoding").unwrap_or_default();
            let decoded = decode_mime_body(part_body, &encoding);
            if part_type.to_ascii_lowercase().starts_with("text/plain") {
                text.push_str(&decode_text_bytes(&decoded));
                text.push('\n');
            } else if part_type.to_ascii_lowercase().starts_with("text/html") {
                text.push_str(&html_to_text(&decode_text_bytes(&decoded)));
                text.push('\n');
            } else if let Some(name) = mime_file_name(part_headers) {
                let attachment_dir = workspace.join(format!("mail-{}", Uuid::new_v4()));
                std::fs::create_dir_all(&attachment_dir).map_err(|error| error.to_string())?;
                let attachment = attachment_dir.join(safe_file_name(&name));
                std::fs::write(&attachment, decoded).map_err(|error| error.to_string())?;
                if is_supported_path(&attachment) {
                    match normalize_path(&attachment, workspace, depth + 1) {
                        Ok(nested) => {
                            text.push_str(&format!("\n[Вложение: {name}]\n{}\n", nested.text));
                            let mut nested_layout = nested.layout_items;
                            archive::prefix_layout_source(
                                &mut nested_layout,
                                &format!("email_attachment:{name}"),
                            );
                            attachment_layout.extend(nested_layout);
                            warnings.extend(nested.warnings);
                        }
                        Err(error) => {
                            warnings.push(format!("Вложение «{name}» не обработано: {error}"))
                        }
                    }
                }
            }
        }
    } else {
        let encoding = header_value(headers, "Content-Transfer-Encoding").unwrap_or_default();
        let decoded = decode_mime_body(body, &encoding);
        if content_type.to_ascii_lowercase().starts_with("text/html") {
            text.push_str(&html_to_text(&decode_text_bytes(&decoded)));
        } else {
            text.push_str(&decode_text_bytes(&decoded));
        }
    }
    let source_reference = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|name| format!("email:{name}"));
    let mut layout_items = layout_items_from_text(&text, None, source_reference);
    layout_items.extend(attachment_layout);
    Ok(NormalizedSource {
        text,
        source_kind: "email".into(),
        warnings,
        processed_files: vec![path.to_path_buf()],
        layout_items,
    })
}

fn normalize_office_via_libreoffice(
    path: &Path,
    workspace: &Path,
    target_extension: &str,
) -> Result<NormalizedSource, String> {
    let output_dir = workspace.join(format!("office-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    run_command(
        "soffice",
        &[
            "--headless",
            "--convert-to",
            target_extension,
            "--outdir",
            output_dir.to_string_lossy().as_ref(),
            path.to_string_lossy().as_ref(),
        ],
    )?;
    let converted = std::fs::read_dir(&output_dir)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .find(|item| {
            item.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(target_extension))
        })
        .ok_or_else(|| "LibreOffice не создал преобразованный файл.".to_string())?;
    let result = normalize_path(&converted, workspace, 0)?;
    let _ = std::fs::remove_dir_all(output_dir);
    Ok(result)
}

fn command_available(program: &str) -> bool {
    let executable = resolve_tool(program);
    Command::new(executable)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub(crate) fn resolve_tool(program: &str) -> PathBuf {
    let executable_name = if cfg!(windows) && !program.to_ascii_lowercase().ends_with(".exe") {
        format!("{program}.exe")
    } else {
        program.to_string()
    };
    if let Some(path) =
        crate::component_manager::resolve_trusted_component_tool(program, &executable_name)
    {
        return path;
    }
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("DOKKOMPLEKT_TOOLS_DIR") {
        append_tool_candidates(
            &mut candidates,
            &PathBuf::from(dir),
            program,
            &executable_name,
        );
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            append_tool_candidates(
                &mut candidates,
                &parent.join("tools"),
                program,
                &executable_name,
            );
            append_tool_candidates(
                &mut candidates,
                &parent.join("resources").join("tools"),
                program,
                &executable_name,
            );
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(program))
}

fn append_tool_candidates(
    candidates: &mut Vec<PathBuf>,
    root: &Path,
    program: &str,
    executable_name: &str,
) {
    let platform = std::env::consts::OS;
    let architecture = std::env::consts::ARCH;
    let platform_arch = format!("{platform}-{architecture}");
    candidates.extend([
        root.join(executable_name),
        root.join(platform).join(executable_name),
        root.join(&platform_arch).join(executable_name),
        root.join(program).join(executable_name),
        root.join(program).join(platform).join(executable_name),
        root.join(program)
            .join(&platform_arch)
            .join(executable_name),
    ]);
    match program {
        "pdftotext" | "pdftoppm" => {
            candidates.push(root.join("poppler").join("bin").join(executable_name));
            candidates.push(
                root.join("poppler")
                    .join(&platform_arch)
                    .join("bin")
                    .join(executable_name),
            );
        }
        "soffice" => {
            candidates.push(
                root.join("libreoffice")
                    .join("program")
                    .join(executable_name),
            );
            candidates.push(
                root.join("libreoffice")
                    .join(&platform_arch)
                    .join("program")
                    .join(executable_name),
            );
        }
        "7z" => {
            candidates.push(root.join("7zip").join(executable_name));
            candidates.push(root.join("7zip").join(&platform_arch).join(executable_name));
        }
        "sumatrapdf" => {
            candidates.push(root.join("sumatrapdf").join("SumatraPDF.exe"));
            candidates.push(
                root.join("sumatrapdf")
                    .join(&platform_arch)
                    .join("SumatraPDF.exe"),
            );
        }
        _ => {}
    }
}

fn run_command(program: &str, args: &[&str]) -> Result<std::process::Output, String> {
    run_command_in(Path::new("."), program, args)
}

fn run_command_in(
    cwd: &Path,
    program: &str,
    args: &[&str],
) -> Result<std::process::Output, String> {
    let executable = resolve_tool(program);
    let mut command = Command::new(executable);
    command
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Не найден или не запускается «{program}»: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("Не удалось перехватить stdout процесса «{program}»."))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Не удалось перехватить stderr процесса «{program}»."))?;
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.read_to_end(&mut bytes);
        (result, bytes)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr.read_to_end(&mut bytes);
        (result, bytes)
    });

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let (_, stderr_bytes) = stderr_reader
                .join()
                .map_err(|_| format!("Поток stderr процесса «{program}» аварийно завершился."))?;
            let _ = stdout_reader.join();
            let stderr_text = String::from_utf8_lossy(&stderr_bytes);
            let detail = stderr_text.trim();
            return Err(if detail.is_empty() {
                format!(
                    "«{program}» не завершился за {} секунд.",
                    COMMAND_TIMEOUT.as_secs()
                )
            } else {
                format!(
                    "«{program}» не завершился за {} секунд: {detail}",
                    COMMAND_TIMEOUT.as_secs()
                )
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let (stdout_result, stdout_bytes) = stdout_reader
        .join()
        .map_err(|_| format!("Поток stdout процесса «{program}» аварийно завершился."))?;
    stdout_result.map_err(|error| format!("Ошибка чтения stdout «{program}»: {error}"))?;
    let (stderr_result, stderr_bytes) = stderr_reader
        .join()
        .map_err(|_| format!("Поток stderr процесса «{program}» аварийно завершился."))?;
    stderr_result.map_err(|error| format!("Ошибка чтения stderr «{program}»: {error}"))?;

    let output = std::process::Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    };
    if !output.status.success() {
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "«{program}» завершился с ошибкой: {}",
            stderr_text.trim()
        ));
    }
    Ok(output)
}

fn generic_xml_to_text(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut output = String::new();
    let block_tags = BTreeSet::from(["p", "h", "tr", "row", "table-row", "list-item"]);
    let cell_tags = BTreeSet::from(["tc", "cell", "table-cell"]);
    loop {
        match reader.read_event() {
            Ok(Event::Text(event)) => output.push_str(&decode_xml_text(&event)?),
            Ok(Event::GeneralRef(event)) => output.push_str(&decode_xml_reference(&event)?),
            Ok(Event::End(event)) => {
                let name =
                    String::from_utf8_lossy(local_name(event.name().as_ref())).to_ascii_lowercase();
                if block_tags.contains(name.as_str()) {
                    output.push('\n');
                } else if cell_tags.contains(name.as_str()) {
                    output.push('\t');
                }
            }
            Ok(Event::Empty(event)) if local_name(event.name().as_ref()) == b"br" => {
                output.push('\n')
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("XML повреждён: {error}")),
            _ => {}
        }
    }
    Ok(output)
}

fn decode_xml_text(event: &quick_xml::events::BytesText<'_>) -> Result<String, String> {
    event
        .decode()
        .map(|value| value.into_owned())
        .map_err(|error| error.to_string())
}

fn decode_xml_reference(event: &quick_xml::events::BytesRef<'_>) -> Result<String, String> {
    if let Some(character) = event
        .resolve_char_ref()
        .map_err(|error| error.to_string())?
    {
        return Ok(character.to_string());
    }
    let name = event.decode().map_err(|error| error.to_string())?;
    quick_xml::escape::resolve_predefined_entity(&name)
        .map(str::to_owned)
        .ok_or_else(|| format!("неизвестная XML-сущность: &{name};"))
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn html_to_text(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_depth = 0_u32;
    for character in html.chars() {
        match character {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let raw = tag.trim().to_ascii_lowercase();
                let closing = raw.starts_with('/');
                let name = raw
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('/');
                if matches!(name, "script" | "style" | "noscript") {
                    if closing {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else if !raw.ends_with('/') {
                        skip_depth = skip_depth.saturating_add(1);
                    }
                }
                if skip_depth == 0
                    && (closing
                        && matches!(
                            name,
                            "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                        )
                        || !closing && name == "br")
                {
                    output.push('\n');
                } else if skip_depth == 0 && closing && matches!(name, "td" | "th") {
                    output.push('\t');
                }
            }
            _ if in_tag || skip_depth > 0 => {
                if in_tag {
                    tag.push(character);
                }
            }
            _ => output.push(character),
        }
    }
    decode_html_entities(&output)
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn rtf_to_text(bytes: &[u8]) -> String {
    #[derive(Clone, Copy)]
    struct RtfState {
        skip_destination: bool,
        unicode_fallback_len: usize,
        code_page: u16,
    }

    let input = String::from_utf8_lossy(bytes);
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    let mut state = RtfState {
        skip_destination: false,
        unicode_fallback_len: 1,
        code_page: 1252,
    };
    let mut stack = Vec::new();
    let mut fallback_to_skip = 0_usize;

    while let Some(character) = chars.next() {
        match character {
            '{' => stack.push(state),
            '}' => {
                state = stack.pop().unwrap_or(state);
                fallback_to_skip = 0;
            }
            '\\' => {
                let Some(next) = chars.peek().copied() else {
                    break;
                };
                match next {
                    '\\' | '{' | '}' => {
                        let literal = chars.next().unwrap_or_default();
                        if fallback_to_skip > 0 {
                            fallback_to_skip -= 1;
                        } else if !state.skip_destination {
                            output.push(literal);
                        }
                    }
                    '\'' => {
                        chars.next();
                        let first = chars.next();
                        let second = chars.next();
                        if fallback_to_skip > 0 {
                            fallback_to_skip -= 1;
                            continue;
                        }
                        if !state.skip_destination {
                            if let (Some(first), Some(second)) = (first, second) {
                                let hex = [first, second].iter().collect::<String>();
                                if let Ok(value) = u8::from_str_radix(&hex, 16) {
                                    output.push(decode_rtf_ansi_byte(value, state.code_page));
                                }
                            }
                        }
                    }
                    '*' => {
                        chars.next();
                        state.skip_destination = true;
                    }
                    '~' => {
                        chars.next();
                        if fallback_to_skip > 0 {
                            fallback_to_skip -= 1;
                        } else if !state.skip_destination {
                            output.push('\u{00a0}');
                        }
                    }
                    '-' => {
                        chars.next();
                        fallback_to_skip = fallback_to_skip.saturating_sub(1);
                    }
                    '_' => {
                        chars.next();
                        if fallback_to_skip > 0 {
                            fallback_to_skip -= 1;
                        } else if !state.skip_destination {
                            output.push('\u{2011}');
                        }
                    }
                    _ => {
                        let mut word = String::new();
                        while chars
                            .peek()
                            .is_some_and(|value| value.is_ascii_alphabetic())
                        {
                            word.push(chars.next().unwrap_or_default());
                        }
                        let mut number = String::new();
                        if chars.peek() == Some(&'-') {
                            number.push(chars.next().unwrap_or_default());
                        }
                        while chars.peek().is_some_and(|value| value.is_ascii_digit()) {
                            number.push(chars.next().unwrap_or_default());
                        }
                        if chars.peek() == Some(&' ') {
                            chars.next();
                        }

                        match word.as_str() {
                            "uc" => {
                                if let Ok(value) = number.parse::<usize>() {
                                    state.unicode_fallback_len = value.min(16);
                                }
                            }
                            "ansicpg" => {
                                if let Ok(value) = number.parse::<u16>() {
                                    state.code_page = value;
                                }
                            }
                            "u" => {
                                if !state.skip_destination {
                                    if let Ok(value) = number.parse::<i32>() {
                                        let code = if value < 0 { value + 65_536 } else { value };
                                        if let Some(character) = char::from_u32(code as u32) {
                                            output.push(character);
                                        }
                                    }
                                }
                                fallback_to_skip = state.unicode_fallback_len;
                            }
                            "par" | "line" => {
                                if fallback_to_skip > 0 {
                                    fallback_to_skip -= 1;
                                } else if !state.skip_destination {
                                    output.push('\n');
                                }
                            }
                            "tab" => {
                                if fallback_to_skip > 0 {
                                    fallback_to_skip -= 1;
                                } else if !state.skip_destination {
                                    output.push('\t');
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ if fallback_to_skip > 0 => fallback_to_skip -= 1,
            _ if !state.skip_destination => output.push(character),
            _ => {}
        }
    }
    output
}

fn decode_rtf_ansi_byte(value: u8, code_page: u16) -> char {
    if value.is_ascii() {
        return value as char;
    }
    if code_page == 1251 {
        return match value {
            0xA8 => 'Ё',
            0xB8 => 'ё',
            0xC0..=0xFF => char::from_u32(0x0410 + u32::from(value - 0xC0)).unwrap_or('\u{fffd}'),
            _ => CP1251_EXTENDED[(value - 0x80) as usize],
        };
    }
    CP1252_EXTENDED[(value - 0x80) as usize]
}

const CP1251_EXTENDED: [char; 64] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', 'ђ', '‘', '’',
    '“', '”', '•', '–', '—', '\u{fffd}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', '\u{00a0}', 'Ў', 'ў',
    'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{00ad}', '®', 'Ї', '°', '±', 'І', 'і',
    'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї',
];

const CP1252_EXTENDED: [char; 128] = [
    '€', '\u{0081}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{008d}', 'Ž',
    '\u{008f}', '\u{0090}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{009d}',
    'ž', 'Ÿ', '\u{00a0}', '¡', '¢', '£', '¤', '¥', '¦', '§', '¨', '©', 'ª', '«', '¬', '\u{00ad}',
    '®', '¯', '°', '±', '²', '³', '´', 'µ', '¶', '·', '¸', '¹', 'º', '»', '¼', '½', '¾', '¿', 'À',
    'Á', 'Â', 'Ã', 'Ä', 'Å', 'Æ', 'Ç', 'È', 'É', 'Ê', 'Ë', 'Ì', 'Í', 'Î', 'Ï', 'Ð', 'Ñ', 'Ò', 'Ó',
    'Ô', 'Õ', 'Ö', '×', 'Ø', 'Ù', 'Ú', 'Û', 'Ü', 'Ý', 'Þ', 'ß', 'à', 'á', 'â', 'ã', 'ä', 'å', 'æ',
    'ç', 'è', 'é', 'ê', 'ë', 'ì', 'í', 'î', 'ï', 'ð', 'ñ', 'ò', 'ó', 'ô', 'õ', 'ö', '÷', 'ø', 'ù',
    'ú', 'û', 'ü', 'ý', 'þ', 'ÿ',
];

fn split_headers_body(input: &str) -> (&str, &str) {
    input
        .split_once("\r\n\r\n")
        .or_else(|| input.split_once("\n\n"))
        .unwrap_or(("", input))
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    let mut current_name = String::new();
    let mut current_value = String::new();
    for line in headers.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            current_value.push(' ');
            current_value.push_str(line.trim());
            continue;
        }
        if current_name.eq_ignore_ascii_case(name) {
            return Some(current_value.trim().to_string());
        }
        if let Some((header_name, value)) = line.split_once(':') {
            current_name = header_name.trim().to_string();
            current_value = value.trim().to_string();
        }
    }
    current_name
        .eq_ignore_ascii_case(name)
        .then(|| current_value.trim().to_string())
}

fn mime_boundary(content_type: &str) -> Option<String> {
    content_type.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        key.eq_ignore_ascii_case("boundary")
            .then(|| value.trim().trim_matches('"').to_string())
    })
}

fn mime_file_name(headers: &str) -> Option<String> {
    for header in ["Content-Disposition", "Content-Type"] {
        let Some(value) = header_value(headers, header) else {
            continue;
        };
        for part in value.split(';') {
            let Some((key, raw)) = part.trim().split_once('=') else {
                continue;
            };
            if key.eq_ignore_ascii_case("filename") || key.eq_ignore_ascii_case("name") {
                let value = raw.trim().trim_matches('"').trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn decode_mime_body(body: &str, encoding: &str) -> Vec<u8> {
    if encoding.eq_ignore_ascii_case("base64") {
        let compact = body
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        BASE64_STANDARD.decode(compact).unwrap_or_default()
    } else if encoding.eq_ignore_ascii_case("quoted-printable") {
        decode_quoted_printable(body)
    } else {
        body.as_bytes().to_vec()
    }
}

fn decode_quoted_printable(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n') {
                index += 3;
                continue;
            }
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 2;
                continue;
            }
            if let (Some(a), Some(b)) = (bytes.get(index + 1), bytes.get(index + 2)) {
                let hex = [*a, *b];
                if let Ok(text) = std::str::from_utf8(&hex) {
                    if let Ok(value) = u8::from_str_radix(text, 16) {
                        output.push(value);
                        index += 3;
                        continue;
                    }
                }
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    output
}

fn decode_text_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|byte| *byte as char).collect())
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn meaningful_character_count(text: &str) -> usize {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .count()
}

fn safe_file_name(input: &str) -> String {
    Path::new(input)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("source")
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>()
}

fn metadata_is_link_like(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn walk_files_bounded(
    root: &Path,
    limit: usize,
    byte_limit: u64,
) -> Result<(Vec<PathBuf>, u64), String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total = 0_u64;
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata_is_link_like(&metadata) {
                return Err("Распакованный архив содержит ссылку/reparse point.".into());
            }
            let canonical = path.canonicalize().map_err(|error| error.to_string())?;
            if !canonical.starts_with(&canonical_root) {
                return Err("Распакованный архив вышел за пределы безопасной папки.".into());
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| "Переполнение размера архива.".to_string())?;
                if total > byte_limit {
                    return Err(format!(
                        "После распаковки превышен предел {byte_limit} байт."
                    ));
                }
                files.push(path);
                if files.len() > limit {
                    return Err(format!(
                        "После распаковки обнаружено больше {limit} файлов."
                    ));
                }
            } else {
                return Err("Распакованный архив содержит специальный файл.".into());
            }
        }
    }
    Ok((files, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_formats_cover_requested_universal_intake() {
        for extension in [
            "pdf", "jpg", "png", "xlsx", "xls", "odt", "rtf", "eml", "msg", "zip", "7z", "rar",
        ] {
            assert!(
                supported_extensions().contains(&extension),
                "missing {extension}"
            );
        }
    }

    #[test]
    fn pdf_ocr_decision_is_page_local() {
        let rich_text_page = "Договор ".repeat(30);
        let scanned_page = "  \n";
        assert!(!pdf_page_requires_ocr(&rich_text_page));
        assert!(pdf_page_requires_ocr(scanned_page));
    }

    #[test]
    fn rtf_decoder_preserves_unicode_and_lines() {
        let text = rtf_to_text(r#"{\rtf1\uc1 Первый\par \u1042?торой}"#.as_bytes());
        assert!(text.contains("Первый"), "{text}");
        assert!(text.contains("Второй"), "{text}");
        assert!(text.contains('\n'), "{text}");
    }

    #[test]
    fn rtf_decoder_supports_windows_1251_hex_escapes() {
        let text = rtf_to_text(br#"{\rtf1\ansi\ansicpg1251 \'cf\'f0\'e8\'e2\'e5\'f2}"#);
        assert!(text.contains("Привет"), "{text}");
    }

    #[test]
    fn rtf_decoder_respects_unicode_fallback_length() {
        let text = rtf_to_text(br#"{\rtf1\uc2 \u1042??test}"#);
        assert!(text.contains("Вtest"), "{text}");
        assert!(!text.contains('?'), "{text}");
    }

    #[test]
    fn html_decoder_removes_scripts_and_preserves_table_boundaries() {
        let text =
            html_to_text("<script>bad()</script><table><tr><td>A</td><td>B</td></tr></table>");
        assert!(!text.contains("bad"));
        assert!(text.contains("A\tB"));
    }

    #[test]
    fn private_networks_are_rejected() {
        for address in [
            "127.0.0.1",
            "10.1.2.3",
            "100.64.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(address.parse().unwrap()), "{address}");
        }
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn tesseract_tsv_preserves_page_coordinates_and_table_cells() {
        let tsv = concat!(
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
            "5\t1\t2\t1\t1\t1\t10\t20\t30\t12\t96.0\tИНН\n",
            "5\t1\t2\t1\t1\t2\t210\t20\t80\t12\t94.0\t7736050003\n",
            "5\t1\t3\t1\t1\t1\t10\t60\t40\t12\t93.0\tОбычная\n",
            "5\t1\t3\t1\t1\t2\t58\t60\t50\t12\t92.0\tстрока\n",
        );
        let items = parse_tesseract_tsv(tsv, 2).expect("valid tsv");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_kind, "table_row");
        assert_eq!(
            items[0].cells,
            vec!["ИНН".to_string(), "7736050003".to_string()]
        );
        assert_eq!(items[0].page_index, Some(2));
        assert_eq!(items[0].bbox.as_ref().map(|bbox| bbox.left), Some(10));
        assert_eq!(items[1].item_kind, "text_line");
        assert_eq!(items[1].text, "Обычная строка");
    }

    #[test]
    fn real_image_only_pdf_tsv_fixture_preserves_table_rows() {
        let tsv = include_str!("../../tests/fixtures/ocr/scanned_table.tesseract.tsv");
        let items = parse_tesseract_tsv(tsv, 0).expect("golden OCR TSV must parse");
        let table_rows = items
            .iter()
            .filter(|item| item.item_kind == "table_row")
            .collect::<Vec<_>>();
        assert!(table_rows.len() >= 4, "{table_rows:#?}");
        let combined = items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for token in ["02.07.2026", "500100732259", "Сидорова", "6671000014"] {
            assert!(combined.contains(token), "missing {token}: {combined}");
        }
        assert!(table_rows.iter().all(|item| item.page_index == Some(0)));
        assert!(table_rows.iter().all(|item| item.bbox.is_some()));
    }

    #[test]
    fn layout_is_exposed_to_case_and_enriches_field_evidence() {
        use dokkomplekt_core::{SemanticValue, ValueEvidence, ValueSource};

        let items = vec![NormalizedLayoutItem {
            item_kind: "table_row".into(),
            page_index: Some(4),
            block_index: Some(7),
            text: "ИНН 7736050003".into(),
            cells: vec!["ИНН".into(), "7736050003".into()],
            bbox: Some(LayoutBoundingBox {
                left: 10,
                top: 20,
                width: 200,
                height: 12,
            }),
            confidence: 0.93,
            source_reference: Some("page:5;block:7".into()),
        }];
        let mut case = SemanticCase::default();
        case.values.insert(
            "org.inn".into(),
            SemanticValue::new("org.inn", "7736050003", ValueSource::Scanner, 0.98)
                .with_evidence(ValueEvidence::new("", "7736050003", "deterministic", 0.98)),
        );

        apply_layout_to_case("scanned_pdf_ocr", &items, &mut case);
        attach_layout_evidence(&items, &mut case);

        assert_eq!(
            case.blocks.get("source.kind").map(String::as_str),
            Some("scanned_pdf_ocr")
        );
        assert_eq!(
            case.collection("source.layout_items")
                .map(|records| records.len()),
            Some(1)
        );
        let evidence = &case.values["org.inn"].evidence[0];
        assert_eq!(evidence.page_index, Some(4));
        assert_eq!(evidence.source_reference.as_deref(), Some("page:5;block:7"));
        assert_eq!(evidence.source_kind, "scanned_pdf_ocr");
        assert!(evidence.confidence <= 0.93);
    }

    #[test]
    fn archive_paths_are_sanitized() {
        assert_eq!(safe_file_name("../../secret.pdf"), "secret.pdf");
        for unsafe_path in [
            "../secret.txt",
            "/etc/passwd",
            "C:/secret.txt",
            "safe/file.txt:ads",
            "safe//file.txt",
            "safe/*.txt",
            "CON.txt",
            "folder. /file.txt",
        ] {
            assert!(
                validate_archive_relative_path(unsafe_path).is_err(),
                "{unsafe_path}"
            );
        }
        assert_eq!(
            validate_archive_relative_path("safe/folder/file.txt").unwrap(),
            PathBuf::from("safe/folder/file.txt")
        );
        assert_eq!(
            validate_archive_relative_path("safe/folder/").unwrap(),
            PathBuf::from("safe/folder")
        );
    }

    #[test]
    fn xlsx_rejects_columns_beyond_xfd_before_allocating_a_row() {
        assert_eq!(xlsx_column_index("XFD1"), Some(16_383));
        assert_eq!(xlsx_column_index("XFE1"), None);
        assert_eq!(xlsx_column_index("AAAAAAA1"), None);
        let malicious = r#"<worksheet><sheetData><row><c r="AAAAAAA1"><v>1</v></c></row></sheetData></worksheet>"#;
        let error = xlsx_sheet_to_text(malicious, &[]).unwrap_err();
        assert!(error.contains("чрезмерная ссылка"), "{error}");
    }

    #[test]
    fn external_archive_listing_is_preflighted_before_extraction() {
        let listing = "Path = archive.7z
Type = 7z

----------
Path = safe/file.txt
Size = 12
Folder = -
Attributes = A

Path = link
Size = 0
Folder = -
Symbolic Link = ../../secret
";
        let entries = parse_7z_technical_listing(listing).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].size, 12);
        assert!(entries[1].link_like);
    }

    #[test]
    fn uploaded_source_session_deletes_plaintext_on_successful_drop() {
        let workspace = std::env::temp_dir().join(format!("dkk-intake-{}", Uuid::new_v4()));
        let root = {
            let mut session =
                normalize_uploaded_bytes("patient.txt", b"secret medical data", &workspace)
                    .expect("session");
            let root = session.root().to_path_buf();
            assert!(root.join("patient.txt").is_file());
            let source = session.take_source().expect("source");
            assert!(source.text.contains("secret medical data"));
            root
        };
        assert!(!root.exists());
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn uploaded_source_session_deletes_plaintext_after_parse_error() {
        let workspace = std::env::temp_dir().join(format!("dkk-intake-{}", Uuid::new_v4()));
        let result = normalize_uploaded_bytes("patient.unknown", b"secret", &workspace);
        assert!(result.is_err());
        let entries = std::fs::read_dir(&workspace)
            .map(|items| items.count())
            .unwrap_or_default();
        assert_eq!(entries, 0);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn cleanup_never_removes_a_live_sensitive_session() {
        let workspace = std::env::temp_dir().join(format!("dkk-intake-{}", Uuid::new_v4()));
        let session = normalize_uploaded_bytes("patient.txt", b"secret", &workspace).unwrap();
        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 0);
        assert!(session.root().exists());
        drop(session);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn retained_learning_session_survives_zero_hour_cleanup_while_lease_is_active() {
        let workspace = std::env::temp_dir().join(format!("dkk-learning-{}", Uuid::new_v4()));
        let session = create_retained_workspace_session(&workspace).expect("learning session");
        let source = session.join("example.txt");
        std::fs::write(&source, b"sensitive learning example").expect("example");
        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 0);
        assert!(source.is_file());
        assert!(refresh_retained_workspace_session(&workspace, &source).unwrap());
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn zero_hour_cleanup_removes_released_learning_session_without_touching_other_root() {
        let workspace = std::env::temp_dir().join(format!("dkk-learning-{}", Uuid::new_v4()));
        let other = std::env::temp_dir().join(format!("dkk-learning-other-{}", Uuid::new_v4()));
        let session = create_retained_workspace_session(&workspace).expect("learning session");
        std::fs::write(session.join("example.txt"), b"sensitive").expect("example");
        std::fs::remove_file(session.join(ACTIVE_SESSION_MARKER)).expect("release lease");
        std::fs::create_dir_all(&other).expect("other root");
        let other_file = other.join("user-owned.txt");
        std::fs::write(&other_file, b"must survive").expect("other file");
        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 1);
        assert!(!session.exists());
        assert!(other_file.is_file());
        let _ = std::fs::remove_dir_all(workspace);
        let _ = std::fs::remove_dir_all(other);
    }

    #[test]
    fn retained_learning_lease_refresh_ignores_paths_outside_workspace() {
        let workspace = std::env::temp_dir().join(format!("dkk-learning-{}", Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("outside-{}.txt", Uuid::new_v4()));
        std::fs::write(&outside, b"external").expect("outside");
        assert!(!refresh_retained_workspace_session(&workspace, &outside).unwrap());
        assert!(outside.is_file());
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn retained_source_uses_virtual_path_and_materializes_only_inside_raii_session() {
        let workspace = std::env::temp_dir().join(format!("dkk-retained-{}", Uuid::new_v4()));
        let retained = RetainedUploadedSource::new("patient.docx", b"not-a-real-docx").unwrap();
        assert_eq!(
            retained.virtual_path(),
            "dokkomplekt-upload://current/patient.docx"
        );
        let root = {
            let session = retained.materialize(&workspace).unwrap();
            let root = session.root().to_path_buf();
            assert!(session.original_path().unwrap().is_file());
            root
        };
        assert!(!root.exists());
        let _ = std::fs::remove_dir_all(workspace);
    }
}
