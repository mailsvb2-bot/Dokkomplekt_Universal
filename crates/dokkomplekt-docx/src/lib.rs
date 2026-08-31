//! DOCX adapter seam.
//!
//! The domain core only knows semantic fields and template text. This crate is the only place allowed
//! to touch OpenXML/ZIP. It repairs run-split placeholders, XML-escapes substituted values, and performs
//! strict placeholder checks before writing a rendered DOCX.

mod legacy_diary_table;

use dokkomplekt_core::{
    render_docx_xml_template, render_text_template, DomainKind, LabeledTemplateValueCandidate,
    RenderResult, SemanticCase, StructuralAnchorMode,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

#[derive(Debug, Error)]
pub enum DocxError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("strict DOCX render blocked; missing={missing:?}, unknown={unknown:?}, template_errors={template_errors:?}")]
    StrictRenderBlocked {
        missing: Vec<String>,
        unknown: Vec<String>,
        template_errors: Vec<String>,
    },
    #[error("word/document.xml not found in DOCX")]
    MainDocumentPartMissing,
    #[error("DOCX size limit exceeded: {0}")]
    SizeLimit(String),
    #[error("DOCX changed while it was being rendered: {0}")]
    ArchiveChanged(String),
    #[error("cannot inject document watermark: word/document.xml has no body")]
    WatermarkBodyMissing,
    #[error("image placeholder was not found in rendered DOCX: {0}")]
    ImageMarkerMissing(String),
    #[error("unsupported image type for {0}")]
    UnsupportedImage(String),
    #[error("cannot update OOXML relationships for {0}")]
    RelationshipPart(String),
    #[error("template learning map cannot be applied safely: {0}")]
    TemplateLearningMap(String),
    #[error("structural template compilation cannot be applied safely: {0}")]
    StructuralTemplateCompilation(String),
    #[error("unsafe active or externally linked content in DOCX template: {0}")]
    UnsafeActiveContent(String),
    #[error("legacy diary table cannot be rendered safely: {0}")]
    LegacyDiaryTable(String),
}

pub type DocxResult<T> = Result<T, DocxError>;

/// Inject rendered `[[DOKKOMPLEKT_IMAGE:<field>]]` markers directly into OOXML.
///
/// This is deliberately independent of Microsoft Word and LibreOffice. Every
/// story part gets its own image relationship, while the image binary is stored
/// once under `word/media/`. The destination archive is replaced atomically only
/// after all markers and relationships have been validated.
pub fn inject_docx_images(
    document_path: &Path,
    assets: &[(String, std::path::PathBuf)],
) -> DocxResult<()> {
    if assets.is_empty() {
        return Ok(());
    }
    let input = File::open(document_path)?;
    let mut archive = ZipArchive::new(input)?;
    let mut entries = BTreeMap::<String, (Vec<u8>, zip::CompressionMethod)>::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        entries.insert(name, (bytes, entry.compression()));
    }

    let mut content_types = String::from_utf8(
        entries
            .get("[Content_Types].xml")
            .map(|(bytes, _)| bytes.clone())
            .unwrap_or_else(|| b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"></Types>".to_vec()),
    )?;
    let mut marker_counts = BTreeMap::<String, usize>::new();

    for (asset_index, (field_id, image_path)) in assets.iter().enumerate() {
        let marker = format!("[[DOKKOMPLEKT_IMAGE:{field_id}]]");
        let extension = image_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let (extension, content_type) = match extension.as_str() {
            "png" => ("png", "image/png"),
            "jpg" | "jpeg" => ("jpg", "image/jpeg"),
            "gif" => ("gif", "image/gif"),
            "bmp" => ("bmp", "image/bmp"),
            "tif" | "tiff" => ("tiff", "image/tiff"),
            _ => {
                return Err(DocxError::UnsupportedImage(
                    image_path.display().to_string(),
                ))
            }
        };
        ensure_content_type_default(&mut content_types, extension, content_type)?;
        let media_name = format!(
            "word/media/dokkomplekt-{}-{}.{}",
            sanitize_ooxml_name(field_id),
            asset_index + 1,
            extension
        );
        let image_metadata = std::fs::metadata(image_path)?;
        if image_metadata.len() > MAX_IMAGE_ASSET_BYTES {
            return Err(DocxError::SizeLimit(format!(
                "image asset {} is larger than 32 MB",
                image_path.display()
            )));
        }
        let image_bytes = std::fs::read(image_path)?;
        entries.insert(
            media_name.clone(),
            (image_bytes, zip::CompressionMethod::Deflated),
        );

        let story_names = entries
            .keys()
            .filter(|name| is_text_bearing_word_part(name))
            .cloned()
            .collect::<Vec<_>>();
        let mut field_replacements = 0_usize;
        for story_name in story_names {
            let Some((bytes, compression)) = entries.get(&story_name).cloned() else {
                continue;
            };
            let mut xml = String::from_utf8(bytes)?;
            if !xml.contains(&marker) {
                continue;
            }
            let relationship_id = unique_relationship_id(&entries, &story_name, asset_index + 1);
            let (updated, replacements) = replace_image_markers_in_story(
                &xml,
                &marker,
                &relationship_id,
                field_id,
                (asset_index + 1) * 10_000,
            )?;
            if replacements == 0 {
                continue;
            }
            xml = updated;
            entries.insert(story_name.clone(), (xml.into_bytes(), compression));
            upsert_image_relationship(&mut entries, &story_name, &relationship_id, &media_name)?;
            field_replacements += replacements;
        }
        if field_replacements == 0 {
            return Err(DocxError::ImageMarkerMissing(field_id.clone()));
        }
        marker_counts.insert(field_id.clone(), field_replacements);
    }

    if let Some((_, compression)) = entries.get("[Content_Types].xml").cloned() {
        entries.insert(
            "[Content_Types].xml".into(),
            (content_types.into_bytes(), compression),
        );
    } else {
        entries.insert(
            "[Content_Types].xml".into(),
            (content_types.into_bytes(), zip::CompressionMethod::Deflated),
        );
    }

    let temporary = temporary_output_path(document_path);
    let output = File::create(&temporary)?;
    let mut writer = ZipWriter::new(output);
    for (name, (bytes, compression)) in entries {
        let options = SimpleFileOptions::default().compression_method(compression);
        if name.ends_with('/') {
            writer.add_directory(name, options)?;
        } else {
            writer.start_file(name, options)?;
            writer.write_all(&bytes)?;
        }
    }
    writer.finish()?;
    if let Err(error) = commit_temporary_file(&temporary, document_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    debug_assert!(marker_counts.values().all(|count| *count > 0));
    Ok(())
}

fn sanitize_ooxml_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "image".into()
    } else {
        trimmed.to_string()
    }
}

fn ensure_content_type_default(
    content_types: &mut String,
    extension: &str,
    content_type: &str,
) -> DocxResult<()> {
    let marker = format!("Extension=\"{extension}\"");
    if content_types.contains(&marker) {
        return Ok(());
    }
    if let Some(closing) = content_types.rfind("</Types>") {
        content_types.insert_str(
            closing,
            &format!("<Default Extension=\"{extension}\" ContentType=\"{content_type}\"/>"),
        );
    } else if let Some(self_closing) = content_types.rfind("/>") {
        content_types.replace_range(
            self_closing..,
            &format!(
                "><Default Extension=\"{extension}\" ContentType=\"{content_type}\"/></Types>"
            ),
        );
    } else {
        return Err(DocxError::RelationshipPart("[Content_Types].xml".into()));
    }
    Ok(())
}

fn unique_relationship_id(
    entries: &BTreeMap<String, (Vec<u8>, zip::CompressionMethod)>,
    story_name: &str,
    seed: usize,
) -> String {
    let rels_name = relationship_part_name(story_name);
    let rels = entries
        .get(&rels_name)
        .map(|(bytes, _)| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default();
    let mut index = seed;
    loop {
        let candidate = format!("rIdDokkomplektImage{index}");
        if !rels.contains(&format!("Id=\"{candidate}\"")) {
            return candidate;
        }
        index += 1;
    }
}

fn relationship_part_name(story_name: &str) -> String {
    let path = Path::new(story_name);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document.xml");
    parent
        .join("_rels")
        .join(format!("{file}.rels"))
        .to_string_lossy()
        .replace('\\', "/")
}

fn upsert_image_relationship(
    entries: &mut BTreeMap<String, (Vec<u8>, zip::CompressionMethod)>,
    story_name: &str,
    relationship_id: &str,
    media_name: &str,
) -> DocxResult<()> {
    let rels_name = relationship_part_name(story_name);
    let target = media_name
        .strip_prefix("word/")
        .unwrap_or(media_name)
        .to_string();
    let relationship = format!(
        "<Relationship Id=\"{relationship_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{target}\"/>"
    );
    let (mut xml, compression) = match entries.get(&rels_name).cloned() {
        Some((bytes, compression)) => (String::from_utf8(bytes)?, compression),
        None => (
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".into(),
            zip::CompressionMethod::Deflated,
        ),
    };
    let closing = xml
        .rfind("</Relationships>")
        .ok_or_else(|| DocxError::RelationshipPart(rels_name.clone()))?;
    xml.insert_str(closing, &relationship);
    entries.insert(rels_name, (xml.into_bytes(), compression));
    Ok(())
}

fn replace_image_markers_in_story(
    xml: &str,
    marker: &str,
    relationship_id: &str,
    field_id: &str,
    document_property_seed: usize,
) -> DocxResult<(String, usize)> {
    let mut output = xml.to_string();
    let mut replacements = 0_usize;
    while let Some(marker_position) = output.find(marker) {
        let text_start = output[..marker_position]
            .rfind("<w:t")
            .ok_or_else(|| DocxError::ImageMarkerMissing(field_id.into()))?;
        let text_open_end = output[text_start..]
            .find('>')
            .map(|offset| text_start + offset + 1)
            .ok_or_else(|| DocxError::ImageMarkerMissing(field_id.into()))?;
        let text_end = output[marker_position + marker.len()..]
            .find("</w:t>")
            .map(|offset| marker_position + marker.len() + offset)
            .ok_or_else(|| DocxError::ImageMarkerMissing(field_id.into()))?;
        if marker_position < text_open_end || marker_position > text_end {
            return Err(DocxError::ImageMarkerMissing(field_id.into()));
        }
        let before = &output[text_open_end..marker_position];
        let after = &output[marker_position + marker.len()..text_end];
        let property_id = document_property_seed.saturating_add(replacements + 1);
        let escaped_field = escape_xml_attribute(field_id);
        let replacement = format!(
            "{before}</w:t></w:r><w:r><w:drawing xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"1800000\" cy=\"900000\"/><wp:docPr id=\"{property_id}\" name=\"Dokkomplekt {escaped_field}\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{property_id}\" name=\"{escaped_field}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{relationship_id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"1800000\" cy=\"900000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><w:t xml:space=\"preserve\">{after}"
        );
        output.replace_range(text_open_end..text_end, &replacement);
        replacements += 1;
    }
    Ok((output, replacements))
}

pub fn render_docx_from_text_contract(
    template_text: &str,
    case: &SemanticCase,
    strict: bool,
) -> RenderResult {
    render_text_template(template_text, case, strict)
}

pub fn extract_docx_text(path: &Path) -> DocxResult<String> {
    let file = File::open(path)?;
    extract_docx_text_from_archive(ZipArchive::new(file)?)
}

/// Extract each text-bearing Word story independently. Story boundaries are a
/// safety boundary for legacy-template inference: body text must never be
/// concatenated with headers, footers, notes, or comments before deciding what
/// old patient value owns a semantic field.
pub fn extract_docx_story_texts(path: &Path) -> DocxResult<BTreeMap<String, String>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut stories = BTreeMap::new();
    let mut found_main = false;
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        if !is_text_bearing_word_part(&name) {
            continue;
        }
        ensure_text_part_size(&name, entry.size())?;
        if name == "word/document.xml" {
            found_main = true;
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        stories.insert(name, xml_to_text(&xml));
    }
    if !found_main {
        return Err(DocxError::MainDocumentPartMissing);
    }
    Ok(stories)
}

/// Extract text directly from uploaded DOCX bytes without first trusting or
/// persisting them on disk.
pub fn extract_docx_text_from_bytes(bytes: &[u8]) -> DocxResult<String> {
    extract_docx_text_from_archive(ZipArchive::new(Cursor::new(bytes))?)
}

const MAX_DOCX_TEXT_PART_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_ASSET_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DOCX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

fn add_uncompressed_size(total: &mut u64, name: &str, size: u64) -> DocxResult<()> {
    if size > MAX_DOCX_UNCOMPRESSED_BYTES {
        return Err(DocxError::SizeLimit(format!(
            "entry {name:?} is larger than 512 MB"
        )));
    }
    *total = total
        .checked_add(size)
        .ok_or_else(|| DocxError::SizeLimit("uncompressed size overflow".into()))?;
    if *total > MAX_DOCX_UNCOMPRESSED_BYTES {
        return Err(DocxError::SizeLimit(
            "total uncompressed size is larger than 512 MB".into(),
        ));
    }
    Ok(())
}

