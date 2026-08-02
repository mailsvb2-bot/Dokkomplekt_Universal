use super::*;

pub(super) fn validate_archive_relative_path(raw: &str) -> Result<PathBuf, String> {
    let normalized = raw.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized.contains('\0')
        || normalized
            .as_bytes()
            .get(1)
            .is_some_and(|value| *value == b':')
    {
        return Err("Архив содержит абсолютный или служебный путь.".into());
    }
    let path = Path::new(&normalized);
    let invalid_component = normalized.split('/').any(|component| {
        let trimmed = component.trim_end_matches([' ', '.']);
        let base = trimmed
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let reserved = matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
            || (base.len() == 4
                && (base.starts_with("com") || base.starts_with("lpt"))
                && base.as_bytes()[3].is_ascii_digit()
                && base.as_bytes()[3] != b'0');
        component.is_empty()
            || component == "."
            || component == ".."
            || component != trimmed
            || component.chars().any(|character| {
                matches!(character, ':' | '<' | '>' | '"' | '|' | '?' | '*')
                    || character.is_control()
            })
            || reserved
    });
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) || invalid_component
    {
        return Err("Архив содержит небезопасный путь (path traversal/ADS/reparse alias).".into());
    }
    Ok(path.to_path_buf())
}

fn archive_entry_is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|value| value & 0o170000 == 0o120000)
}

pub(super) fn normalize_zip(
    path: &Path,
    workspace: &Path,
    depth: usize,
) -> Result<NormalizedSource, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("ZIP повреждён: {error}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!("В архиве больше {MAX_ARCHIVE_ENTRIES} файлов."));
    }
    let extraction = workspace.join(format!("archive-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&extraction).map_err(|error| error.to_string())?;
    let result = (|| {
        let mut total = 0_u64;
        let mut extracted = Vec::new();
        let mut seen = BTreeSet::<String>::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            if archive_entry_is_symlink(entry.unix_mode()) {
                return Err("ZIP содержит символическую ссылку; такие архивы запрещены.".into());
            }
            let relative = validate_archive_relative_path(entry.name())?;
            let key = relative.to_string_lossy().replace('\\', "/").to_lowercase();
            if !seen.insert(key) {
                return Err("ZIP содержит повторяющиеся или конфликтующие пути.".into());
            }
            let target = extraction.join(&relative);
            if entry.is_dir() {
                std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
                continue;
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| "Переполнение размера архива.".to_string())?;
            if total > MAX_ARCHIVE_UNPACKED_BYTES {
                return Err("Распакованный архив превышает 512 МБ.".into());
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|error| format!("Небезопасный или повторяющийся путь ZIP: {error}"))?;
            let remaining = MAX_ARCHIVE_UNPACKED_BYTES
                .checked_sub(total.saturating_sub(entry.size()))
                .ok_or_else(|| "Распакованный архив превышает 512 МБ.".to_string())?;
            let copied = std::io::copy(&mut (&mut entry).take(remaining + 1), &mut output)
                .map_err(|error| error.to_string())?;
            if copied > remaining {
                return Err("Фактический распакованный размер ZIP превышает 512 МБ.".into());
            }
            if copied != entry.size() {
                return Err("Размер распакованного ZIP-файла не совпал с каталогом архива.".into());
            }
            extracted.push(target);
        }
        normalize_extracted_files(path, &extraction, extracted, workspace, depth)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&extraction);
    }
    result
}

#[derive(Debug, Clone, Default)]
pub(super) struct ExternalArchiveEntry {
    path: String,
    pub(super) size: u64,
    folder: bool,
    pub(super) link_like: bool,
}

