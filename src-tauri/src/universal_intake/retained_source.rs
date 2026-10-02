use super::{safe_file_name, MAX_UPLOAD_BYTES};
#[cfg(any(target_os = "windows", test))]
use super::{materialize_sensitive_file, UploadedSourceSession};
use std::io::Write as _;
use std::path::{Path, PathBuf};

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

    pub fn byte_len(&self) -> u64 {
        self.bytes.len() as u64
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn materialize(&self, workspace: &Path) -> Result<UploadedSourceSession, String> {
        materialize_sensitive_file(&self.file_name, &self.bytes, workspace)
    }

    /// Copy the immutable uploaded source into an atomically published result set.
    /// The private stage directory is unique per generation, but create_new still
    /// protects against an accidental filename collision with a generated document.
    pub fn copy_to_directory(&self, directory: &Path, prefix: &str) -> Result<PathBuf, String> {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("Не удалось подготовить папку комплекта: {error}"))?;
        let base = safe_file_name(&format!("{prefix}{}", self.file_name));
        let base_path = Path::new(&base);
        let stem = base_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Исходный документ");
        let extension = base_path.extension().and_then(|value| value.to_str());

        for index in 1..=10_000usize {
            let name = if index == 1 {
                base.clone()
            } else if let Some(extension) = extension {
                format!("{stem} ({index}).{extension}")
            } else {
                format!("{stem} ({index})")
            };
            let target = directory.join(name);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(mut file) => {
                    file.write_all(&self.bytes).map_err(|error| {
                        format!("Не удалось сохранить исходный документ в комплект: {error}")
                    })?;
                    file.sync_all().map_err(|error| {
                        format!("Не удалось зафиксировать исходный документ на диске: {error}")
                    })?;
                    return Ok(target);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "Не удалось создать копию исходного документа в комплекте: {error}"
                    ))
                }
            }
        }
        Err("Не удалось подобрать уникальное имя для исходного документа в комплекте.".into())
    }
}

impl Drop for RetainedUploadedSource {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn immutable_uploaded_source_is_copied_with_profession_neutral_name() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-retained-source-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let source = RetainedUploadedSource::new("Договор.docx", b"source-bytes").unwrap();
        let copied = source.copy_to_directory(&root, "Исходный - ").unwrap();
        assert_eq!(
            copied.file_name().and_then(|value| value.to_str()),
            Some("Исходный - Договор.docx")
        );
        assert_eq!(std::fs::read(&copied).unwrap(), b"source-bytes");
        let _ = std::fs::remove_dir_all(root);
    }
}