fn ensure_text_part_size(name: &str, size: u64) -> DocxResult<()> {
    if size > MAX_DOCX_TEXT_PART_BYTES {
        return Err(DocxError::SizeLimit(format!(
            "text part {name:?} is larger than 32 MB"
        )));
    }
    Ok(())
}

fn extract_docx_text_from_archive<R: Read + Seek>(
    mut archive: ZipArchive<R>,
) -> DocxResult<String> {
    let mut parts = Vec::new();
    let mut found_main = false;
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        if !is_text_bearing_word_part(&name) {
            continue;
        }
        ensure_text_part_size(&name, entry.size())?;
        if name == "word/document.xml" {
            found_main = true;
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let text = xml_to_text(&xml);
        if !text.trim().is_empty() {
            parts.push(text);
        }
    }
    if !found_main {
        return Err(DocxError::MainDocumentPartMissing);
    }
    Ok(parts.join("\n"))
}

/// Reject macros, embedded executables/objects, ActiveX/custom UI and every
/// external OOXML relationship before a user template is persisted or rendered.
/// Copying a downloaded DOCM into app-data removes Windows Mark-of-the-Web, so
/// retaining active content would turn an untrusted upload into a trusted local
/// document.
pub fn validate_safe_template_bytes(bytes: &[u8]) -> DocxResult<()> {
    let archive = ZipArchive::new(Cursor::new(bytes))?;
    validate_safe_template_archive(archive)
}

pub fn validate_safe_template_file(path: &Path) -> DocxResult<()> {
    let archive = ZipArchive::new(File::open(path)?)?;
    validate_safe_template_archive(archive)
}