pub(super) fn parse_7z_technical_listing(
    output: &str,
) -> Result<Vec<ExternalArchiveEntry>, String>
{
    let listing = output
        .split_once("----------")
        .map_or(output, |(_, body)| body);
    let mut entries = Vec::new();
    let mut current = ExternalArchiveEntry::default();
    let flush = |current: &mut ExternalArchiveEntry, entries: &mut Vec<ExternalArchiveEntry>| {
        if !current.path.trim().is_empty() {
            entries.push(std::mem::take(current));
        }
    };
    for raw in listing.lines().chain(std::iter::once("")) {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut current, &mut entries);
            continue;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "path" => current.path = value.trim().to_string(),
            "size" => {
                current.size = value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| "7z вернул некорректный размер файла.".to_string())?;
            }
            "folder" => current.folder = value.trim() == "+",
            "symbolic link" | "hard link" | "reparse" => {
                current.link_like = !value.trim().is_empty() && value.trim() != "-";
            }
            "attributes" => {
                let lower = value.to_ascii_lowercase();
                current.link_like |= lower.starts_with('l') || lower.contains(" reparse");
            }
            _ => {}
        }
    }
    Ok(entries)
}

fn preflight_external_archive(path: &Path) -> Result<Vec<ExternalArchiveEntry>, String> {
    let output = run_command("7z", &["l", "-slt", "-ba", path.to_string_lossy().as_ref()])?;
    let listing = String::from_utf8_lossy(&output.stdout);
    let entries = parse_7z_technical_listing(&listing)?;
    if entries.is_empty() {
        return Err("7z не вернул проверяемый каталог архива.".into());
    }
    if entries.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!("В архиве больше {MAX_ARCHIVE_ENTRIES} объектов."));
    }
    let mut total = 0_u64;
    let mut seen = BTreeSet::new();
    for entry in &entries {
        let relative = validate_archive_relative_path(&entry.path)?;
        let key = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        if !seen.insert(key) {
            return Err("Архив содержит повторяющиеся или конфликтующие пути.".into());
        }
        if entry.link_like {
            return Err("Архив содержит ссылку/reparse point и не будет распакован.".into());
        }
        if !entry.folder {
            total = total
                .checked_add(entry.size)
                .ok_or_else(|| "Переполнение размера архива.".to_string())?;
            if total > MAX_ARCHIVE_UNPACKED_BYTES {
                return Err("Заявленный распакованный размер архива превышает 512 МБ.".into());
            }
        }
    }
    Ok(entries)
}

