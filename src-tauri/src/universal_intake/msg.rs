use msg_parser::{Attachment, Outlook, Person};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::{
    html_to_text, is_supported_path, layout_items_from_text, normalize_path, restrict_directory_permissions,
    restrict_file_permissions, rtf_to_text, safe_file_name, NormalizedLayoutItem, NormalizedSource,
    MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_UNPACKED_BYTES, MAX_SOURCE_FILE_BYTES,
};

fn person_list(people: &[Person]) -> String {
    people
        .iter()
        .map(ToString::to_string)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn message_date(message: &Outlook) -> &str {
    [
        message.headers.date.as_str(),
        message.message_delivery_time.as_str(),
        message.client_submit_time.as_str(),
        message.creation_time.as_str(),
    ]
    .into_iter()
    .find(|value| !value.trim().is_empty())
    .unwrap_or_default()
}

fn message_body(message: &Outlook) -> String {
    if !message.body.trim().is_empty() {
        return message.body.clone();
    }
    if !message.html.trim().is_empty() {
        return html_to_text(&message.html);
    }
    if let Some(html) = message.html_from_rtf().filter(|value| !value.trim().is_empty()) {
        return html_to_text(&html);
    }
    message
        .rtf_decompressed()
        .map(|rtf| rtf_to_text(&rtf))
        .unwrap_or_default()
}

fn write_header(output: &mut String, name: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    output.push_str(name);
    output.push_str(": ");
    output.push_str(value);
    output.push('\n');
}

fn base_message_text(message: &Outlook) -> String {
    let mut text = String::new();
    write_header(&mut text, "From", &message.sender.to_string());
    write_header(&mut text, "To", &person_list(&message.to));
    write_header(&mut text, "Cc", &person_list(&message.cc));
    write_header(&mut text, "Bcc", &person_list(&message.bcc));
    write_header(&mut text, "Date", message_date(message));
    write_header(&mut text, "Subject", &message.subject);
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(message_body(message).trim());
    text
}

fn preferred_attachment_name(attachment: &Attachment, index: usize) -> String {
    let candidate = [
        attachment.long_file_name.as_str(),
        attachment.file_name.as_str(),
        attachment.display_name.as_str(),
    ]
    .into_iter()
    .find(|value| !value.trim().is_empty())
    .unwrap_or("attachment");
    let mut name = safe_file_name(candidate);
    if name.trim().is_empty() {
        name = format!("attachment-{index}");
    }
    if attachment.is_embedded_message()
        && Path::new(&name)
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case("msg"))
    {
        name.push_str(".msg");
    } else if Path::new(&name).extension().is_none() && !attachment.extension.trim().is_empty() {
        let extension = attachment.extension.trim().trim_start_matches('.');
        if !extension.is_empty()
            && extension.len() <= 16
            && extension
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            name.push('.');
            name.push_str(extension);
        }
    }
    format!("{index:03}-{name}")
}

fn prefix_attachment_layout(items: &mut [NormalizedLayoutItem], name: &str) {
    let prefix = format!("email_attachment:{name}");
    for item in items {
        item.source_reference = Some(match item.source_reference.take() {
            Some(existing) if !existing.trim().is_empty() => format!("{prefix};{existing}"),
            _ => prefix.clone(),
        });
    }
}

fn attachment_workspace(workspace: &Path) -> Result<PathBuf, String> {
    let root = workspace.join(format!("msg-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Не удалось создать временную папку MSG: {error}"))?;
    restrict_directory_permissions(&root)?;
    Ok(root)
}

