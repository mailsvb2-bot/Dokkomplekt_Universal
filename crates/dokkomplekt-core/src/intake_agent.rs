use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakeEvent {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_unix_ms: u128,
    #[serde(default)]
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntakeDecision {
    IgnoreNonDocx,
    IgnoreTemporaryOfficeFile,
    IgnoreDuplicateWithinDebounce,
    Accept,
}

#[derive(Debug, Clone)]
pub struct IntakeDeduplicator {
    debounce: Duration,
    seen: BTreeMap<PathBuf, SeenIntake>,
}

#[derive(Debug, Clone)]
struct SeenIntake {
    observed_at: SystemTime,
    size_bytes: u64,
    modified_unix_ms: u128,
    content_sha256: Option<String>,
}

impl IntakeDeduplicator {
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            seen: BTreeMap::new(),
        }
    }

    /// Prevents double UI/double popup when Windows emits several create/rename/write events for one dragged DOCX.
    pub fn decide(&mut self, path: &Path, now: SystemTime) -> IntakeDecision {
        self.decide_event(
            &IntakeEvent {
                path: path.to_path_buf(),
                size_bytes: 0,
                modified_unix_ms: 0,
                content_sha256: None,
            },
            now,
        )
    }

    /// Content-aware variant used by the background agent. A same-path rewrite
    /// is accepted immediately when its size, timestamp or SHA-256 changed, even
    /// inside the debounce window. Identical noisy events are ignored.
    pub fn decide_event(&mut self, event: &IntakeEvent, now: SystemTime) -> IntakeDecision {
        let path = &event.path;
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default();
        if name.starts_with("~$") {
            return IntakeDecision::IgnoreTemporaryOfficeFile;
        }
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(is_supported_intake_extension)
            != Some(true)
        {
            return IntakeDecision::IgnoreNonDocx;
        }
        if let Some(previous) = self.seen.get(path) {
            let same_content = previous.size_bytes == event.size_bytes
                && previous.modified_unix_ms == event.modified_unix_ms
                && previous.content_sha256 == event.content_sha256;
            if same_content
                && now.duration_since(previous.observed_at).unwrap_or_default() < self.debounce
            {
                return IntakeDecision::IgnoreDuplicateWithinDebounce;
            }
        }
        self.seen.insert(
            path.to_path_buf(),
            SeenIntake {
                observed_at: now,
                size_bytes: event.size_bytes,
                modified_unix_ms: event.modified_unix_ms,
                content_sha256: event.content_sha256.clone(),
            },
        );
        IntakeDecision::Accept
    }
}

/// Formats accepted by the universal intake router. Individual decoders can
/// still fail closed with an actionable dependency message (for example when
/// OCR or 7-Zip is not installed), but the watcher must not discard the event.
pub fn is_supported_intake_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "docx"
            | "docm"
            | "doc"
            | "ppt"
            | "pptx"
            | "pdf"
            | "jpg"
            | "jpeg"
            | "png"
            | "tif"
            | "tiff"
            | "bmp"
            | "webp"
            | "xlsx"
            | "xls"
            | "ods"
            | "odt"
            | "rtf"
            | "txt"
            | "md"
            | "csv"
            | "tsv"
            | "json"
            | "xml"
            | "html"
            | "htm"
            | "eml"
            | "msg"
            | "zip"
            | "7z"
            | "rar"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SingleInstanceDecision {
    pub should_start_ui: bool,
    pub should_raise_existing_window: bool,
    pub reason: String,
}

pub fn route_intake_event(
    app_already_running: bool,
    user_requested_ui: bool,
) -> SingleInstanceDecision {
    match (app_already_running, user_requested_ui) {
        (false, _) => SingleInstanceDecision {
            should_start_ui: true,
            should_raise_existing_window: false,
            reason: "Первый экземпляр приложения".into(),
        },
        (true, true) => SingleInstanceDecision {
            should_start_ui: false,
            should_raise_existing_window: true,
            reason: "Не запускать второй UI; поднять существующее окно".into(),
        },
        (true, false) => SingleInstanceDecision {
            should_start_ui: false,
            should_raise_existing_window: false,
            reason: "Фоновое событие обработать в существующем экземпляре".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicated_drag_event_is_ignored() {
        let mut dedup = IntakeDeduplicator::new(Duration::from_secs(3));
        let path = Path::new("C:/x/Первичный.docx");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        assert_eq!(dedup.decide(path, now), IntakeDecision::Accept);
        assert_eq!(
            dedup.decide(path, now + Duration::from_millis(200)),
            IntakeDecision::IgnoreDuplicateWithinDebounce
        );
    }

    #[test]
    fn universal_source_formats_reach_the_intake_router() {
        let mut dedup = IntakeDeduplicator::new(Duration::from_secs(3));
        for name in [
            "legacy.doc",
            "slides.ppt",
            "slides.pptx",
            "source.pdf",
            "scan.jpg",
            "table.xlsx",
            "letter.eml",
            "bundle.zip",
        ] {
            assert_eq!(
                dedup.decide(Path::new(name), SystemTime::UNIX_EPOCH),
                IntakeDecision::Accept,
                "{name}"
            );
        }
    }

    #[test]
    fn macro_enabled_word_document_is_accepted() {
        let mut dedup = IntakeDeduplicator::new(Duration::from_secs(3));
        assert_eq!(
            dedup.decide(Path::new("C:/x/source.docm"), SystemTime::UNIX_EPOCH),
            IntakeDecision::Accept
        );
    }

    #[test]
    fn same_path_rewrite_with_new_content_is_not_suppressed() {
        let mut dedup = IntakeDeduplicator::new(Duration::from_secs(3));
        let path = PathBuf::from("C:/x/source.docx");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let first = IntakeEvent {
            path: path.clone(),
            size_bytes: 100,
            modified_unix_ms: 10_000,
            content_sha256: Some("aaa".into()),
        };
        let rewritten = IntakeEvent {
            path,
            size_bytes: 100,
            modified_unix_ms: 10_000,
            content_sha256: Some("bbb".into()),
        };
        assert_eq!(dedup.decide_event(&first, now), IntakeDecision::Accept);
        assert_eq!(
            dedup.decide_event(&rewritten, now + Duration::from_millis(100)),
            IntakeDecision::Accept
        );
    }

    #[test]
    fn already_running_ui_must_not_start_second_window() {
        let d = route_intake_event(true, true);
        assert!(!d.should_start_ui);
        assert!(d.should_raise_existing_window);
    }
}