fn extract_external_archive_entry_bounded(
    archive_path: &Path,
    entry_path: &str,
    target: &Path,
    max_bytes: u64,
) -> Result<u64, String> {
    let executable = resolve_tool("7z");
    let mut command = Command::new(executable);
    command
        .args([
            "x",
            "-so",
            "-y",
            "-spd",
            "--",
            archive_path.to_string_lossy().as_ref(),
            entry_path,
        ])
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
        .map_err(|error| format!("Не найден или не запускается «7z»: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Не удалось перехватить stdout процесса «7z».".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Не удалось перехватить stderr процесса «7z».".to_string())?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let reader_exceeded = Arc::clone(&exceeded);
    let target_path = target.to_path_buf();
    let stdout_reader = std::thread::spawn(move || -> Result<u64, String> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target_path)
            .map_err(|error| error.to_string())?;
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = stdout
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| "Переполнение размера распакованного файла.".to_string())?;
            if total > max_bytes {
                reader_exceeded.store(true, Ordering::SeqCst);
                return Err("Распакованный файл превышает безопасный предел.".into());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
        output.sync_all().map_err(|error| error.to_string())?;
        Ok(total)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0_u8; 16 * 1024];
        while let Ok(count) = stderr.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let remaining = 1024 * 1024usize - kept.len().min(1024 * 1024);
            kept.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        kept
    });
    let started = Instant::now();
    let status = loop {
        if exceeded.load(Ordering::SeqCst) {
            let _ = child.kill();
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            let _ = std::fs::remove_file(target);
            return Err(format!(
                "«7z» не завершил распаковку одного файла за {} секунд.",
                COMMAND_TIMEOUT.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let extracted = stdout_reader
        .join()
        .map_err(|_| "Поток безопасной распаковки 7z аварийно завершился.".to_string())?;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| "Поток stderr 7z аварийно завершился.".to_string())?;
    if !status.success() || exceeded.load(Ordering::SeqCst) || extracted.is_err() {
        let _ = std::fs::remove_file(target);
        let detail = String::from_utf8_lossy(&stderr_bytes);
        return Err(extracted
            .err()
            .unwrap_or_else(|| format!("7z не распаковал файл безопасно: {}", detail.trim())));
    }
    extracted
}

pub(super) fn normalize_external_archive(
    path: &Path,
    workspace: &Path,
    depth: usize,
) -> Result<NormalizedSource, String> {
    let entries = preflight_external_archive(path)?;
    let extraction = workspace.join(format!("archive-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&extraction).map_err(|error| error.to_string())?;
    let result = (|| {
        let mut extracted = Vec::new();
        let mut actual_total = 0_u64;
        for entry in entries {
            let relative = validate_archive_relative_path(&entry.path)?;
            let target = extraction.join(relative);
            if entry.folder {
                std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let remaining = MAX_ARCHIVE_UNPACKED_BYTES
                .checked_sub(actual_total)
                .ok_or_else(|| "Распакованный архив превышает 512 МБ.".to_string())?;
            let actual = extract_external_archive_entry_bounded(
                path,
                &entry.path,
                &target,
                remaining.min(entry.size.saturating_add(1)),
            )?;
            if actual != entry.size {
                return Err(format!(
                    "7z сообщил один размер файла, но распаковал другой: {} ({} != {}).",
                    entry.path, actual, entry.size
                ));
            }
            actual_total = actual_total
                .checked_add(actual)
                .ok_or_else(|| "Переполнение размера архива.".to_string())?;
            extracted.push(target);
        }
        let (verified, verified_total) =
            walk_files_bounded(&extraction, MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_UNPACKED_BYTES)?;
        if verified_total != actual_total || verified.len() != extracted.len() {
            return Err("Состав распакованного архива изменился во время проверки.".into());
        }
        normalize_extracted_files(path, &extraction, verified, workspace, depth)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&extraction);
    }
    result
}

pub(super) fn prefix_layout_source(items: &mut [NormalizedLayoutItem], prefix: &str) {
    for item in items {
        item.source_reference = Some(match item.source_reference.take() {
            Some(reference) if !reference.trim().is_empty() => format!("{prefix};{reference}"),
            _ => prefix.to_string(),
        });
    }
}

fn normalize_extracted_files(
    archive_path: &Path,
    extraction: &Path,
    extracted: Vec<PathBuf>,
    workspace: &Path,
    depth: usize,
) -> Result<NormalizedSource, String> {
    let mut text = String::new();
    let mut warnings = Vec::new();
    let mut processed_files = vec![archive_path.to_path_buf()];
    let mut layout_items = Vec::new();
    for item in extracted
        .into_iter()
        .filter(|item| is_supported_path(item) && !is_temporary_source(item))
    {
        match normalize_path(&item, workspace, depth + 1) {
            Ok(nested) => {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                let label = item.strip_prefix(extraction).unwrap_or(&item).display();
                let label_text = label.to_string();
                text.push_str(&format!("[Файл из архива: {label}]\n{}", nested.text));
                let mut nested_layout = nested.layout_items;
                prefix_layout_source(&mut nested_layout, &format!("archive:{label_text}"));
                layout_items.extend(nested_layout);
                warnings.extend(nested.warnings);
                processed_files.extend(nested.processed_files);
            }
            Err(error) => warnings.push(format!("Файл «{}» пропущен: {error}", item.display())),
        }
    }
    let _ = std::fs::remove_dir_all(extraction);
    Ok(NormalizedSource {
        text,
        source_kind: "archive".into(),
        warnings,
        processed_files,
        layout_items,
    })
}