fn normalize_attachments(
    message: &Outlook,
    workspace: &Path,
    depth: usize,
    text: &mut String,
    warnings: &mut Vec<String>,
    layout_items: &mut Vec<NormalizedLayoutItem>,
) -> Result<(), String> {
    if message.attachments.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "MSG содержит слишком много вложений: {} > {MAX_ARCHIVE_ENTRIES}.",
            message.attachments.len()
        ));
    }
    if message.attachments.is_empty() {
        return Ok(());
    }

    let attachment_root = attachment_workspace(workspace)?;
    let result = (|| {
        let mut total_bytes = 0_u64;
        for (offset, attachment) in message.attachments.iter().enumerate() {
            let index = offset + 1;
            let size = u64::try_from(attachment.payload_bytes.len()).unwrap_or(u64::MAX);
            if size > MAX_SOURCE_FILE_BYTES {
                return Err(format!(
                    "Вложение MSG №{index} превышает безопасный предел {} МБ.",
                    MAX_SOURCE_FILE_BYTES / (1024 * 1024)
                ));
            }
            total_bytes = total_bytes
                .checked_add(size)
                .ok_or_else(|| "Суммарный размер вложений MSG переполнен.".to_string())?;
            if total_bytes > MAX_ARCHIVE_UNPACKED_BYTES {
                return Err(format!(
                    "Вложения MSG превышают суммарный безопасный предел {} МБ.",
                    MAX_ARCHIVE_UNPACKED_BYTES / (1024 * 1024)
                ));
            }

            let file_name = preferred_attachment_name(attachment, index);
            let attachment_path = attachment_root.join(&file_name);
            std::fs::write(&attachment_path, &attachment.payload_bytes).map_err(|error| {
                format!("Не удалось материализовать вложение MSG «{file_name}»: {error}")
            })?;
            restrict_file_permissions(&attachment_path)?;

            if !is_supported_path(&attachment_path) {
                warnings.push(format!(
                    "Вложение MSG «{file_name}» имеет неподдерживаемый формат и не было разобрано."
                ));
                continue;
            }
            match normalize_path(&attachment_path, workspace, depth + 1) {
                Ok(nested) => {
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(&format!("[Вложение: {file_name}]\n{}", nested.text));
                    let mut nested_layout = nested.layout_items;
                    prefix_attachment_layout(&mut nested_layout, &file_name);
                    layout_items.extend(nested_layout);
                    warnings.extend(nested.warnings);
                }
                Err(error) => warnings.push(format!(
                    "Вложение MSG «{file_name}» не обработано: {error}"
                )),
            }
        }
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&attachment_root);
    if result.is_ok() {
        cleanup.map_err(|error| {
            format!(
                "Не удалось удалить временную папку MSG {}: {error}",
                attachment_root.display()
            )
        })?;
    }
    result
}

pub(super) fn normalize_msg(
    path: &Path,
    workspace: &Path,
    depth: usize,
) -> Result<NormalizedSource, String> {
    let message = Outlook::from_path(path)
        .map_err(|error| format!("MSG повреждён или имеет неподдерживаемую структуру: {error}"))?;
    let mut text = base_message_text(&message);
    let source_reference = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|name| format!("email:{name}"));
    let mut layout_items = layout_items_from_text(&text, None, source_reference);
    let mut warnings = Vec::new();
    normalize_attachments(
        &message,
        workspace,
        depth,
        &mut text,
        &mut warnings,
        &mut layout_items,
    )?;
    if text.trim().is_empty() {
        return Err("Из MSG не удалось получить содержательный текст или поддерживаемые вложения.".into());
    }
    Ok(NormalizedSource {
        text,
        source_kind: "email".into(),
        warnings,
        processed_files: vec![path.to_path_buf()],
        layout_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("outlook-msg")
            .join(name)
    }

    fn test_workspace() -> PathBuf {
        std::env::temp_dir().join(format!("dokkomplekt-msg-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn parses_real_outlook_ascii_fixture_without_external_converter() {
        let workspace = test_workspace();
        std::fs::create_dir_all(&workspace).unwrap();
        let result = normalize_msg(&fixture("ascii.msg"), &workspace, 0).unwrap();
        let _ = std::fs::remove_dir_all(&workspace);
        assert_eq!(result.source_kind, "email");
        assert!(!result.text.trim().is_empty());
        assert!(result.text.contains("Subject:"));
        assert!(!result.layout_items.is_empty());
    }

    #[test]
    fn malformed_msg_fails_closed() {
        let workspace = test_workspace();
        std::fs::create_dir_all(&workspace).unwrap();
        let broken = workspace.join("broken.msg");
        std::fs::write(&broken, b"not-an-ole-msg").unwrap();
        let error = normalize_msg(&broken, &workspace, 0).unwrap_err();
        let _ = std::fs::remove_dir_all(&workspace);
        assert!(error.contains("MSG повреждён") || error.contains("неподдерживаемую структуру"));
    }
}
