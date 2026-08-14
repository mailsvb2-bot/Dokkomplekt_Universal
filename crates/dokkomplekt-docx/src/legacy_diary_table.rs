//! Compatibility adapter for legacy Word diary tables.
//!
//! Older `diary-filler` templates contain a pre-built Word table rather than
//! `{{#each diaries}}` collection markers.  The universal core must not learn
//! Word table layout or medical formatting, so this adapter lives at the DOCX
//! boundary and consumes the same canonical `SemanticCase.collections` data as
//! the modern template renderer.

use dokkomplekt_core::{
    prepare_professional_collections, SemanticAtom, SemanticCase, SemanticRecord,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LegacyDiaryTableReport {
    pub xml: String,
    pub detected_tables: usize,
    pub detected_rows: usize,
    pub filled_rows: usize,
    pub removed_after_discharge: usize,
    pub final_rows: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct TableLayout {
    diary_col: usize,
    day_col: Option<usize>,
    hospitalization_day_col: Option<usize>,
    month_year_col: Option<usize>,
}

#[derive(Debug, Clone)]
struct TablePlan {
    range: (usize, usize),
    row_ranges: Vec<(usize, usize)>,
    data_row_indices: Vec<usize>,
    layout: TableLayout,
}

pub(crate) fn fill_legacy_diary_tables(
    xml: &str,
    case: &SemanticCase,
    strict: bool,
) -> Result<LegacyDiaryTableReport, String> {
    let mut report = LegacyDiaryTableReport {
        xml: xml.to_string(),
        ..Default::default()
    };

    let table_ranges = element_ranges(xml, "w:tbl");
    if table_ranges.is_empty() {
        return Ok(report);
    }

    let mut plans = Vec::new();
    for range in table_ranges {
        let table_xml = &xml[range.0..range.1];
        // Modern collection-aware templates are handled by the canonical
        // template engine. Never rewrite the same table twice.
        if table_xml.contains("{{#each") || table_xml.contains("{{diary.") {
            continue;
        }
        let Some(layout) = detect_diary_table_layout(table_xml, strict)? else {
            continue;
        };
        let row_ranges = element_ranges(table_xml, "w:tr");
        let data_row_indices = row_ranges
            .iter()
            .enumerate()
            .filter_map(|(index, (start, end))| {
                let row = &table_xml[*start..*end];
                is_data_row(row, &layout).then_some(index)
            })
            .collect::<Vec<_>>();
        if data_row_indices.is_empty() {
            if strict {
                return Err(
                    "Обнаружена таблица дневников, но не найдены строки с числом или днём госпитализации. Шаблон не изменён."
                        .into(),
                );
            }
            report
                .warnings
                .push("legacy_diary_table_without_recognizable_data_rows".into());
            continue;
        }
        plans.push(TablePlan {
            range,
            row_ranges,
            data_row_indices,
            layout,
        });
    }

    if plans.is_empty() {
        return Ok(report);
    }
    report.detected_tables = plans.len();
    report.detected_rows = plans.iter().map(|plan| plan.data_row_indices.len()).sum();

    // Ask the universal domain layer for the same collection used by modern
    // `{{#each diaries}}` templates. No scheduling/content rule is duplicated in
    // this Word compatibility adapter.
    let prepared =
        prepare_professional_collections("{{#each diaries}}{{diary.text}}{{/each}}", case);
    let Some(entries) = prepared.collection("diaries") else {
        if strict {
            return Err(
                "Шаблон содержит таблицу дневников, но медицинский профиль не смог построить коллекцию diaries. Проверьте дату поступления, дату выписки и источник текстов дневников."
                    .into(),
            );
        }
        report
            .warnings
            .push("legacy_diary_table_missing_diary_collection".into());
        return Ok(report);
    };
    if entries.is_empty() {
        if strict {
            return Err("Коллекция дневников пуста; дневниковый документ не создан.".into());
        }
        return Ok(report);
    }
    if report.detected_rows < entries.len() && strict {
        return Err(format!(
            "В legacy-шаблоне только {} распознанных строк дневников, а по подтверждённым датам требуется {}. Добавьте строки в шаблон — сокращать дневники нельзя.",
            report.detected_rows,
            entries.len()
        ));
    }
    if report.detected_rows < entries.len() {
        report.warnings.push(format!(
            "legacy_diary_table_capacity_shortfall:{}<{}",
            report.detected_rows,
            entries.len()
        ));
    }

    // Replace tables back-to-front to keep byte ranges stable.
    for plan in plans.iter().rev() {
        let table_xml = &xml[plan.range.0..plan.range.1];
        let rows_before_this = plans
            .iter()
            .take_while(|candidate| candidate.range.0 < plan.range.0)
            .map(|candidate| candidate.data_row_indices.len())
            .sum::<usize>();
        let mut table = table_xml.to_string();
        let mut edits = Vec::<(usize, usize, Option<String>)>::new();
        for (local_data_index, row_index) in plan.data_row_indices.iter().enumerate() {
            let global_index = rows_before_this + local_data_index;
            let (row_start, row_end) = plan.row_ranges[*row_index];
            if global_index >= entries.len() {
                edits.push((row_start, row_end, None));
                continue;
            }
            let row_xml = &table_xml[row_start..row_end];
            let filled = fill_row(row_xml, &plan.layout, &entries[global_index], strict)?;
            edits.push((row_start, row_end, Some(filled)));
        }
        for (start, end, replacement) in edits.into_iter().rev() {
            match replacement {
                Some(value) => table.replace_range(start..end, &value),
                None => table.replace_range(start..end, ""),
            }
        }
        report.xml.replace_range(plan.range.0..plan.range.1, &table);
    }

    report.filled_rows = report.detected_rows.min(entries.len());
    report.removed_after_discharge = report.detected_rows.saturating_sub(entries.len());
    report.final_rows = entries
        .iter()
        .take(report.filled_rows)
        .filter(|row| atom_bool(row, "is_final"))
        .count();
    if strict && report.final_rows != 1 {
        return Err(format!(
            "После заполнения legacy-дневников ожидалась ровно одна итоговая запись, получено {}.",
            report.final_rows
        ));
    }
    Ok(report)
}

fn detect_diary_table_layout(table_xml: &str, strict: bool) -> Result<Option<TableLayout>, String> {
    let rows = element_ranges(table_xml, "w:tr");
    if rows.is_empty() {
        return Ok(None);
    }
    let header_rows = rows.iter().take(5).collect::<Vec<_>>();
    let mut diary_candidates = Vec::new();
    let mut day_candidates = Vec::new();
    let mut hospitalization_candidates = Vec::new();
    let mut month_year_candidates = Vec::new();

    for (start, end) in header_rows {
        let row = &table_xml[*start..*end];
        for (index, cell) in cells(row).iter().enumerate() {
            let normalized = normalize(&visible_text(cell));
            if normalized.contains("дневник") || normalized.contains("наблюдени") {
                diary_candidates.push(index);
            }
            if normalized.contains("число") {
                day_candidates.push(index);
            }
            if normalized.contains("госпит")
                && (normalized.contains("день") || normalized.contains("ализа"))
            {
                hospitalization_candidates.push(index);
            }
            if normalized.contains("месяц")
                && (normalized.contains("год") || normalized.contains('/'))
            {
                month_year_candidates.push(index);
            }
        }
    }
    diary_candidates.sort_unstable();
    diary_candidates.dedup();
    if diary_candidates.is_empty() {
        return Ok(None);
    }
    if diary_candidates.len() != 1 {
        if strict {
            return Err(format!(
                "В таблице дневников неоднозначно определена колонка текста: {:?}.",
                diary_candidates
            ));
        }
        return Ok(None);
    }
    dedup(&mut day_candidates);
    dedup(&mut hospitalization_candidates);
    dedup(&mut month_year_candidates);
    if strict
        && (day_candidates.len() > 1
            || hospitalization_candidates.len() > 1
            || month_year_candidates.len() > 1)
    {
        return Err("В таблице дневников неоднозначны служебные колонки дат/дня госпитализации. Шаблон не изменён.".into());
    }
    Ok(Some(TableLayout {
        diary_col: diary_candidates[0],
        day_col: day_candidates.first().copied(),
        hospitalization_day_col: hospitalization_candidates.first().copied(),
        month_year_col: month_year_candidates.first().copied(),
    }))
}

fn dedup(values: &mut Vec<usize>) {
    values.sort_unstable();
    values.dedup();
}

fn is_data_row(row_xml: &str, layout: &TableLayout) -> bool {
    let row_cells = cells(row_xml);
    if row_cells.is_empty() {
        return false;
    }
    if layout
        .day_col
        .and_then(|index| row_cells.get(index))
        .is_some_and(|cell| contains_integer(&visible_text(cell)))
    {
        return true;
    }
    if layout
        .hospitalization_day_col
        .and_then(|index| row_cells.get(index))
        .is_some_and(|cell| contains_integer(&visible_text(cell)))
    {
        return true;
    }
    contains_integer(&visible_text(row_cells[0]))
}

fn fill_row(
    row_xml: &str,
    layout: &TableLayout,
    record: &SemanticRecord,
    strict: bool,
) -> Result<String, String> {
    let cell_ranges = element_ranges(row_xml, "w:tc");
    if layout.diary_col >= cell_ranges.len() {
        return Err("Строка дневника не содержит распознанную колонку текста.".into());
    }
    let mut edits = Vec::<(usize, usize, String)>::new();

    if let Some(index) = layout.day_col {
        if let Some((start, end)) = cell_ranges.get(index).copied() {
            let value = atom_text(record, "day_number")
                .or_else(|| atom_text(record, "day"))
                .unwrap_or_default();
            edits.push((start, end, replace_cell_text(&row_xml[start..end], &value)));
        }
    }
    if let Some(index) = layout.hospitalization_day_col {
        if let Some((start, end)) = cell_ranges.get(index).copied() {
            let value = atom_text(record, "sequence").unwrap_or_default();
            edits.push((start, end, replace_cell_text(&row_xml[start..end], &value)));
        }
    }
    if let Some(index) = layout.month_year_col {
        if let Some((start, end)) = cell_ranges.get(index).copied() {
            let month = atom_text(record, "month").unwrap_or_default();
            let year = atom_text(record, "year").unwrap_or_default();
            let value = if month.is_empty() || year.is_empty() {
                atom_text(record, "date").unwrap_or_default()
            } else {
                format!("{month:0>2}.{year:0>4}")
            };
            edits.push((start, end, replace_cell_text(&row_xml[start..end], &value)));
        }
    }

    let (diary_start, diary_end) = cell_ranges[layout.diary_col];
    let existing = visible_text(&row_xml[diary_start..diary_end]);
    let body = atom_text(record, "text").unwrap_or_default();
    if strict && body.trim().is_empty() {
        return Err(format!(
            "Для дневника {} отсутствует профессиональный текст; пустая запись не будет опубликована.",
            atom_text(record, "date").unwrap_or_else(|| "без даты".into())
        ));
    }
    let content = diary_cell_content(&existing, &body, record);
    edits.push((
        diary_start,
        diary_end,
        replace_cell_text(&row_xml[diary_start..diary_end], &content),
    ));

    let mut out = row_xml.to_string();
    edits.sort_by_key(|edit| edit.0);
    for (start, end, replacement) in edits.into_iter().rev() {
        out.replace_range(start..end, &replacement);
    }
    Ok(out)
}

fn diary_cell_content(existing: &str, body: &str, record: &SemanticRecord) -> String {
    let lines = existing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let structural = lines
        .iter()
        .copied()
        .filter(|line| normalize(line).starts_with("совместный осмотр"))
        .collect::<Vec<_>>();
    let existing_signatures = lines
        .iter()
        .copied()
        .filter(|line| is_signature_line(line))
        .collect::<Vec<_>>();
    let has_treating_signature = existing_signatures
        .iter()
        .any(|line| is_treating_signature(line));
    let has_department_head_signature = existing_signatures
        .iter()
        .any(|line| is_department_head_signature(line));

    let mut parts = structural
        .iter()
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>();
    if !body.trim().is_empty() {
        parts.push(body.trim().to_string());
    }
    parts.extend(existing_signatures.into_iter().map(str::to_string));
    if !has_treating_signature {
        if let Some(value) = atom_text(record, "treating_physician_signature") {
            parts.push(value);
        }
    }
    if !has_department_head_signature {
        if let Some(value) = atom_text(record, "department_head_signature") {
            parts.push(value);
        }
    }
    parts.join("\n")
}

fn has_signature_cue(value: &str) -> bool {
    let normalized = normalize(value);
    value.contains("___") || normalized.contains("подпись") || value.contains("/____")
}

fn is_treating_signature(value: &str) -> bool {
    normalize(value).contains("лечащий врач") && has_signature_cue(value)
}

fn is_department_head_signature(value: &str) -> bool {
    let normalized = normalize(value);
    (normalized.contains("зав. отдел")
        || normalized.contains("зав отдел")
        || normalized.contains("заведующ") && normalized.contains("отдел"))
        && has_signature_cue(value)
}

fn is_signature_line(value: &str) -> bool {
    is_treating_signature(value) || is_department_head_signature(value)
}

fn atom_text(record: &SemanticRecord, key: &str) -> Option<String> {
    record.get(key).map(SemanticAtom::as_text)
}

fn atom_bool(record: &SemanticRecord, key: &str) -> bool {
    match record.get(key) {
        Some(SemanticAtom::Boolean(value)) => *value,
        Some(value) => matches!(
            value.as_text().trim().to_lowercase().as_str(),
            "1" | "true" | "да"
        ),
        None => false,
    }
}

fn contains_integer(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit())
}

fn normalize(value: &str) -> String {
    value
        .replace('\u{00a0}', " ")
        .to_lowercase()
        .replace('ё', "е")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn cells(row_xml: &str) -> Vec<&str> {
    element_ranges(row_xml, "w:tc")
        .into_iter()
        .map(|(start, end)| &row_xml[start..end])
        .collect()
}

fn replace_cell_text(cell_xml: &str, value: &str) -> String {
    let Some(open_end) = cell_xml.find('>').map(|offset| offset + 1) else {
        return cell_xml.to_string();
    };
    let Some(close_start) = cell_xml.rfind("</w:tc>") else {
        return cell_xml.to_string();
    };
    let inner = &cell_xml[open_end..close_start];
    let properties = if inner.trim_start().starts_with("<w:tcPr") {
        if let Some(start) = inner.find("<w:tcPr") {
            if let Some(end) = matching_element_end(inner, start, "w:tcPr") {
                &inner[..end]
            } else {
                ""
            }
        } else {
            ""
        }
    } else {
        ""
    };
    let paragraph_properties = first_element(inner, "w:pPr").unwrap_or("");
    let run_properties = first_element(inner, "w:rPr").unwrap_or("");
    let paragraphs = value
        .split('\n')
        .map(|line| {
            format!(
                "<w:p>{paragraph_properties}<w:r>{run_properties}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                escape_xml(line)
            )
        })
        .collect::<String>();
    format!(
        "{}{}{}{}",
        &cell_xml[..open_end],
        properties,
        paragraphs,
        &cell_xml[close_start..]
    )
}

fn first_element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let (start, end) = element_ranges(xml, name).into_iter().next()?;
    Some(&xml[start..end])
}

fn visible_text(xml: &str) -> String {
    let paragraphs = element_ranges(xml, "w:p");
    if paragraphs.is_empty() {
        return visible_text_runs(xml);
    }
    paragraphs
        .into_iter()
        .map(|(start, end)| visible_text_runs(&xml[start..end]))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn visible_text_runs(xml: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0usize;
    while let Some(relative) = xml[cursor..].find("<w:t") {
        let start = cursor + relative;
        let Some(open_end) = xml[start..].find('>').map(|offset| start + offset + 1) else {
            break;
        };
        let Some(close_relative) = xml[open_end..].find("</w:t>") else {
            break;
        };
        let close = open_end + close_relative;
        output.push_str(&decode_xml(&xml[open_end..close]));
        cursor = close + "</w:t>".len();
    }
    output
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn element_ranges(xml: &str, name: &str) -> Vec<(usize, usize)> {
    let marker = format!("<{name}");
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = xml[cursor..].find(&marker) {
        let start = cursor + relative;
        if !xml_name_boundary(xml.as_bytes().get(start + marker.len()).copied()) {
            cursor = start + marker.len();
            continue;
        }
        let Some(end) = matching_element_end(xml, start, name) else {
            break;
        };
        ranges.push((start, end));
        cursor = end;
    }
    ranges
}

fn matching_element_end(xml: &str, start: usize, name: &str) -> Option<usize> {
    let opening_end = start + xml[start..].find('>')? + 1;
    if xml[start..opening_end].trim_end().ends_with("/>") {
        return Some(opening_end);
    }
    let open_marker = format!("<{name}");
    let close_marker = format!("</{name}");
    let mut depth = 1usize;
    let mut cursor = opening_end;
    while depth > 0 {
        let next_open = xml[cursor..]
            .find(&open_marker)
            .map(|offset| cursor + offset);
        let next_close = xml[cursor..]
            .find(&close_marker)
            .map(|offset| cursor + offset);
        match (next_open, next_close) {
            (_, None) => return None,
            (Some(open), Some(close)) if open < close => {
                if xml_name_boundary(xml.as_bytes().get(open + open_marker.len()).copied()) {
                    let end = open + xml[open..].find('>')? + 1;
                    if !xml[open..end].trim_end().ends_with("/>") {
                        depth += 1;
                    }
                    cursor = end;
                } else {
                    cursor = open + open_marker.len();
                }
            }
            (_, Some(close)) => {
                if !xml_name_boundary(xml.as_bytes().get(close + close_marker.len()).copied()) {
                    cursor = close + close_marker.len();
                    continue;
                }
                let end = close + xml[close..].find('>')? + 1;
                depth -= 1;
                cursor = end;
            }
        }
    }
    Some(cursor)
}

fn xml_name_boundary(byte: Option<u8>) -> bool {
    matches!(
        byte,
        None | Some(b'>') | Some(b'/') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dokkomplekt_core::{DomainKind, SemanticValue, ValueSource};

    fn medical_case() -> SemanticCase {
        let mut case = SemanticCase::default();
        case.active_domains.push(DomainKind::Medical);
        for (field, value) in [
            ("medical.admission_date", "10.05.2026"),
            ("medical.discharge_date", "12.05.2026"),
            ("medical.diagnosis", "F20.0"),
        ] {
            case.values.insert(
                field.into(),
                SemanticValue::new(field, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        let mut regular = SemanticRecord::new();
        regular.insert("diagnosis".into(), SemanticAtom::Text("F20.0".into()));
        regular.insert("text".into(), SemanticAtom::Text("Основной статус".into()));
        let mut final_row = SemanticRecord::new();
        final_row.insert("diagnosis".into(), SemanticAtom::Text("F20.0".into()));
        final_row.insert("text".into(), SemanticAtom::Text("Итоговый статус".into()));
        final_row.insert("is_final".into(), SemanticAtom::Boolean(true));
        case.set_collection("medical_diary_texts", vec![regular, final_row]);
        case
    }

    fn cell(text: &str) -> String {
        format!(
            "<w:tc><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:tc>",
            escape_xml(text)
        )
    }

    fn row(values: &[&str]) -> String {
        format!(
            "<w:tr>{}</w:tr>",
            values.iter().map(|value| cell(value)).collect::<String>()
        )
    }

    fn legacy_table() -> String {
        format!(
            "<w:document><w:body><w:tbl>{}{}{}{}{}</w:tbl></w:body></w:document>",
            row(&[
                "День госпитализации",
                "Число",
                "Месяц / год",
                "Дневник наблюдения"
            ]),
            row(&[
                "1",
                "01",
                "",
                "Совместный осмотр.\nЛечащий врач ___\nЗаведующий отделением ___"
            ]),
            row(&["2", "02", "", ""]),
            row(&["3", "03", "", ""]),
            row(&["4", "04", "", ""]),
        )
    }

    #[test]
    fn fills_legacy_rows_and_removes_rows_after_discharge() {
        let report = fill_legacy_diary_tables(&legacy_table(), &medical_case(), true).unwrap();
        assert_eq!(report.detected_tables, 1);
        assert_eq!(report.filled_rows, 2);
        assert_eq!(report.removed_after_discharge, 2);
        assert_eq!(report.final_rows, 1);
        assert!(report.xml.contains("11"));
        assert!(report.xml.contains("12"));
        assert!(report.xml.contains("05.2026"));
        assert!(!report.xml.contains("5/2026"));
        assert!(report.xml.contains("Основной статус"));
        assert!(report.xml.contains("Итоговый статус"));
        assert!(report.xml.contains("Совместный осмотр"));
        assert!(!report.xml.contains(">03<"));
        assert!(!report.xml.contains(">04<"));
    }

    #[test]
    fn missing_second_signature_is_added_without_replacing_existing_one() {
        let mut record = SemanticRecord::new();
        record.insert("text".into(), SemanticAtom::Text("Статус".into()));
        record.insert(
            "treating_physician_signature".into(),
            SemanticAtom::Text("Лечащий врач GENERATED".into()),
        );
        record.insert(
            "department_head_signature".into(),
            SemanticAtom::Text("Заведующий отделением GENERATED".into()),
        );
        let result = diary_cell_content("Лечащий врач ___", "Статус", &record);
        assert!(result.contains("Лечащий врач ___"));
        assert!(!result.contains("Лечащий врач GENERATED"));
        assert!(result.contains("Заведующий отделением GENERATED"));
    }

    #[test]
    fn visible_text_preserves_paragraph_boundaries_and_split_runs() {
        let xml = concat!(
            "<w:tc>",
            "<w:p><w:r><w:t>Совместный </w:t></w:r><w:r><w:t>осмотр</w:t></w:r></w:p>",
            "<w:p><w:r><w:t>Лечащий врач ___</w:t></w:r></w:p>",
            "</w:tc>"
        );
        assert_eq!(visible_text(xml), "Совместный осмотр\nЛечащий врач ___");
    }

    #[test]
    fn unrelated_table_is_untouched() {
        let xml = format!(
            "<w:document><w:body><w:tbl>{}</w:tbl></w:body></w:document>",
            row(&["Сумма", "Цена"])
        );
        let report = fill_legacy_diary_tables(&xml, &medical_case(), true).unwrap();
        assert_eq!(report.xml, xml);
        assert_eq!(report.detected_tables, 0);
    }

    #[test]
    fn ambiguous_diary_columns_fail_closed() {
        let xml = format!(
            "<w:document><w:body><w:tbl>{}{}</w:tbl></w:body></w:document>",
            row(&["Число", "Дневник", "Наблюдения"]),
            row(&["1", "", ""]),
        );
        let error = fill_legacy_diary_tables(&xml, &medical_case(), true).unwrap_err();
        assert!(error.contains("неоднозначно"));
    }

    #[test]
    fn nonmedical_case_cannot_silently_fill_detected_medical_table() {
        let error =
            fill_legacy_diary_tables(&legacy_table(), &SemanticCase::default(), true).unwrap_err();
        assert!(error.contains("не смог построить коллекцию diaries"));
    }
}