fn validate_safe_template_archive<R: Read + Seek>(mut archive: ZipArchive<R>) -> DocxResult<()> {
    let mut findings = Vec::<String>::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        let lower = name.to_ascii_lowercase();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        if lower == "word/vbaproject.bin"
            || lower.starts_with("word/activex/")
            || lower.starts_with("word/embeddings/")
            || lower.starts_with("word/ctrlprops/")
            || lower.starts_with("customui/")
        {
            findings.push(name);
            continue;
        }
        if !lower.ends_with(".rels") {
            continue;
        }
        ensure_text_part_size(&name, entry.size())?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let mut reader = Reader::from_str(&xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event() {
                Ok(Event::Empty(element)) | Ok(Event::Start(element))
                    if element.name().as_ref().ends_with(b"Relationship") =>
                {
                    let mut external = false;
                    let mut relationship_type = String::new();
                    for attribute in element.attributes().flatten() {
                        let key =
                            String::from_utf8_lossy(attribute.key.as_ref()).to_ascii_lowercase();
                        let value = String::from_utf8_lossy(attribute.value.as_ref()).to_string();
                        if key.ends_with("targetmode") && value.eq_ignore_ascii_case("external") {
                            external = true;
                        }
                        if key.ends_with("type") {
                            relationship_type = value.to_ascii_lowercase();
                        }
                    }
                    let harmless_hyperlink = relationship_type.ends_with("/hyperlink");
                    if (external && !harmless_hyperlink)
                        || relationship_type.contains("oleobject")
                        || relationship_type.contains("attachedtemplate")
                        || relationship_type.contains("control")
                    {
                        findings.push(format!("{name}: external/active relationship"));
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => {
                    return Err(DocxError::UnsafeActiveContent(format!(
                        "relationship XML {name:?} cannot be verified: {error}"
                    )))
                }
            }
        }
    }
    findings.sort();
    findings.dedup();
    if findings.is_empty() {
        Ok(())
    } else {
        Err(DocxError::UnsafeActiveContent(findings.join(", ")))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedDocxProof {
    pub render_result: RenderResult,
    /// Exact visible text derived from every rendered Word text-bearing part
    /// before the archive is published. This is the semantic publication proof;
    /// unlike reopening the finished DOCX it cannot drift from the bytes rendered
    /// in this operation because it is produced from the same in-memory OOXML.
    pub visible_text: String,
}

pub fn render_docx_file(
    template_path: &Path,
    output_path: &Path,
    case: &SemanticCase,
    strict: bool,
) -> DocxResult<RenderResult> {
    render_docx_file_with_watermark(template_path, output_path, case, strict, None)
}

pub fn render_docx_file_with_watermark(
    template_path: &Path,
    output_path: &Path,
    case: &SemanticCase,
    strict: bool,
    watermark: Option<&str>,
) -> DocxResult<RenderResult> {
    render_docx_file_with_watermark_proof(template_path, output_path, case, strict, watermark)
        .map(|proof| proof.render_result)
}

pub fn render_docx_file_with_watermark_proof(
    template_path: &Path,
    output_path: &Path,
    case: &SemanticCase,
    strict: bool,
    watermark: Option<&str>,
) -> DocxResult<RenderedDocxProof> {
    validate_safe_template_file(template_path)?;
    let input = File::open(template_path)?;
    let mut archive = ZipArchive::new(input)?;
    let mut rendered_parts = BTreeMap::<String, Vec<u8>>::new();
    let mut aggregate = RenderResult {
        output_text: String::new(),
        missing_fields: Vec::new(),
        unknown_fields: Vec::new(),
        warnings: Vec::new(),
        template_errors: Vec::new(),
    };
    let mut found_main = false;
    let mut total_uncompressed = 0_u64;
    let mut visible_parts = Vec::new();

    // Validate the whole archive and render every text-bearing part before
    // touching the destination. Large or suspicious archives fail closed.
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        if !is_text_bearing_word_part(&name) {
            continue;
        }
        ensure_text_part_size(&name, entry.size())?;
        if name == "word/document.xml" {
            found_main = true;
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let mut prepared = promote_table_row_loops(&stitch_split_placeholders(&xml));
        if name == "word/document.xml" {
            let legacy = legacy_diary_table::fill_legacy_diary_tables(&prepared, case, strict)
                .map_err(DocxError::LegacyDiaryTable)?;
            if legacy.detected_tables > 0 {
                aggregate.warnings.push(format!(
                    "legacy_diary_table:tables={},rows={},filled={},removed_after_discharge={},final_rows={}",
                    legacy.detected_tables,
                    legacy.detected_rows,
                    legacy.filled_rows,
                    legacy.removed_after_discharge,
                    legacy.final_rows
                ));
                extend_unique(&mut aggregate.warnings, legacy.warnings);
            }
            prepared = legacy.xml;
        }
        let result = render_docx_xml_template(&prepared, case, strict);
        extend_unique(&mut aggregate.missing_fields, result.missing_fields);
        extend_unique(&mut aggregate.unknown_fields, result.unknown_fields);
        extend_unique(&mut aggregate.warnings, result.warnings);
        extend_unique(&mut aggregate.template_errors, result.template_errors);
        let mut output_xml = result.output_text;
        if name == "word/document.xml" {
            if let Some(text) = watermark.map(str::trim).filter(|value| !value.is_empty()) {
                output_xml = inject_watermark_paragraph(&output_xml, text)?;
                aggregate.warnings.push("license_watermark_applied".into());
            }
            aggregate.output_text = output_xml.clone();
        }
        let visible = xml_to_text(&output_xml);
        if !visible.trim().is_empty() {
            visible_parts.push(visible);
        }
        rendered_parts.insert(name, output_xml.into_bytes());
    }

    if !found_main {
        return Err(DocxError::MainDocumentPartMissing);
    }
    if strict
        && (!aggregate.missing_fields.is_empty()
            || !aggregate.unknown_fields.is_empty()
            || !aggregate.template_errors.is_empty())
    {
        return Err(DocxError::StrictRenderBlocked {
            missing: aggregate.missing_fields,
            unknown: aggregate.unknown_fields,
            template_errors: aggregate.template_errors,
        });
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = temporary_output_path(output_path);
    let write_result = (|| -> DocxResult<()> {
        let input = File::open(template_path)?;
        let mut archive = ZipArchive::new(input)?;
        let output = File::create(&temporary)?;
        let mut writer = ZipWriter::new(output);
        let mut second_pass_total = 0_u64;

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let name = entry.name().to_string();
            add_uncompressed_size(&mut second_pass_total, &name, entry.size())?;
            let options = SimpleFileOptions::default().compression_method(entry.compression());
            if entry.is_dir() {
                writer.add_directory(name, options)?;
                continue;
            }
            writer.start_file(name.as_str(), options)?;
            if is_text_bearing_word_part(&name) {
                let bytes = rendered_parts
                    .remove(&name)
                    .ok_or_else(|| DocxError::ArchiveChanged(name.clone()))?;
                writer.write_all(&bytes)?;
            } else {
                std::io::copy(&mut entry, &mut writer)?;
            }
        }
        if let Some(name) = rendered_parts.keys().next().cloned() {
            return Err(DocxError::ArchiveChanged(name));
        }
        writer.finish()?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = commit_temporary_file(&temporary, output_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(RenderedDocxProof {
        render_result: aggregate,
        visible_text: visible_parts.join("\n"),
    })
}

fn inject_watermark_paragraph(xml: &str, watermark: &str) -> DocxResult<String> {
    let body_end = xml
        .rfind("</w:body>")
        .ok_or(DocxError::WatermarkBodyMissing)?;
    let paragraph = format!(
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="C00000"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        escape_xml_text(watermark)
    );
    let mut output = String::with_capacity(xml.len() + paragraph.len());
    output.push_str(&xml[..body_end]);
    output.push_str(&paragraph);
    output.push_str(&xml[body_end..]);
    Ok(output)
}

fn is_text_bearing_word_part(name: &str) -> bool {
    name == "word/document.xml"
        || (name.starts_with("word/header") && name.ends_with(".xml"))
        || (name.starts_with("word/footer") && name.ends_with(".xml"))
        || matches!(
            name,
            "word/footnotes.xml" | "word/endnotes.xml" | "word/comments.xml"
        )
}

fn extend_unique(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn temporary_output_path(output_path: &Path) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.docx");
    output_path.with_file_name(format!(".{file_name}.{nonce}.tmp"))
}

fn backup_output_path(output_path: &Path) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.docx");
    output_path.with_file_name(format!(".{file_name}.{nonce}.bak"))
}

fn commit_temporary_file(temporary: &Path, output_path: &Path) -> std::io::Result<()> {
    if !output_path.exists() {
        return std::fs::rename(temporary, output_path);
    }
    let backup = backup_output_path(output_path);
    std::fs::rename(output_path, &backup)?;
    match std::fs::rename(temporary, output_path) {
        Ok(()) => {
            let _ = std::fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let restore = std::fs::rename(&backup, output_path);
            let _ = std::fs::remove_file(temporary);
            match restore {
                Ok(()) => Err(error),
                Err(restore_error) => Err(std::io::Error::new(
                    error.kind(),
                    format!(
                        "output replace failed: {error}; restoring previous file failed: {restore_error}"
                    ),
                )),
            }
        }
    }
}

/// Create a minimal, valid DOCX at `output_path` whose body is `text`,
/// one paragraph per line. Placeholders (`{{field}}`) are preserved verbatim,
/// so a template pasted as plain text becomes a real, renderable DOCX file —
/// the same file later consumed by [`render_docx_file`] and the zero-touch
/// pipeline. Text is XML-escaped; `xml:space="preserve"` keeps leading and
/// trailing spaces.
pub fn create_docx_from_text(output_path: &Path, text: &str) -> DocxResult<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = String::new();
    for line in text.lines() {
        body.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
        body.push_str(&escape_xml_text(line));
        body.push_str("</w:t></w:r></w:p>");
    }
    if body.is_empty() {
        body.push_str("<w:p/>");
    }
    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
        <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
        <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
        <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
        </Types>";
    const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
        </Relationships>";
    let file = File::create(output_path)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    writer.start_file("[Content_Types].xml", options)?;
    writer.write_all(CONTENT_TYPES.as_bytes())?;
    writer.start_file("_rels/.rels", options)?;
    writer.write_all(ROOT_RELS.as_bytes())?;
    writer.start_file("word/document.xml", options)?;
    writer.write_all(document_xml.as_bytes())?;
    writer.finish()?;
    Ok(())
}

/// Insert one plain semantic paragraph into the main Word story immediately
/// before the first paragraph whose visible text starts with one of `markers`.
/// The caller owns semantic policy; the original user file is never modified.
pub fn insert_text_paragraph_before_first_matching_file(
    input_path: &Path,
    output_path: &Path,
    markers: &[&str],
    paragraph_text: &str,
) -> DocxResult<bool> {
    validate_safe_template_file(input_path)?;
    if markers.is_empty() || paragraph_text.trim().is_empty() {
        return Ok(false);
    }
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = temporary_output_path(output_path);
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let output = File::create(&temporary)?;
    let mut writer = ZipWriter::new(output);
    let mut found_main = false;
    let mut inserted = false;
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(name, options)?;
            continue;
        }
        writer.start_file(name.as_str(), options)?;
        if name != "word/document.xml" {
            std::io::copy(&mut entry, &mut writer)?;
            continue;
        }
        found_main = true;
        ensure_text_part_size(&name, entry.size())?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        if !inserted {
            let marker_folds = markers
                .iter()
                .map(|marker| structural_fold(marker))
                .collect::<Vec<_>>();
            let target = paragraph_spans(&xml).into_iter().find(|span| {
                let text = structural_fold(span.text.trim());
                marker_folds.iter().any(|marker| text.starts_with(marker))
            });
            if let Some(target) = target {
                let paragraph = format!(
                    "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    escape_xml_text(paragraph_text)
                );
                xml.insert_str(target.start, &paragraph);
                inserted = true;
            }
        }
        writer.write_all(xml.as_bytes())?;
    }
    writer.finish()?;
    if !found_main {
        let _ = std::fs::remove_file(&temporary);
        return Err(DocxError::MainDocumentPartMissing);
    }
    if !inserted {
        let _ = std::fs::remove_file(&temporary);
        return Ok(false);
    }
    if let Err(error) = commit_temporary_file(&temporary, output_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(true)
}

fn escape_xml_attribute(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_xml_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Promote a collection loop placed inside one Word table row to wrap the entire
/// `<w:tr>` element. This preserves all row formatting and lets the template
/// engine duplicate a complete row for every record.
pub fn promote_table_row_loops(xml: &str) -> String {
    let mut out = xml.to_string();
    let mut cursor = 0usize;
    while let Some(row_start) = find_opening_element(&out, cursor, "w:tr") {
        let Some(row_end) = find_matching_element_end(&out, row_start, "w:tr") else {
            break;
        };
        let row = &out[row_start..row_end];
        let Some(each_start) = row.find("{{#each ") else {
            cursor = row_end;
            continue;
        };
        let Some(each_tag_end_rel) = row[each_start..].find("}}") else {
            cursor = row_end;
            continue;
        };
        let each_tag_end = each_start + each_tag_end_rel + 2;
        let Some(each_close) = row[each_tag_end..].find("{{/each}}") else {
            cursor = row_end;
            continue;
        };
        let each_close = each_tag_end + each_close;
        let open_tag = row[each_start..each_tag_end].to_string();
        let mut clean = String::with_capacity(row.len());
        clean.push_str(&row[..each_start]);
        clean.push_str(&row[each_tag_end..each_close]);
        clean.push_str(&row[each_close + "{{/each}}".len()..]);
        let replacement = format!("{open_tag}{clean}{{{{/each}}}}");
        out.replace_range(row_start..row_end, &replacement);
        cursor = row_start + replacement.len();
    }
    out
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StructuralTemplateCompilationReport {
    pub output_path: String,
    pub applied_field_ids: Vec<String>,
    pub binding_count: usize,
}

#[derive(Debug, Clone)]
struct ParagraphSpan {
    start: usize,
    end: usize,
    text: String,
}

fn infer_structural_bindings_by_story(
    input_path: &Path,
    preferred_domain: &DomainKind,
    role_id: &str,
) -> DocxResult<BTreeMap<String, Vec<LabeledTemplateValueCandidate>>> {
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let mut bindings_by_story = BTreeMap::new();
    let mut found_main = false;
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        if !is_text_bearing_word_part(&name) {
            continue;
        }
        ensure_text_part_size(&name, entry.size())?;
        if name == "word/document.xml" {
            found_main = true;
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let story_text = xml_to_text(&xml);
        let bindings = dokkomplekt_core::infer_structural_template_values(
            &story_text,
            Some(preferred_domain),
            Some(role_id),
        );
        if !bindings.is_empty() {
            bindings_by_story.insert(name, bindings);
        }
    }
    if !found_main {
        return Err(DocxError::MainDocumentPartMissing);
    }
    Ok(bindings_by_story)
}

/// Compile an already-filled user DOCX into a semantic template by binding
/// values to their owning labels/sections. Unlike value-global replacement,
/// every edit is constrained to the concrete Word paragraph/block discovered by
/// the structural core, mirroring the proven donor editor mechanics. Inference
/// is performed independently inside every Word story so a body section can
/// never absorb text from a header, footer, note, or comment.
pub fn compile_labeled_template_file(
    input_path: &Path,
    output_path: &Path,
    preferred_domain: &DomainKind,
    role_id: &str,
) -> DocxResult<StructuralTemplateCompilationReport> {
    validate_safe_template_file(input_path)?;
    let bindings_by_story =
        infer_structural_bindings_by_story(input_path, preferred_domain, role_id)?;
    let binding_count = bindings_by_story.values().map(Vec::len).sum();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = temporary_output_path(output_path);
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let output = File::create(&temporary)?;
    let mut writer = ZipWriter::new(output);
    let mut applied_field_ids = Vec::new();
    let mut skipped = Vec::new();
    let mut total_uncompressed = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(name, options)?;
            continue;
        }
        writer.start_file(name.as_str(), options)?;
        if is_text_bearing_word_part(&name) {
            ensure_text_part_size(&name, entry.size())?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            if let Some(bindings) = bindings_by_story.get(&name) {
                for binding in bindings {
                    if let Some(next) = apply_structural_binding_in_story(&xml, binding) {
                        xml = next;
                        applied_field_ids.push(binding.field_id.clone());
                    } else {
                        skipped.push(format!("{}:{} ({})", name, binding.field_id, binding.label));
                    }
                }
            }
            writer.write_all(xml.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut writer)?;
        }
    }
    writer.finish()?;

    if !skipped.is_empty() {
        let _ = std::fs::remove_file(&temporary);
        return Err(DocxError::StructuralTemplateCompilation(format!(
            "structural anchors were detected but not rewritten: {}",
            skipped.join(", ")
        )));
    }
    if let Err(error) = commit_temporary_file(&temporary, output_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    applied_field_ids.sort();
    applied_field_ids.dedup();
    Ok(StructuralTemplateCompilationReport {
        output_path: output_path.display().to_string(),
        applied_field_ids,
        binding_count,
    })
}

fn apply_structural_binding_in_story(
    xml: &str,
    binding: &LabeledTemplateValueCandidate,
) -> Option<String> {
    let value_lines = binding
        .value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let first_value = *value_lines.first()?;
    let spans = paragraph_spans(xml);
    for (anchor_index, anchor) in spans.iter().enumerate() {
        let placeholder = format!("{{{{{}}}}}", binding.field_id);
        if binding.anchor_mode == StructuralAnchorMode::Contains {
            let folded_text = structural_fold(&anchor.text);
            if !folded_text.contains(&structural_fold(&binding.label))
                || !folded_text.contains(&structural_fold(first_value))
            {
                continue;
            }
            let paragraph = &xml[anchor.start..anchor.end];
            // Prefer the first matching value owned by the matched label. Donor
            // composite lines may repeat short values earlier (for example the
            // case number `2` inside a leading date) or later (department № 2).
            // When a donor field legitimately lives before its descriptive label
            // (name/birth date before `зарегистрирован по адресу`), fall back to
            // the historical first-visible-value replacement.
            let replaced = structural_match_end(&anchor.text, &binding.label)
                .and_then(|label_end| {
                    replace_visible_text_once_from(paragraph, first_value, &placeholder, label_end)
                })
                .or_else(|| replace_visible_text_once(paragraph, first_value, &placeholder))?;
            let mut output = xml.to_string();
            output.replace_range(anchor.start..anchor.end, &replaced);
            return Some(output);
        }

        let Some(remainder) = structural_remainder_after_label(&anchor.text, &binding.label) else {
            continue;
        };
        if anchor.text.contains("{{") || anchor.text.contains("}}") {
            continue;
        }
        let inline = !remainder.is_empty();
        if inline && structural_fold(&remainder) != structural_fold(first_value) {
            continue;
        }

        let mut edits = Vec::<(usize, usize, String)>::new();
        let mut next_value_index = 0usize;
        let mut search_index = anchor_index + 1;
        if inline {
            let paragraph = &xml[anchor.start..anchor.end];
            let replaced = replace_visible_text_once(paragraph, first_value, &placeholder)?;
            edits.push((anchor.start, anchor.end, replaced));
            next_value_index = 1;
        }

        while next_value_index < value_lines.len() {
            let expected = value_lines[next_value_index];
            let mut matched = None;
            while search_index < spans.len() {
                let span = &spans[search_index];
                search_index += 1;
                if span.text.trim().is_empty() {
                    continue;
                }
                if structural_fold(&span.text) == structural_fold(expected) {
                    matched = Some(span);
                }
                break;
            }
            let span = matched?;
            let paragraph = &xml[span.start..span.end];
            let replacement = if next_value_index == 0 {
                placeholder.as_str()
            } else {
                ""
            };
            let replaced = replace_visible_text_once(paragraph, expected, replacement)?;
            edits.push((span.start, span.end, replaced));
            next_value_index += 1;
        }

        if !inline && value_lines.is_empty() {
            continue;
        }
        edits.sort_by_key(|(start, _, _)| *start);
        let mut output = xml.to_string();
        for (start, end, replacement) in edits.into_iter().rev() {
            output.replace_range(start..end, &replacement);
        }
        return Some(output);
    }
    None
}

fn paragraph_spans(xml: &str) -> Vec<ParagraphSpan> {
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_opening_element(xml, cursor, "w:p") {
        let Some(end) = find_matching_element_end(xml, start, "w:p") else {
            break;
        };
        let text = xml_to_text(&xml[start..end]);
        spans.push(ParagraphSpan { start, end, text });
        cursor = end;
    }
    spans
}

fn structural_remainder_after_label(text: &str, label: &str) -> Option<String> {
    let text = text.trim_start();
    let wanted = label.chars().count();
    let prefix = text.chars().take(wanted).collect::<String>();
    if prefix.chars().count() != wanted || structural_fold(&prefix) != structural_fold(label) {
        return None;
    }
    let remainder = &text[prefix.len()..];
    if let Some(first) = remainder.chars().next() {
        if !(first.is_whitespace()
            || matches!(first, ':' | ';' | ',' | '.' | '-' | '–' | '—' | '№' | '('))
        {
            return None;
        }
    }
    Some(
        remainder
            .trim()
            .trim_start_matches(|character: char| {
                character.is_whitespace()
                    || matches!(character, ':' | ';' | ',' | '.' | '-' | '–' | '—' | '№')
            })
            .trim()
            .to_string(),
    )
}

fn structural_match_end(text: &str, needle: &str) -> Option<usize> {
    let wanted = structural_fold(needle);
    if wanted.is_empty() {
        return None;
    }
    let boundaries = text
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    for (start_position, start) in boundaries.iter().copied().enumerate() {
        for end in boundaries.iter().copied().skip(start_position + 1) {
            let candidate = &text[start..end];
            if structural_fold(candidate) == wanted {
                return Some(end);
            }
        }
    }
    None
}

fn structural_fold(value: &str) -> String {
    value
        .replace('\u{00a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace('ё', "е")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateMarkupAction {
    #[default]
    Replace,
    InsertAfter,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TemplateMarkupReplacement {
    pub field_id: String,
    pub value: String,
    #[serde(default)]
    pub action: TemplateMarkupAction,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TemplateMarkupReport {
    pub output_path: String,
    pub replacement_count: usize,
    pub replaced_occurrences: usize,
    pub skipped_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoryTemplateMarkupReport {
    pub output_path: String,
    pub applied_field_ids: Vec<String>,
    pub applied_binding_count: usize,
    pub replaced_occurrences: usize,
    pub skipped_bindings: Vec<String>,
}

fn unique_visible_needle(xml: &str, value: &str) -> Option<String> {
    if value.trim().is_empty() {
        return None;
    }
    let visible = text_nodes(xml)
        .iter()
        .map(|node| node.decoded.as_str())
        .collect::<String>();
    let compact = value
        .chars()
        .filter(|character| !matches!(character, '\r' | '\n' | '\t'))
        .collect::<String>();
    let mut variants = vec![value.to_string()];
    if compact != value && !compact.is_empty() {
        variants.push(compact);
    }
    variants
        .into_iter()
        .find(|needle| !needle.is_empty() && visible.match_indices(needle.as_str()).count() == 1)
}

/// Apply compatibility fallback replacements only inside the Word story that
/// produced the inference candidate. Every candidate must have exactly one
/// visible target inside that story; repeated or missing values are reported and
/// never guessed. This closes the body/header/footer cross-story ambiguity that
/// a flattened DOCX text view cannot represent safely.
pub fn apply_story_template_markup_file(
    input_path: &Path,
    output_path: &Path,
    replacements_by_story: &BTreeMap<String, Vec<TemplateMarkupReplacement>>,
) -> DocxResult<StoryTemplateMarkupReport> {
    validate_safe_template_file(input_path)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = temporary_output_path(output_path);
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let output = File::create(&temp)?;
    let mut writer = ZipWriter::new(output);
    let mut applied_fields = BTreeSet::new();
    let mut applied_binding_count = 0_usize;
    let mut replaced_occurrences = 0_usize;
    let mut skipped_bindings = Vec::new();
    let mut seen_stories = BTreeSet::new();
    let mut found_main = false;
    let mut total_uncompressed = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(name, options)?;
            continue;
        }
        writer.start_file(&name, options)?;
        if is_text_bearing_word_part(&name) {
            ensure_text_part_size(&name, entry.size())?;
            if name == "word/document.xml" {
                found_main = true;
            }
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            if let Some(replacements) = replacements_by_story.get(&name) {
                seen_stories.insert(name.clone());
                for replacement in replacements {
                    let binding = format!("{}:{}", name, replacement.field_id);
                    let Some(needle) = unique_visible_needle(&xml, &replacement.value) else {
                        skipped_bindings.push(binding);
                        continue;
                    };
                    let placeholder = format!("{{{{{}}}}}", replacement.field_id.trim());
                    let rendered = match replacement.action {
                        TemplateMarkupAction::Replace => placeholder,
                        TemplateMarkupAction::InsertAfter => {
                            format!("{}{}", replacement.value, placeholder)
                        }
                    };
                    let Some(next) = replace_visible_text_once(&xml, &needle, &rendered) else {
                        skipped_bindings.push(binding);
                        continue;
                    };
                    xml = next;
                    applied_fields.insert(replacement.field_id.clone());
                    applied_binding_count += 1;
                    replaced_occurrences += 1;
                }
            }
            writer.write_all(xml.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut writer)?;
        }
    }
    writer.finish()?;
    if !found_main {
        let _ = std::fs::remove_file(&temp);
        return Err(DocxError::MainDocumentPartMissing);
    }
    for (story, replacements) in replacements_by_story {
        if seen_stories.contains(story) {
            continue;
        }
        skipped_bindings.extend(
            replacements
                .iter()
                .map(|replacement| format!("{}:{}", story, replacement.field_id)),
        );
    }
    if let Err(error) = commit_temporary_file(&temp, output_path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(StoryTemplateMarkupReport {
        output_path: output_path.display().to_string(),
        applied_field_ids: applied_fields.into_iter().collect(),
        applied_binding_count,
        replaced_occurrences,
        skipped_bindings,
    })
}

/// Create a marked-up copy of an existing DOCX/DOCM. Only explicitly confirmed
/// values are replaced; all other ZIP parts (including `vbaProject.bin`) are copied.
pub fn apply_template_markup_file(
    input_path: &Path,
    output_path: &Path,
    replacements: &[TemplateMarkupReplacement],
) -> DocxResult<TemplateMarkupReport> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = temporary_output_path(output_path);
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let output = File::create(&temp)?;
    let mut writer = ZipWriter::new(output);
    let mut replaced = 0usize;
    let mut used = std::collections::BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(name, options)?;
            continue;
        }
        writer.start_file(&name, options)?;
        if is_text_bearing_word_part(&name) {
            ensure_text_part_size(&name, entry.size())?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            for replacement in replacements {
                if replacement.field_id.trim().is_empty() || replacement.value.trim().is_empty() {
                    continue;
                }
                let placeholder = format!("{{{{{}}}}}", replacement.field_id.trim());
                let rendered_replacement = match replacement.action {
                    TemplateMarkupAction::Replace => placeholder,
                    TemplateMarkupAction::InsertAfter => {
                        format!("{}{}", replacement.value, placeholder)
                    }
                };
                let mut local = 0usize;
                let compact_value = replacement
                    .value
                    .chars()
                    .filter(|character| !matches!(character, '\r' | '\n' | '\t'))
                    .collect::<String>();
                let needles = if replacement.action == TemplateMarkupAction::Replace
                    && compact_value != replacement.value
                    && !compact_value.is_empty()
                {
                    vec![replacement.value.as_str(), compact_value.as_str()]
                } else {
                    vec![replacement.value.as_str()]
                };
                for needle in needles {
                    while let Some(next) =
                        replace_visible_text_once(&xml, needle, &rendered_replacement)
                    {
                        xml = next;
                        local += 1;
                    }
                    if local > 0 {
                        break;
                    }
                }
                if local > 0 {
                    used.insert(replacement.field_id.clone());
                    replaced += local;
                }
            }
            writer.write_all(xml.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut writer)?;
        }
    }
    writer.finish()?;
    if let Err(error) = commit_temporary_file(&temp, output_path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    let skipped_values = replacements
        .iter()
        .filter(|r| !used.contains(&r.field_id))
        .map(|r| r.value.clone())
        .collect();
    Ok(TemplateMarkupReport {
        output_path: output_path.display().to_string(),
        replacement_count: used.len(),
        replaced_occurrences: replaced,
        skipped_values,
    })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TemplateLearningMapField {
    pub field_id: String,
    pub line_index: usize,
    pub blank_line: String,
    pub common_prefix: String,
    pub common_suffix: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TemplateLearningMapReport {
    pub output_path: String,
    pub applied_field_ids: Vec<String>,
    pub skipped_field_ids: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoryTemplateLearningMapReport {
    pub output_path: String,
    pub applied_field_ids: Vec<String>,
    pub applied_binding_count: usize,
    pub skipped_bindings: Vec<String>,
}

/// Apply inferred blank-line bindings only inside the Word story that produced
/// each candidate. A target must occur exactly once inside that story. This
/// preserves repeated headers/footers as separate structural owners and prevents
/// a blank line inferred in the document body from being written elsewhere.
pub fn apply_story_template_learning_map_file(
    input_path: &Path,
    output_path: &Path,
    fields_by_story: &BTreeMap<String, Vec<TemplateLearningMapField>>,
) -> DocxResult<StoryTemplateLearningMapReport> {
    validate_safe_template_file(input_path)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = temporary_output_path(output_path);
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let output = File::create(&temporary)?;
    let mut writer = ZipWriter::new(output);
    let mut applied_fields = BTreeSet::new();
    let mut applied_binding_count = 0_usize;
    let mut skipped_bindings = Vec::new();
    let mut seen_stories = BTreeSet::new();
    let mut found_main = false;
    let mut total_uncompressed = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        add_uncompressed_size(&mut total_uncompressed, &name, entry.size())?;
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(name, options)?;
            continue;
        }
        writer.start_file(&name, options)?;
        if is_text_bearing_word_part(&name) {
            ensure_text_part_size(&name, entry.size())?;
            if name == "word/document.xml" {
                found_main = true;
            }
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            if let Some(fields) = fields_by_story.get(&name) {
                seen_stories.insert(name.clone());
                for field in fields {
                    let field_id = field.field_id.trim();
                    let target = field.blank_line.trim();
                    let binding = format!("{}:{}", name, field_id);
                    if field_id.is_empty() || target.is_empty() {
                        skipped_bindings.push(binding);
                        continue;
                    }
                    let Some(needle) = unique_visible_needle(&xml, target) else {
                        skipped_bindings.push(binding);
                        continue;
                    };
                    let replacement = format!(
                        "{}{{{{{}}}}}{}",
                        field.common_prefix, field_id, field.common_suffix
                    );
                    let Some(next) = replace_visible_text_once(&xml, &needle, &replacement) else {
                        skipped_bindings.push(binding);
                        continue;
                    };
                    xml = next;
                    applied_fields.insert(field_id.to_string());
                    applied_binding_count += 1;
                }
            }
            writer.write_all(xml.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut writer)?;
        }
    }
    writer.finish()?;
    if !found_main {
        let _ = std::fs::remove_file(&temporary);
        return Err(DocxError::MainDocumentPartMissing);
    }
    for (story, fields) in fields_by_story {
        if seen_stories.contains(story) {
            continue;
        }
        skipped_bindings.extend(
            fields
                .iter()
                .map(|field| format!("{}:{}", story, field.field_id.trim())),
        );
    }
    if let Err(error) = commit_temporary_file(&temporary, output_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(StoryTemplateLearningMapReport {
        output_path: output_path.display().to_string(),
        applied_field_ids: applied_fields.into_iter().collect(),
        applied_binding_count,
        skipped_bindings,
    })
}

/// Apply an explicitly confirmed field map inferred from several filled
/// examples. The operation only replaces the exact visible blank-template line
/// and copies every other OOXML part byte-for-byte. Ambiguous duplicate targets
/// are rejected rather than guessed.
pub fn apply_template_learning_map_file(
    input_path: &Path,
    output_path: &Path,
    fields: &[TemplateLearningMapField],
) -> DocxResult<TemplateLearningMapReport> {
    if fields.is_empty() {
        return Err(DocxError::TemplateLearningMap(
            "no confirmed fields were supplied".into(),
        ));
    }
    let mut target_owner = BTreeMap::<String, String>::new();
    for field in fields {
        let field_id = field.field_id.trim();
        let target = field.blank_line.trim();
        if field_id.is_empty() || target.is_empty() {
            continue;
        }
        if let Some(previous) = target_owner.insert(target.to_string(), field_id.to_string()) {
            if previous != field_id {
                return Err(DocxError::TemplateLearningMap(format!(
                    "the same blank line is mapped to both {previous} and {field_id}; mark it manually"
                )));
            }
        }
    }
    if target_owner.is_empty() {
        return Err(DocxError::TemplateLearningMap(
            "all confirmed fields have empty or conditional blank lines".into(),
        ));
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = temporary_output_path(output_path);
    let input = File::open(input_path)?;
    let mut archive = ZipArchive::new(input)?;
    let output = File::create(&temporary)?;
    let mut writer = ZipWriter::new(output);
    let mut applied_counts = BTreeMap::<String, usize>::new();
    let mut warnings = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(name, options)?;
            continue;
        }
        writer.start_file(&name, options)?;
        if is_text_bearing_word_part(&name) {
            ensure_text_part_size(&name, entry.size())?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            for field in fields {
                let field_id = field.field_id.trim();
                let target = field.blank_line.trim();
                if field_id.is_empty() || target.is_empty() {
                    continue;
                }
                let replacement = format!(
                    "{}{{{{{}}}}}{}",
                    field.common_prefix, field_id, field.common_suffix
                );
                if let Some(next) = replace_visible_text_once(&xml, target, &replacement) {
                    xml = next;
                    *applied_counts.entry(field_id.to_string()).or_default() += 1;
                }
            }
            writer.write_all(xml.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut writer)?;
        }
    }
    writer.finish()?;

    let mut applied_field_ids = Vec::new();
    let mut skipped_field_ids = Vec::new();
    for field in fields {
        let field_id = field.field_id.trim().to_string();
        if field_id.is_empty() {
            continue;
        }
        match applied_counts.get(&field_id).copied().unwrap_or_default() {
            0 => skipped_field_ids.push(field_id),
            1 => applied_field_ids.push(field_id),
            count => {
                warnings.push(format!(
                    "field {field_id} was inserted {count} times; verify the visual diff"
                ));
                applied_field_ids.push(field_id);
            }
        }
    }
    applied_field_ids.sort();
    applied_field_ids.dedup();
    skipped_field_ids.sort();
    skipped_field_ids.dedup();
    if applied_field_ids.is_empty() {
        let _ = std::fs::remove_file(&temporary);
        return Err(DocxError::TemplateLearningMap(
            "none of the confirmed blank lines was found in the DOCX".into(),
        ));
    }
    if !skipped_field_ids.is_empty() {
        warnings.push(format!(
            "some fields were not written and require manual markup: {}",
            skipped_field_ids.join(", ")
        ));
    }
    if let Err(error) = commit_temporary_file(&temporary, output_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(TemplateLearningMapReport {
        output_path: output_path.display().to_string(),
        applied_field_ids,
        skipped_field_ids,
        warnings,
    })
}

fn is_xml_name_boundary(byte: Option<u8>) -> bool {
    matches!(
        byte,
        None | Some(b'>') | Some(b'/') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
    )
}

fn find_opening_element(xml: &str, mut from: usize, name: &str) -> Option<usize> {
    let marker = format!("<{name}");
    while let Some(relative) = xml.get(from..)?.find(&marker) {
        let start = from + relative;
        if is_xml_name_boundary(xml.as_bytes().get(start + marker.len()).copied()) {
            return Some(start);
        }
        from = start + marker.len();
    }
    None
}

fn find_closing_element(xml: &str, mut from: usize, name: &str) -> Option<(usize, usize)> {
    let marker = format!("</{name}");
    while let Some(relative) = xml.get(from..)?.find(&marker) {
        let start = from + relative;
        if is_xml_name_boundary(xml.as_bytes().get(start + marker.len()).copied()) {
            let end = start + xml.get(start..)?.find('>')? + 1;
            return Some((start, end));
        }
        from = start + marker.len();
    }
    None
}

fn find_matching_element_end(xml: &str, start: usize, name: &str) -> Option<usize> {
    let opening_end = start + xml.get(start..)?.find('>')? + 1;
    if xml.get(start..opening_end)?.trim_end().ends_with("/>") {
        return Some(opening_end);
    }
    let mut depth = 1usize;
    let mut cursor = opening_end;
    while depth > 0 {
        let next_open = find_opening_element(xml, cursor, name);
        let next_close = find_closing_element(xml, cursor, name);
        match (next_open, next_close) {
            (_, None) => return None,
            (Some(open), Some((close, _close_end))) if open < close => {
                let open_end = open + xml.get(open..)?.find('>')? + 1;
                if !xml.get(open..open_end)?.trim_end().ends_with("/>") {
                    depth += 1;
                }
                cursor = open_end;
            }
            (_, Some((_close, close_end))) => {
                depth -= 1;
                cursor = close_end;
            }
        }
    }
    Some(cursor)
}

#[derive(Clone)]
struct TextNode {
    content_start: usize,
    content_end: usize,
    decoded: String,
}
fn text_nodes(xml: &str) -> Vec<TextNode> {
    let mut nodes = Vec::new();
    let mut pos = 0;
    while let Some(tag) = find_opening_element(xml, pos, "w:t") {
        let Some(gt_rel) = xml[tag..].find('>') else {
            break;
        };
        let start = tag + gt_rel + 1;
        let Some((end, close_end)) = find_closing_element(xml, start, "w:t") else {
            break;
        };
        nodes.push(TextNode {
            content_start: start,
            content_end: end,
            decoded: decode_xml_entities(&xml[start..end]),
        });
        pos = close_end;
    }
    nodes
}
fn replace_visible_text_once(xml: &str, needle: &str, replacement: &str) -> Option<String> {
    replace_visible_text_once_from(xml, needle, replacement, 0)
}

fn replace_visible_text_once_from(
    xml: &str,
    needle: &str,
    replacement: &str,
    visible_start: usize,
) -> Option<String> {
    if needle.is_empty() {
        return None;
    }
    let nodes = text_nodes(xml);
    if nodes.is_empty() {
        return None;
    }
    let visible = nodes.iter().map(|n| n.decoded.as_str()).collect::<String>();
    if visible_start > visible.len() || !visible.is_char_boundary(visible_start) {
        return None;
    }
    let start_byte = visible_start + visible[visible_start..].find(needle)?;
    let end_byte = start_byte + needle.len();
    if !visible.is_char_boundary(start_byte) || !visible.is_char_boundary(end_byte) {
        return None;
    }
    let mut offset = 0usize;
    let mut start_node = None;
    let mut end_node = None;
    let mut local_start = 0;
    let mut local_end = 0;
    for (i, n) in nodes.iter().enumerate() {
        let next = offset + n.decoded.len();
        if start_node.is_none() && start_byte >= offset && start_byte <= next {
            start_node = Some(i);
            local_start = start_byte - offset;
        }
        if end_byte >= offset && end_byte <= next {
            end_node = Some(i);
            local_end = end_byte - offset;
            break;
        }
        offset = next;
    }
    let (si, ei) = (start_node?, end_node?);
    if !nodes[si].decoded.is_char_boundary(local_start)
        || !nodes[ei].decoded.is_char_boundary(local_end)
    {
        return None;
    }
    let mut edits = Vec::<(usize, usize, String)>::new();
    if si == ei {
        let n = &nodes[si];
        let new = format!(
            "{}{}{}",
            &n.decoded[..local_start],
            replacement,
            &n.decoded[local_end..]
        );
        edits.push((n.content_start, n.content_end, escape_xml_text(&new)));
    } else {
        let first = &nodes[si];
        let last = &nodes[ei];
        let new_first = format!("{}{}", &first.decoded[..local_start], replacement);
        edits.push((
            first.content_start,
            first.content_end,
            escape_xml_text(&new_first),
        ));
        for n in &nodes[si + 1..ei] {
            edits.push((n.content_start, n.content_end, String::new()));
        }
        edits.push((
            last.content_start,
            last.content_end,
            escape_xml_text(&last.decoded[local_end..]),
        ));
    }
    let mut out = xml.to_string();
    for (a, b, v) in edits.into_iter().rev() {
        out.replace_range(a..b, &v)
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Run-split placeholder repair
// ---------------------------------------------------------------------------

/// Word stores a placeholder like `{{subject.name}}` across several runs
/// (`<w:t>{{sub</w:t>...<w:t>ject.name}}</w:t>`), and can even split the
/// `{{`/`}}` markers themselves. This removes the OpenXML tags that fall *inside*
/// a placeholder token so substitution can find it. The removed run boundaries are
/// balanced markup, so the resulting XML stays well-formed; text and formatting
/// outside placeholders are untouched.
pub fn stitch_split_placeholders(xml: &str) -> String {
    let joined_open = join_split_marker(xml, '{');
    let joined = join_split_marker(&joined_open, '}');
    strip_tags_inside_placeholders(&joined)
}

/// Collapse a marker char that Word split across runs: `{ <tags> {` -> `{{`.
/// Only collapses when the gap between the two marker chars is pure markup and
/// whitespace, so ordinary text such as `{ a {` is never touched.
fn join_split_marker(xml: &str, marker: char) -> String {
    let chars: Vec<char> = xml.chars().collect();
    let mut out = String::with_capacity(xml.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == marker {
            if let Some(j) = marker_after_markup(&chars, i + 1, marker) {
                out.push(marker);
                out.push(marker);
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// If, starting at `from`, the sequence is `(<...> | whitespace)*` followed by
/// `marker`, and at least one tag was crossed, return the index of that marker.
fn marker_after_markup(chars: &[char], from: usize, marker: char) -> Option<usize> {
    let mut k = from;
    let mut saw_tag = false;
    while k < chars.len() {
        let c = chars[k];
        if c == '<' {
            saw_tag = true;
            while k < chars.len() && chars[k] != '>' {
                k += 1;
            }
            if k >= chars.len() {
                return None;
            }
            k += 1;
        } else if c.is_whitespace() {
            k += 1;
        } else if c == marker {
            return if saw_tag { Some(k) } else { None };
        } else {
            return None;
        }
    }
    None
}

/// Remove OpenXML tags that appear between `{{` and the matching `}}`.
fn strip_tags_inside_placeholders(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                // A stray `{{` before the close means this open is spurious; re-scan from it.
                if after[..end].contains("{{") {
                    out.push_str("{{");
                    rest = after;
                } else {
                    out.push_str("{{");
                    out.push_str(remove_xml_tags(&after[..end]).trim());
                    out.push_str("}}");
                    rest = &after[end + 2..];
                }
            }
            None => {
                out.push_str("{{");
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn remove_xml_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Text extraction
// ---------------------------------------------------------------------------

fn xml_to_text(xml: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out = String::new();
    let mut table_cell_depth = 0usize;
    let mut table_row_depth = 0usize;
    let mut ignored_depth = 0usize;
    let mut run_depth = 0usize;
    let mut hidden_run_depth = 0usize;
    let mut visible_text_depth = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let event_name = event.name();
                let name = xml_local_name(event_name.as_ref());
                if ignored_depth > 0 {
                    ignored_depth += 1;
                    continue;
                }
                if matches!(name, b"del" | b"moveFrom" | b"customXmlDelRangeStart") {
                    ignored_depth = 1;
                    continue;
                }
                match name {
                    b"r" => run_depth += 1,
                    b"vanish" | b"webHidden" if run_depth > 0 => hidden_run_depth = run_depth,
                    b"tr" => table_row_depth += 1,
                    b"tc" => table_cell_depth += 1,
                    b"t" if hidden_run_depth == 0 => visible_text_depth += 1,
                    b"instrText" | b"delText" => ignored_depth = 1,
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                let event_name = event.name();
                let name = xml_local_name(event_name.as_ref());
                if ignored_depth > 0 {
                    continue;
                }
                match name {
                    b"vanish" | b"webHidden" if run_depth > 0 => hidden_run_depth = run_depth,
                    b"br" | b"cr" => {
                        if table_cell_depth > 0 {
                            push_separator(&mut out, ' ');
                        } else {
                            push_separator(&mut out, '\n');
                        }
                    }
                    b"tab" => push_separator(&mut out, '\t'),
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                let event_name = event.name();
                let name = xml_local_name(event_name.as_ref());
                if ignored_depth > 0 {
                    ignored_depth -= 1;
                    continue;
                }
                match name {
                    b"t" => visible_text_depth = visible_text_depth.saturating_sub(1),
                    b"r" => {
                        if hidden_run_depth == run_depth {
                            hidden_run_depth = 0;
                        }
                        run_depth = run_depth.saturating_sub(1);
                    }
                    b"tr" => {
                        table_row_depth = table_row_depth.saturating_sub(1);
                        trim_trailing_table_separators(&mut out);
                        push_separator(&mut out, '\n');
                    }
                    b"tc" => {
                        table_cell_depth = table_cell_depth.saturating_sub(1);
                        trim_trailing_spaces(&mut out);
                        push_separator(&mut out, '\t');
                    }
                    b"p" => {
                        if table_cell_depth > 0 || table_row_depth > 0 {
                            push_separator(&mut out, ' ');
                        } else {
                            push_separator(&mut out, '\n');
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(event))
                if ignored_depth == 0 && visible_text_depth > 0 && hidden_run_depth == 0 =>
            {
                if let Ok(text) = event.decode() {
                    out.push_str(&text);
                }
            }
            Ok(Event::GeneralRef(event))
                if ignored_depth == 0 && visible_text_depth > 0 && hidden_run_depth == 0 =>
            {
                if let Ok(Some(character)) = event.resolve_char_ref() {
                    out.push(character);
                } else if let Ok(name) = event.decode() {
                    if let Some(value) = quick_xml::escape::resolve_predefined_entity(&name) {
                        out.push_str(value);
                    }
                }
            }
            Ok(Event::CData(event))
                if ignored_depth == 0 && visible_text_depth > 0 && hidden_run_depth == 0 =>
            {
                out.push_str(&String::from_utf8_lossy(event.as_ref()));
            }
            Ok(Event::Eof) => break,
            Err(_) => return String::new(),
            _ => {}
        }
    }

    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn xml_local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn push_separator(out: &mut String, separator: char) {
    if out.ends_with(separator) {
        return;
    }
    if separator == ' ' && out.ends_with(['\n', '\t', ' ']) {
        return;
    }
    out.push(separator);
}

fn trim_trailing_spaces(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
}

fn trim_trailing_table_separators(out: &mut String) {
    while out.ends_with([' ', '\t']) {
        out.pop();
    }
}

fn decode_xml_entities(input: &str) -> String {
    let numeric = decode_numeric_entities(input);
    numeric
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Decode numeric character references `&#123;` and `&#x1F;` that Word emits.
fn decode_numeric_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find("&#") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        if let Some(semi) = after.find(';') {
            let body = &after[..semi];
            let parsed = if let Some(hex) = body.strip_prefix(['x', 'X']) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                body.parse::<u32>().ok()
            };
            match parsed.and_then(char::from_u32) {
                Some(ch) => {
                    out.push(ch);
                    rest = &after[semi + 1..];
                }
                None => {
                    out.push_str("&#");
                    rest = after;
                }
            }
        } else {
            out.push_str("&#");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dokkomplekt_core::{SemanticValue, ValueSource};
    use std::collections::BTreeMap;

    fn build_test_docx(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut writer = ZipWriter::new(cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer
                .start_file(*name, options)
                .expect("start test ZIP entry");
            writer.write_all(bytes).expect("write test ZIP entry");
        }
        writer.finish().expect("finish test DOCX").into_inner()
    }

    fn case_with(pairs: &[(&str, &str)]) -> SemanticCase {
        let mut values = BTreeMap::new();
        for (k, v) in pairs {
            values.insert(
                (*k).to_string(),
                SemanticValue {
                    field_id: (*k).to_string(),
                    value: (*v).to_string(),
                    source: ValueSource::UserConfirmed,
                    confidence: 1.0,
                    evidence: Vec::new(),
                },
            );
        }
        SemanticCase {
            values,
            active_domains: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn xml_to_text_preserves_paragraph_breaks() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>А</w:t></w:r></w:p><w:p><w:r><w:t>Б</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(xml_to_text(xml), "А\nБ");
    }

    #[test]
    fn xml_to_text_ignores_deleted_field_instruction_and_hidden_text() {
        let xml = r#"<w:document><w:body>
          <w:p><w:r><w:t>Актуально</w:t></w:r></w:p>
          <w:del><w:r><w:delText>Старая дата 01.01.2020</w:delText></w:r></w:del>
          <w:p><w:r><w:instrText>MERGEFIELD secret</w:instrText></w:r><w:r><w:t>Видимый результат</w:t></w:r></w:p>
          <w:p><w:r><w:rPr><w:vanish/></w:rPr><w:t>Скрыто</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let text = xml_to_text(xml);
        assert!(text.contains("Актуально"));
        assert!(text.contains("Видимый результат"));
        assert!(!text.contains("Старая дата"));
        assert!(!text.contains("MERGEFIELD"));
        assert!(!text.contains("Скрыто"));
    }

    #[test]
    fn xml_to_text_preserves_table_rows_and_cells() {
        let xml = r#"<w:document><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Наименование</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Количество</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Цена</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Аудит</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1500,00</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        assert_eq!(
            xml_to_text(xml),
            "Наименование\tКоличество\tЦена\nАудит\t2\t1500,00"
        );
    }

    #[test]
    fn extracted_word_table_becomes_items_collection() {
        let xml = r#"<w:document><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Наименование</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Количество</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Сумма</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Монтаж</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>3</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>4500,00</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let text = xml_to_text(xml);
        let (case, report) = dokkomplekt_core::parse_source_text(&text, 2026);
        let items = case.collection("items").expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("name").map(|value| value.as_text()).as_deref(),
            Some("Монтаж")
        );
        assert!(report
            .filled_fields
            .iter()
            .any(|field| field == "collection.items"));
    }

    #[test]
    fn stitches_field_split_across_runs() {
        let xml = "<w:t>Пациент {{sub</w:t></w:r><w:r><w:t>ject.name}}</w:t>";
        let stitched = stitch_split_placeholders(xml);
        assert!(stitched.contains("{{subject.name}}"));
    }

    #[test]
    fn stitches_split_markers() {
        assert!(
            stitch_split_placeholders("<w:t>{</w:t><w:t>{org.name}}</w:t>")
                .contains("{{org.name}}")
        );
        assert!(
            stitch_split_placeholders("<w:t>{{org.name}</w:t><w:t>}</w:t>")
                .contains("{{org.name}}")
        );
    }

    #[test]
    fn does_not_collapse_plain_braces_with_text_between() {
        assert_eq!(
            stitch_split_placeholders("<w:t>{ a {</w:t>"),
            "<w:t>{ a {</w:t>"
        );
    }

    #[test]
    fn split_placeholder_renders_escaped_value() {
        let xml = "<w:t>{{org.</w:t><w:t>name}}</w:t>";
        let stitched = stitch_split_placeholders(xml);
        let r = render_docx_xml_template(&stitched, &case_with(&[("org.name", "A & B")]), true);
        assert!(r.missing_fields.is_empty());
        assert!(r.output_text.contains("A &amp; B"));
        assert!(!r.output_text.contains("A & B"));
    }

    #[test]
    fn create_docx_from_text_round_trips_and_renders() {
        let dir = std::env::temp_dir().join("dokkomplekt-docx-test");
        let tpl = dir.join("from_text.docx");
        let out = dir.join("rendered.docx");
        let text =
            "Счёт на оплату № {{document.number}}\nПоставщик: {{org.name}} & Ко <спецсимволы>";
        create_docx_from_text(&tpl, text).expect("create docx from text");
        // Извлечённый текст совпадает с исходным (плейсхолдеры и спецсимволы целы).
        assert_eq!(extract_docx_text(&tpl).expect("extract"), text);
        // Тот же файл проходит строгий рендер настоящим пайплайном.
        let case = case_with(&[("document.number", "148"), ("org.name", "ООО «Ромашка»")]);
        let result = render_docx_file(&tpl, &out, &case, true).expect("render");
        assert!(result.missing_fields.is_empty());
        let rendered = extract_docx_text(&out).expect("extract rendered");
        assert!(rendered.contains("№ 148"));
        assert!(rendered.contains("ООО «Ромашка» & Ко <спецсимволы>"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn read_zip_text(archive: &mut ZipArchive<File>, name: &str) -> String {
        let mut entry = archive.by_name(name).expect("required OOXML part");
        let mut text = String::new();
        entry.read_to_string(&mut text).expect("read OOXML part");
        text
    }

    fn write_test_docx(path: &Path, body: &str, header: Option<&str>) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create test dir");
        }
        let file = File::create(path).expect("create test docx");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer.write_all(body.as_bytes()).expect("document bytes");
        if let Some(header) = header {
            writer
                .start_file("word/header1.xml", options)
                .expect("header");
            writer.write_all(header.as_bytes()).expect("header bytes");
        }
        writer.finish().expect("finish test docx");
    }

    #[test]
    fn filled_multiline_value_can_be_replaced_by_one_semantic_placeholder() {
        let dir =
            std::env::temp_dir().join(format!("dokkomplekt-filled-markup-{}", std::process::id()));
        let input = dir.join("filled.docx");
        let marked = dir.join("marked.docx");
        write_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Лечение: терапия 1</w:t></w:r></w:p><w:p><w:r><w:t>терапия 2</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let report = apply_template_markup_file(
            &input,
            &marked,
            &[TemplateMarkupReplacement {
                field_id: "medical.treatment".into(),
                value: "терапия 1\nтерапия 2".into(),
                action: TemplateMarkupAction::Replace,
            }],
        )
        .expect("filled multiline markup");
        assert_eq!(report.replacement_count, 1);
        assert!(report.skipped_values.is_empty());
        let text = extract_docx_text(&marked).expect("marked text");
        assert!(text.contains("{{medical.treatment}}"), "{text:?}");
        assert!(!text.contains("терапия 1"));
        assert!(!text.contains("терапия 2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn story_scoped_blank_binding_never_writes_same_target_in_header() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-story-blank-header-{}",
            std::process::id()
        ));
        let input = dir.join("blank.docx");
        let marked = dir.join("marked.docx");
        write_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Жалобы:</w:t></w:r></w:p><w:p><w:r><w:t>________</w:t></w:r></w:p></w:body></w:document>"#,
            Some(
                r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>________</w:t></w:r></w:p></w:hdr>"#,
            ),
        );
        let fields = BTreeMap::from([(
            "word/document.xml".to_string(),
            vec![TemplateLearningMapField {
                field_id: "medical.complaints".into(),
                line_index: 1,
                blank_line: "________".into(),
                common_prefix: String::new(),
                common_suffix: String::new(),
            }],
        )]);
        let report = apply_story_template_learning_map_file(&input, &marked, &fields)
            .expect("story-scoped blank markup");
        assert_eq!(report.applied_binding_count, 1);
        assert!(report.skipped_bindings.is_empty());
        let stories = extract_docx_story_texts(&marked).expect("story texts");
        assert!(stories["word/document.xml"].contains("{{medical.complaints}}"));
        assert_eq!(stories["word/header1.xml"].trim(), "________");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn story_scoped_fallback_never_crosses_into_header_with_same_literal() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-story-fallback-header-{}",
            std::process::id()
        ));
        let input = dir.join("filled.docx");
        let marked = dir.join("marked.docx");
        write_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Лечение: старая схема</w:t></w:r></w:p></w:body></w:document>"#,
            Some(
                r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>старая схема</w:t></w:r></w:p></w:hdr>"#,
            ),
        );
        let replacements = BTreeMap::from([(
            "word/document.xml".to_string(),
            vec![TemplateMarkupReplacement {
                field_id: "medical.treatment".into(),
                value: "старая схема".into(),
                action: TemplateMarkupAction::Replace,
            }],
        )]);
        let report = apply_story_template_markup_file(&input, &marked, &replacements)
            .expect("story-scoped markup");
        assert_eq!(report.applied_binding_count, 1);
        assert!(report.skipped_bindings.is_empty());
        let stories = extract_docx_story_texts(&marked).expect("story texts");
        assert!(stories["word/document.xml"].contains("{{medical.treatment}}"));
        assert!(stories["word/header1.xml"].contains("старая схема"));
        assert!(!stories["word/header1.xml"].contains("{{medical.treatment}}"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn story_scoped_fallback_rejects_repeated_literal_inside_owner_story() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-story-fallback-duplicate-{}",
            std::process::id()
        ));
        let input = dir.join("filled.docx");
        let marked = dir.join("marked.docx");
        write_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>42</w:t></w:r></w:p><w:p><w:r><w:t>42</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let replacements = BTreeMap::from([(
            "word/document.xml".to_string(),
            vec![TemplateMarkupReplacement {
                field_id: "subject.age".into(),
                value: "42".into(),
                action: TemplateMarkupAction::Replace,
            }],
        )]);
        let report = apply_story_template_markup_file(&input, &marked, &replacements)
            .expect("ambiguous story markup report");
        assert_eq!(report.applied_binding_count, 0);
        assert_eq!(
            report.skipped_bindings,
            vec!["word/document.xml:subject.age"]
        );
        let text = extract_docx_text(&marked).expect("marked text");
        assert_eq!(text.matches("42").count(), 2);
        assert!(!text.contains("{{subject.age}}"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn partially_dynamic_filled_medical_template_compiles_by_structure_and_renders_new_case() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-partial-medical-compile-{}",
            std::process::id()
        ));
        let input = dir.join("filled.docx");
        let compiled = dir.join("compiled.docx");
        let rendered = dir.join("rendered.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Выписной эпикриз</w:t></w:r></w:p>
<w:p><w:r><w:t>Ф.И.О.: Иванов Иван Иванович</w:t></w:r></w:p>
<w:p><w:r><w:t>Номер истории болезни: АБ-4213/26</w:t></w:r></w:p>
<w:p><w:r><w:t>Дата поступления: 01.09.2026</w:t></w:r></w:p>
<w:p><w:r><w:t>Диагноз: F20 — авторская формулировка</w:t></w:r></w:p>
<w:p><w:r><w:t>Дата выписки: 09.09.2026</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечение:</w:t></w:r></w:p>
<w:p><w:r><w:t>Старая схема первая строка</w:t></w:r></w:p>
<w:p><w:r><w:t>Старая схема вторая строка</w:t></w:r></w:p>
<w:p><w:r><w:t>Место работы: Завод</w:t></w:r></w:p>
<w:p><w:r><w:t>Должность: инженер</w:t></w:r></w:p>
<w:p><w:r><w:t>Состояние при выписке: {{medical.discharge_condition}}</w:t></w:r></w:p>
<w:p><w:r><w:t>Зав. отделением Петров П.П.</w:t></w:r></w:p>
<w:p><w:r><w:t>Врач-психиатр Иванов И.И.</w:t></w:r></w:p>
</w:body></w:document>"#;
        write_test_docx(&input, body, None);

        let report =
            compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
                .expect("donor-style structural compilation");
        for field_id in [
            "subject.name",
            "medical.case_number",
            "medical.admission_date",
            "medical.diagnosis",
            "medical.discharge_date",
            "medical.treatment",
            "medical.workplace",
            "medical.position",
        ] {
            assert!(
                report.applied_field_ids.iter().any(|item| item == field_id),
                "missing structural binding {field_id}: {report:?}"
            );
        }
        let compiled_text = extract_docx_text(&compiled).expect("compiled text");
        assert!(compiled_text.contains("{{subject.name}}"));
        assert!(compiled_text.contains("{{medical.treatment}}"));
        assert!(compiled_text.contains("{{medical.discharge_condition}}"));
        assert!(!compiled_text.contains("Старая схема первая строка"));
        assert!(!compiled_text.contains("Старая схема вторая строка"));

        let case = case_with(&[
            ("subject.name", "Петров Пётр Петрович"),
            ("medical.case_number", "9876"),
            ("medical.admission_date", "02.10.2026"),
            ("medical.diagnosis", "F21"),
            ("medical.discharge_date", "11.10.2026"),
            ("medical.treatment", "новая терапия"),
            ("medical.workplace", "Фабрика"),
            ("medical.position", "мастер"),
            ("medical.discharge_condition", "улучшение"),
        ]);
        let proof = render_docx_file_with_watermark_proof(&compiled, &rendered, &case, true, None)
            .expect("compiled medical template renders strict");
        for expected in [
            "Петров Пётр Петрович",
            "9876",
            "02.10.2026",
            "F21",
            "11.10.2026",
            "новая терапия",
            "Фабрика",
            "мастер",
            "улучшение",
        ] {
            assert!(
                proof.visible_text.contains(expected),
                "missing {expected:?} in {:?}",
                proof.visible_text
            );
        }
        for stale in [
            "Иванов Иван Иванович",
            "АБ-4213/26",
            "01.09.2026",
            "09.09.2026",
            "Лечение: терапия",
            "Завод",
            "инженер",
        ] {
            assert!(
                !proof.visible_text.contains(stale),
                "stale donor value {stale:?} leaked into {:?}",
                proof.visible_text
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn structural_multiline_inference_never_crosses_from_body_into_header_story() {
        let dir =
            std::env::temp_dir().join(format!("dokkomplekt-story-boundary-{}", std::process::id()));
        let input = dir.join("story-boundary.docx");
        let compiled = dir.join("compiled.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Лечение:</w:t></w:r></w:p>
<w:p><w:r><w:t>первая строка схемы</w:t></w:r></w:p>
<w:p><w:r><w:t>вторая строка схемы</w:t></w:r></w:p>
</w:body></w:document>"#;
        let header = r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>ГБУЗ НО «НКЦПЗ» диспансер №2</w:t></w:r></w:p></w:hdr>"#;
        write_test_docx(&input, body, Some(header));

        let report =
            compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
                .expect("body treatment must compile without absorbing header text");
        assert!(report
            .applied_field_ids
            .contains(&"medical.treatment".to_string()));
        let file = File::open(&compiled).expect("compiled file");
        let mut archive = ZipArchive::new(file).expect("compiled archive");
        let document_xml = read_zip_text(&mut archive, "word/document.xml");
        let header_xml = read_zip_text(&mut archive, "word/header1.xml");
        assert!(document_xml.contains("{{medical.treatment}}"));
        assert!(!document_xml.contains("первая строка схемы"));
        assert!(!document_xml.contains("вторая строка схемы"));
        assert!(header_xml.contains("ГБУЗ НО «НКЦПЗ» диспансер №2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn partially_dynamic_same_paragraph_compiles_remaining_labeled_value() {
        let dir =
            std::env::temp_dir().join(format!("dokkomplekt-partial-inline-{}", std::process::id()));
        let input = dir.join("partial-inline.docx");
        let compiled = dir.join("compiled.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Диагноз: {{medical.diagnosis}}; Лечение: старая схема</w:t></w:r></w:p>
</w:body></w:document>"#;
        write_test_docx(&input, body, None);

        let report =
            compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
                .expect("remaining inline treatment must compile");
        assert!(report
            .applied_field_ids
            .contains(&"medical.treatment".to_string()));
        let compiled_text = extract_docx_text(&compiled).expect("compiled text");
        assert!(compiled_text.contains("{{medical.diagnosis}}"));
        assert!(compiled_text.contains("{{medical.treatment}}"));
        assert!(!compiled_text.contains("старая схема"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn discharge_heading_with_department_number_compiles_the_case_number_only() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-discharge-number-anchor-{}",
            std::process::id()
        ));
        let input = dir.join("number-anchor.docx");
        let compiled = dir.join("compiled.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>09.09.2026 Выписной эпикриз № 4213, отделение № 2</w:t></w:r></w:p>
</w:body></w:document>"#;
        write_test_docx(&input, body, None);

        compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
            .expect("case number must bind next to discharge heading");
        let compiled_text = extract_docx_text(&compiled).expect("compiled text");
        assert!(compiled_text.contains("{{medical.case_number}}"));
        assert!(compiled_text.contains("отделение № 2"));
        assert!(!compiled_text.contains("№ 4213"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn short_case_number_is_replaced_after_heading_without_corrupting_leading_date() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-short-case-number-anchor-{}",
            std::process::id()
        ));
        let input = dir.join("short-number-anchor.docx");
        let compiled = dir.join("compiled.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>02.09.2026 Выписной эпикриз № 2</w:t></w:r></w:p>
</w:body></w:document>"#;
        write_test_docx(&input, body, None);

        compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
            .expect("short case number must stay scoped to the discharge heading");
        let compiled_text = extract_docx_text(&compiled).expect("compiled text");
        assert!(
            compiled_text
                .contains("{{medical.discharge_date}} Выписной эпикриз № {{medical.case_number}}"),
            "{compiled_text:?}"
        );
        assert!(!compiled_text.contains("0{{medical.case_number}}.09.2026"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn partly_dynamic_donor_header_keeps_existing_placeholder_and_compiles_old_date() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-partial-donor-header-{}",
            std::process::id()
        ));
        let input = dir.join("partial.docx");
        let compiled = dir.join("compiled.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>09.09.2026 Выписной эпикриз № {{medical.case_number}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{subject.name}}, 01.01.1980 г.р., зарегистрирован по адресу: Н. Новгород</w:t></w:r></w:p>
</w:body></w:document>"#;
        write_test_docx(&input, body, None);

        let report =
            compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
                .expect("compile remaining donor values");
        assert!(report
            .applied_field_ids
            .contains(&"medical.discharge_date".to_string()));
        assert!(report
            .applied_field_ids
            .contains(&"subject.birth_date".to_string()));
        assert!(report
            .applied_field_ids
            .contains(&"subject.address".to_string()));
        let text = extract_docx_text(&compiled).expect("read compiled partial donor template");
        assert!(text.contains("{{medical.case_number}}"));
        assert!(text.contains("{{medical.discharge_date}}"));
        assert!(text.contains("{{subject.name}}"));
        assert!(text.contains("{{subject.birth_date}}"));
        assert!(text.contains("{{subject.address}}"));
        assert!(!text.contains("09.09.2026"));
        assert!(!text.contains("01.01.1980"));
        assert!(!text.contains("Н. Новгород"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn donor_composite_discharge_docx_compiles_and_renders_current_case() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-donor-composite-discharge-{}",
            std::process::id()
        ));
        let input = dir.join("donor.docx");
        let compiled = dir.join("compiled.docx");
        let rendered = dir.join("rendered.docx");
        let body = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>09.09.2026      Выписной эпикриз № 4213</w:t></w:r></w:p>
<w:p><w:r><w:t>Иванов Иван Иванович, 01.01.1980 г.р., зарегистрирован по адресу: Н. Новгород</w:t></w:r></w:p>
<w:p><w:r><w:t>Находился на лечении в ГБУЗ НО «НКЦПЗ» диспансер №2 с 01.09.2026 по 09.09.2026</w:t></w:r></w:p>
<w:p><w:r><w:t>Диагноз: F20</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечение: старая терапия</w:t></w:r></w:p>
<w:p><w:r><w:t>Экспертный анамнез: Работает в Старый завод, в должности старый инженер.</w:t></w:r></w:p>
<w:p><w:r><w:t>Зав. отделением Петров П.П.                    Врач-психиатр Иванов И.И.</w:t></w:r></w:p>
</w:body></w:document>"#;
        write_test_docx(&input, body, None);

        let report =
            compile_labeled_template_file(&input, &compiled, &DomainKind::Medical, "discharge")
                .expect("compile donor composite discharge");
        for field_id in [
            "subject.name",
            "subject.birth_date",
            "subject.address",
            "medical.case_number",
            "medical.admission_date",
            "medical.discharge_date",
            "medical.diagnosis",
            "medical.treatment",
            "medical.expert_anamnesis",
        ] {
            assert!(
                report.applied_field_ids.iter().any(|item| item == field_id),
                "missing {field_id}: {report:?}"
            );
        }
        let compiled_text = extract_docx_text(&compiled).expect("compiled donor text");
        assert!(compiled_text.contains("{{medical.case_number}}"));
        assert_eq!(
            compiled_text.matches("{{medical.discharge_date}}").count(),
            2
        );
        assert!(compiled_text.contains("{{subject.name}}"));
        assert!(compiled_text.contains("{{medical.expert_anamnesis}}"));

        let mut case = case_with(&[
            ("subject.name", "Сидоров Сергей Сергеевич"),
            ("subject.birth_date", "02.02.1982"),
            ("subject.address", "Москва"),
            ("medical.case_number", "9001"),
            ("medical.admission_date", "10.10.2026"),
            ("medical.discharge_date", "20.10.2026"),
            ("medical.diagnosis", "F21"),
            ("medical.treatment", "новое лечение"),
            ("medical.workplace", "Новый завод"),
            ("medical.position", "мастер"),
        ]);
        dokkomplekt_core::domains::medical_semantics::set_medical_sick_leave_choice(
            &mut case, false,
        );
        let render_case = dokkomplekt_core::domains::case_for_document_render(
            &case,
            &DomainKind::Medical,
            "discharge",
        );
        let proof =
            render_docx_file_with_watermark_proof(&compiled, &rendered, &render_case, true, None)
                .expect("render donor composite with current semantic case");
        for expected in [
            "Сидоров Сергей Сергеевич",
            "02.02.1982",
            "Москва",
            "9001",
            "10.10.2026",
            "20.10.2026",
            "F21",
            "новое лечение",
            "Новый завод",
            "мастер",
            "В выдаче ЛН не нуждается",
        ] {
            assert!(
                proof.visible_text.contains(expected),
                "missing {expected:?}: {:?}",
                proof.visible_text
            );
        }
        for stale in [
            "Иванов Иван Иванович",
            "01.01.1980",
            "4213",
            "01.09.2026",
            "09.09.2026",
            "старая терапия",
            "Старый завод",
            "старый инженер",
        ] {
            assert!(
                !proof.visible_text.contains(stale),
                "stale {stale:?}: {:?}",
                proof.visible_text
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn multiline_insert_after_does_not_flatten_word_structure() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-filled-insert-after-{}",
            std::process::id()
        ));
        let input = dir.join("filled.docx");
        let marked = dir.join("marked.docx");
        write_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Лечение: терапия 1</w:t></w:r></w:p><w:p><w:r><w:t>терапия 2</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let value = "терапия 1\nтерапия 2";
        let report = apply_template_markup_file(
            &input,
            &marked,
            &[TemplateMarkupReplacement {
                field_id: "medical.treatment".into(),
                value: value.into(),
                action: TemplateMarkupAction::InsertAfter,
            }],
        )
        .expect("insert-after markup stays fail-closed");
        assert_eq!(report.replacement_count, 0);
        assert_eq!(report.skipped_values, vec![value.to_string()]);
        let text = extract_docx_text(&marked).expect("marked text");
        assert!(text.contains("терапия 1"));
        assert!(text.contains("терапия 2"));
        assert!(!text.contains("{{medical.treatment}}"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn renders_and_extracts_headers_as_part_of_the_document_contract() {
        let dir = std::env::temp_dir().join("dokkomplekt-docx-header-test");
        let tpl = dir.join("header.docx");
        let out = dir.join("header-rendered.docx");
        write_test_docx(
            &tpl,
            "<w:document><w:body><w:p><w:r><w:t>Тело {{org.name}}</w:t></w:r></w:p></w:body></w:document>",
            Some("<w:hdr><w:p><w:r><w:t>Шапка {{document.number}}</w:t></w:r></w:p></w:hdr>"),
        );
        let case = case_with(&[("org.name", "Ромашка"), ("document.number", "148")]);
        let proof = render_docx_file_with_watermark_proof(&tpl, &out, &case, true, None)
            .expect("render all text parts");
        assert!(proof.visible_text.contains("Тело Ромашка"));
        assert!(proof.visible_text.contains("Шапка 148"));
        assert!(!proof.visible_text.contains("{{"));
        let extracted = extract_docx_text(&out).expect("extract all text parts");
        assert!(extracted.contains("Тело Ромашка"));
        assert!(extracted.contains("Шапка 148"));
        assert!(!extracted.contains("{{"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn golden_realistic_docx_preserves_parts_and_renders_every_story() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx/complex_realistic_template.docx");
        let output = std::env::temp_dir().join(format!(
            "dokkomplekt-golden-render-{}.docx",
            std::process::id()
        ));
        let case = case_with(&[
            ("document.number", "Д-148"),
            ("document.date", "18.07.2026"),
            ("org.name", "ООО «Ромашка»"),
            ("org.inn", "7736050003"),
            ("subject.name", "Иванов Иван Иванович"),
        ]);
        let result =
            render_docx_file(&fixture, &output, &case, true).expect("golden fixture must render");
        assert!(result.missing_fields.is_empty());
        assert!(result.unknown_fields.is_empty());
        assert!(result.template_errors.is_empty());
        let extracted = extract_docx_text(&output).expect("rendered fixture must open");
        for expected in [
            "Договор № Д-148",
            "Организация: ООО «Ромашка»",
            "7736050003",
            "18.07.2026",
            "Шапка ООО «Ромашка»",
            "Страница договора Д-148",
            "Сноска для Иванов Иван Иванович",
            "Проверено 18.07.2026",
        ] {
            assert!(
                extracted.contains(expected),
                "missing {expected:?} in {extracted:?}"
            );
        }
        assert!(!extracted.contains("{{"));
        let archive = ZipArchive::new(File::open(&output).expect("open rendered archive"))
            .expect("rendered zip");
        assert!(archive
            .file_names()
            .any(|name| name == "word/media/image1.png"));
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn injects_images_into_body_and_header_without_word_com() {
        let dir = std::env::temp_dir().join(format!(
            "dokkomplekt-docx-image-injection-{}",
            std::process::id()
        ));
        let document = dir.join("images.docx");
        let image = dir.join("stamp.png");
        write_test_docx(
            &document,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>До [[DOKKOMPLEKT_IMAGE:org.stamp]] после</w:t></w:r></w:p></w:body></w:document>"#,
            Some(
                r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>[[DOKKOMPLEKT_IMAGE:org.stamp]]</w:t></w:r></w:p></w:hdr>"#,
            ),
        );
        std::fs::write(
            &image,
            [
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0,
                1, 8, 4, 0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100,
                248, 15, 0, 1, 5, 1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
                130,
            ],
        )
        .expect("write image fixture");

        inject_docx_images(&document, &[("org.stamp".into(), image.clone())])
            .expect("direct OOXML image injection");

        let file = File::open(&document).expect("open injected document");
        let mut archive = ZipArchive::new(file).expect("injected document is zip");
        let content_types = read_zip_text(&mut archive, "[Content_Types].xml");
        let body = read_zip_text(&mut archive, "word/document.xml");
        let body_rels = read_zip_text(&mut archive, "word/_rels/document.xml.rels");
        let header = read_zip_text(&mut archive, "word/header1.xml");
        let header_rels = read_zip_text(&mut archive, "word/_rels/header1.xml.rels");
        assert!(content_types.contains(r#"Extension="png""#));
        assert!(body.contains("<w:drawing"));
        assert!(body.contains("До "));
        assert!(body.contains(" после"));
        assert!(header.contains("<w:drawing"));
        assert!(!body.contains("DOKKOMPLEKT_IMAGE"));
        assert!(!header.contains("DOKKOMPLEKT_IMAGE"));
        assert!(body_rels.contains("relationships/image"));
        assert!(header_rels.contains("relationships/image"));
        assert!(archive
            .file_names()
            .any(|name| name.starts_with("word/media/dokkomplekt-org-stamp-")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn macro_binary_parts_are_rejected_before_rendering_docm() {
        let dir = std::env::temp_dir().join("dokkomplekt-docm-rejection-test");
        let tpl = dir.join("macro-template.docm");
        let out = dir.join("macro-rendered.docm");
        std::fs::create_dir_all(&dir).expect("create test dir");
        {
            let file = File::create(&tpl).expect("create docm");
            let mut writer = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            writer
                .start_file("[Content_Types].xml", options)
                .expect("content types");
            writer.write_all(b"<Types/>").expect("content types bytes");
            writer
                .start_file("word/document.xml", options)
                .expect("document");
            writer
                .write_all(b"<w:document><w:body><w:p><w:r><w:t>{{org.name}}</w:t></w:r></w:p></w:body></w:document>")
                .expect("document bytes");
            writer
                .start_file("word/vbaProject.bin", options)
                .expect("macro part");
            writer
                .write_all(b"synthetic-vba-project")
                .expect("macro bytes");
            writer.finish().expect("finish docm");
        }

        let error = render_docx_file(&tpl, &out, &case_with(&[("org.name", "Ромашка")]), true)
            .expect_err("active content must be rejected");
        assert!(matches!(
            error,
            DocxError::UnsafeActiveContent(ref part) if part == "word/vbaProject.bin"
        ));
        assert!(
            !out.exists(),
            "rejected DOCM must not create an output file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn license_watermark_is_written_into_the_resulting_docx() {
        let dir = std::env::temp_dir().join("dokkomplekt-docx-watermark-test");
        let tpl = dir.join("watermark.docx");
        let out = dir.join("watermark-rendered.docx");
        write_test_docx(
            &tpl,
            "<w:document><w:body><w:p><w:r><w:t>Документ {{org.name}}</w:t></w:r></w:p></w:body></w:document>",
            None,
        );
        let case = case_with(&[("org.name", "Ромашка")]);
        let result = render_docx_file_with_watermark(
            &tpl,
            &out,
            &case,
            true,
            Some("ПРОБНАЯ ВЕРСИЯ & ПРОВЕРКА"),
        )
        .expect("render with watermark");
        assert!(result
            .warnings
            .contains(&"license_watermark_applied".to_string()));
        let text = extract_docx_text(&out).expect("extract");
        assert!(text.contains("ПРОБНАЯ ВЕРСИЯ & ПРОВЕРКА"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_failure_never_creates_partial_destination() {
        let dir = std::env::temp_dir().join("dokkomplekt-docx-strict-test");
        let tpl = dir.join("strict.docx");
        let out = dir.join("must-not-exist.docx");
        let _ = std::fs::remove_file(&out);
        write_test_docx(
            &tpl,
            "<w:document><w:body><w:p><w:r><w:t>{{org.name}}</w:t></w:r></w:p></w:body></w:document>",
            None,
        );
        let error = render_docx_file(&tpl, &out, &SemanticCase::default(), true)
            .expect_err("missing field must block strict render");
        assert!(matches!(error, DocxError::StrictRenderBlocked { .. }));
        assert!(!out.exists(), "strict failure left a partial DOCX");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decodes_numeric_entities() {
        // &#8470; is № ; &#x41; is A
        let xml = "<w:p><w:r><w:t>&#8470; &#x41;</w:t></w:r></w:p>";
        assert_eq!(xml_to_text(xml), "№ A");
    }
    #[test]
    fn promotes_and_clones_complete_table_rows() {
        use dokkomplekt_core::{SemanticAtom, SemanticRecord};
        let xml = r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>{{#each items}}{{item.name}}</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>{{item.price}}{{/each}}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let mut c = SemanticCase::default();
        let mut a = SemanticRecord::new();
        a.insert("name".into(), SemanticAtom::Text("A".into()));
        a.insert("price".into(), SemanticAtom::Text("10".into()));
        let mut b = a.clone();
        b.insert("name".into(), SemanticAtom::Text("B".into()));
        c.collections.insert("items".into(), vec![a, b]);
        let rendered = render_docx_xml_template(&promote_table_row_loops(xml), &c, true);
        assert!(rendered.template_errors.is_empty());
        assert_eq!(rendered.output_text.matches("<w:tr>").count(), 2);
        assert!(rendered.output_text.contains(">A<") && rendered.output_text.contains(">B<"));
    }

    #[test]
    fn template_markup_never_treats_table_tags_as_text_nodes() {
        let xml = r#"<w:document><w:body><w:tbl><w:tblPr/><w:tr><w:tc><w:p><w:r><w:t>ООО «Ромашка»</w:t><w:tab/></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let result = replace_visible_text_once(xml, "ООО «Ромашка»", "{{org.name}}")
            .expect("visible value must be replaced");
        for structural_tag in ["<w:tbl>", "<w:tblPr/>", "<w:tr>", "<w:tc>", "<w:tab/>"] {
            assert_eq!(
                result.matches(structural_tag).count(),
                xml.matches(structural_tag).count(),
                "structural tag was damaged: {structural_tag}"
            );
        }
        assert_eq!(xml_to_text(&result), "{{org.name}}");
    }

    #[test]
    fn promotes_outer_row_loop_without_stopping_at_nested_table_row() {
        use dokkomplekt_core::{SemanticAtom, SemanticRecord};
        let xml = r#"<w:tbl><w:tr><w:tc><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Вложенная строка</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc><w:tc><w:p><w:r><w:t>{{#each items}}{{item.name}}{{/each}}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let promoted = promote_table_row_loops(xml);
        assert!(promoted.starts_with("<w:tbl>{{#each items}}<w:tr>"));
        assert!(promoted.ends_with("</w:tr>{{/each}}</w:tbl>"));
        assert_eq!(
            promoted.matches("<w:tr>").count(),
            promoted.matches("</w:tr>").count()
        );

        let mut case = SemanticCase::default();
        let mut first = SemanticRecord::new();
        first.insert("name".into(), SemanticAtom::Text("A".into()));
        let mut second = SemanticRecord::new();
        second.insert("name".into(), SemanticAtom::Text("B".into()));
        case.collections.insert("items".into(), vec![first, second]);
        let rendered = render_docx_xml_template(&promoted, &case, true);
        assert!(
            rendered.template_errors.is_empty(),
            "{:?}",
            rendered.template_errors
        );
        assert_eq!(rendered.output_text.matches("<w:tr>").count(), 4);
        assert_eq!(rendered.output_text.matches("</w:tr>").count(), 4);
        assert!(rendered.output_text.contains(">A<"));
        assert!(rendered.output_text.contains(">B<"));
    }

    #[test]
    fn macro_template_is_rejected_before_render() {
        let bytes = build_test_docx(&[
            (
                "word/document.xml",
                "<w:document><w:body/></w:document>".as_bytes(),
            ),
            ("word/vbaProject.bin", b"MZ-not-a-real-macro"),
        ]);
        let error = validate_safe_template_bytes(&bytes)
            .unwrap_err()
            .to_string();
        assert!(error.contains("vbaProject.bin"), "{error}");
    }

    #[test]
    fn external_relationship_is_rejected() {
        let relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/attachedTemplate" Target="https://example.invalid/template.dotm" TargetMode="External"/></Relationships>"#;
        let bytes = build_test_docx(&[
            (
                "word/document.xml",
                "<w:document><w:body/></w:document>".as_bytes(),
            ),
            ("word/_rels/document.xml.rels", relationships),
        ]);
        let error = validate_safe_template_bytes(&bytes)
            .unwrap_err()
            .to_string();
        assert!(error.contains("external/active relationship"), "{error}");
    }

    #[test]
    fn visible_text_replacement_crosses_word_runs() {
        let xml = "<w:r><w:t>ООО «Ро</w:t></w:r><w:r><w:t>машка»</w:t></w:r>";
        let result = replace_visible_text_once(xml, "ООО «Ромашка»", "{{org.name}}").unwrap();
        assert!(result.contains("{{org.name}}"));
        assert!(!xml_to_text(&result).contains("Ромашка"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocxStructuralFingerprint {
    pub placeholders: Vec<String>,
    pub story_parts: Vec<String>,
    pub story_sha256: BTreeMap<String, String>,
    pub table_count: usize,
    pub row_count: usize,
    pub cell_count: usize,
    pub section_count: usize,
    pub page_break_count: usize,
    pub content_control_count: usize,
    pub field_count: usize,
    pub header_count: usize,
    pub footer_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateRegressionSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TemplateRegressionIssue {
    pub code: String,
    pub severity: TemplateRegressionSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TemplateRegressionReport {
    pub previous: DocxStructuralFingerprint,
    pub candidate: DocxStructuralFingerprint,
    pub issues: Vec<TemplateRegressionIssue>,
    pub critical: bool,
}

/// Build a deterministic structural fingerprint without Microsoft Word.
/// It covers all text-bearing stories, placeholders, tables, sections,
/// page-break markers and content controls.
pub fn inspect_docx_structure(path: &Path) -> DocxResult<DocxStructuralFingerprint> {
    use sha2::{Digest as _, Sha256};
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut story_parts = Vec::new();
    let mut story_sha256 = BTreeMap::new();
    let mut all_text = String::new();
    let mut table_count = 0usize;
    let mut row_count = 0usize;
    let mut cell_count = 0usize;
    let mut section_count = 0usize;
    let mut page_break_count = 0usize;
    let mut content_control_count = 0usize;
    let mut field_count = 0usize;
    let mut header_count = 0usize;
    let mut footer_count = 0usize;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if !is_text_bearing_word_part(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let xml = String::from_utf8(bytes.clone())?;
        if name.starts_with("word/header") {
            header_count += 1;
        }
        if name.starts_with("word/footer") {
            footer_count += 1;
        }
        table_count += xml.matches("<w:tbl").count();
        row_count += xml.matches("<w:tr").count();
        cell_count += xml.matches("<w:tc").count();
        section_count += xml.matches("<w:sectPr").count();
        page_break_count += xml.matches("w:type=\"page\"").count();
        content_control_count += xml.matches("<w:sdt").count();
        field_count += xml.matches("<w:fldSimple").count() + xml.matches("<w:instrText").count();
        let extracted = xml_to_text(&xml);
        if !all_text.is_empty() {
            all_text.push('\n');
        }
        all_text.push_str(&extracted);
        story_sha256.insert(name.clone(), hex::encode(Sha256::digest(&bytes)));
        story_parts.push(name);
    }
    story_parts.sort();
    let mut placeholders = dokkomplekt_core::template_field_references(&all_text);
    placeholders.sort();
    placeholders.dedup();
    Ok(DocxStructuralFingerprint {
        placeholders,
        story_parts,
        story_sha256,
        table_count,
        row_count,
        cell_count,
        section_count,
        page_break_count,
        content_control_count,
        field_count,
        header_count,
        footer_count,
    })
}

pub fn compare_docx_structures(
    previous_path: &Path,
    candidate_path: &Path,
) -> DocxResult<TemplateRegressionReport> {
    let previous = inspect_docx_structure(previous_path)?;
    let candidate = inspect_docx_structure(candidate_path)?;
    let mut issues = Vec::new();
    let previous_fields = previous
        .placeholders
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let candidate_fields = candidate
        .placeholders
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let removed = previous_fields
        .difference(&candidate_fields)
        .cloned()
        .collect::<Vec<_>>();
    let added = candidate_fields
        .difference(&previous_fields)
        .cloned()
        .collect::<Vec<_>>();
    if !removed.is_empty() {
        issues.push(TemplateRegressionIssue {
            code: "placeholder_removed".into(),
            severity: TemplateRegressionSeverity::Critical,
            message: format!("Исчезли поля workflow: {}.", removed.join(", ")),
        });
    }
    if !added.is_empty() {
        issues.push(TemplateRegressionIssue {
            code: "placeholder_added".into(),
            severity: TemplateRegressionSeverity::Warning,
            message: format!("Добавлены новые поля: {}.", added.join(", ")),
        });
    }
    for (code, label, before, after) in [
        (
            "table_lost",
            "таблиц",
            previous.table_count,
            candidate.table_count,
        ),
        (
            "row_lost",
            "строк таблиц",
            previous.row_count,
            candidate.row_count,
        ),
        (
            "section_lost",
            "секций",
            previous.section_count,
            candidate.section_count,
        ),
        (
            "header_lost",
            "верхних колонтитулов",
            previous.header_count,
            candidate.header_count,
        ),
        (
            "footer_lost",
            "нижних колонтитулов",
            previous.footer_count,
            candidate.footer_count,
        ),
        (
            "page_break_lost",
            "разрывов страниц",
            previous.page_break_count,
            candidate.page_break_count,
        ),
    ] {
        if after < before {
            issues.push(TemplateRegressionIssue {
                code: code.into(),
                severity: TemplateRegressionSeverity::Critical,
                message: format!("Количество {label} уменьшилось: {before} → {after}."),
            });
        }
    }
    if candidate.content_control_count < previous.content_control_count {
        issues.push(TemplateRegressionIssue {
            code: "content_control_lost".into(),
            severity: TemplateRegressionSeverity::Warning,
            message: format!(
                "Количество content controls уменьшилось: {} → {}.",
                previous.content_control_count, candidate.content_control_count
            ),
        });
    }
    for part in &previous.story_parts {
        if !candidate.story_parts.contains(part) {
            issues.push(TemplateRegressionIssue {
                code: "story_part_lost".into(),
                severity: TemplateRegressionSeverity::Critical,
                message: format!("Из DOCX исчезла часть {part}."),
            });
        }
    }
    let critical = issues
        .iter()
        .any(|issue| issue.severity == TemplateRegressionSeverity::Critical);
    Ok(TemplateRegressionReport {
        previous,
        candidate,
        issues,
        critical,
    })
}
