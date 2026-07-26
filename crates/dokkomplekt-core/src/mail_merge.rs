use crate::{
    canonical_field_id, SemanticAtom, SemanticCase, SemanticRecord, SemanticValue, ValueSource,
};
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MailMergeTable {
    pub delimiter: char,
    pub headers: Vec<String>,
    pub canonical_headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}
pub fn parse_delimited_table(text: &str) -> Result<MailMergeTable, String> {
    let delimiter = detect_delimiter(text);
    let mut rows = parse_rows(text, delimiter)?;
    if rows.is_empty() {
        return Err("Таблица пуста".into());
    }
    let mut headers = rows
        .remove(0)
        .into_iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    if headers.iter().all(String::is_empty) {
        return Err("Заголовки таблицы пусты".into());
    }

    let original_width = headers.len();
    let max_width = rows.iter().map(Vec::len).max().unwrap_or(original_width);
    let mut warnings = Vec::new();
    if max_width > original_width {
        let extra_count = max_width - headers.len();
        warnings.push(format!(
            "В таблице обнаружено {extra_count} дополнительных столбцов без заголовков; данные сохранены как mailmerge.extra_1…"
        ));
        for index in 1..=extra_count {
            headers.push(format!("Дополнительное поле {index}"));
        }
    }

    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    let canonical_headers = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let mut id = if index >= original_width && max_width > original_width {
                format!("mailmerge.extra_{}", index - original_width + 1)
            } else if header.trim().is_empty() {
                warnings.push(format!(
                    "Пустой заголовок столбца {} сохранён как mailmerge.column_{}",
                    index + 1,
                    index + 1
                ));
                format!("mailmerge.column_{}", index + 1)
            } else {
                canonical_field_id(header).unwrap_or_else(|| header.trim().to_string())
            };
            let count = seen.entry(id.clone()).or_default();
            *count += 1;
            if *count > 1 {
                let original = id.clone();
                id = format!("{original}_{}", *count);
                warnings.push(format!(
                    "Повторяющийся заголовок «{header}» сохранён отдельно как {id}"
                ));
            }
            id
        })
        .collect::<Vec<_>>();

    let mut data = Vec::new();
    for mut row in rows {
        if row.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        row.resize(headers.len(), String::new());
        data.push(row);
    }
    Ok(MailMergeTable {
        delimiter,
        headers,
        canonical_headers,
        rows: data,
        warnings,
    })
}

fn detect_delimiter(text: &str) -> char {
    let line = text.lines().next().unwrap_or("");
    ['\t', ';', ',']
        .into_iter()
        .max_by_key(|d| line.chars().filter(|c| c == d).count())
        .unwrap_or(';')
}
fn parse_rows(text: &str, d: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"')
                } else {
                    quoted = false
                }
            } else {
                field.push(c)
            }
        } else {
            match c {
                '"' if field.is_empty() => quoted = true,
                c if c == d => {
                    row.push(std::mem::take(&mut field));
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {}
                _ => field.push(c),
            }
        }
    }
    if quoted {
        return Err("Незакрытая кавычка в CSV/TSV".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row)
    }
    Ok(rows)
}
pub fn case_for_mail_merge_row(
    base: &SemanticCase,
    table: &MailMergeTable,
    row_index: usize,
) -> Result<SemanticCase, String> {
    let row = table
        .rows
        .get(row_index)
        .ok_or_else(|| "Строка отсутствует".to_string())?;
    let mut out = base.clone();
    for (id, value) in table.canonical_headers.iter().zip(row) {
        if id.ends_with("[]") {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        out.values.insert(
            id.clone(),
            SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
        );
    }
    Ok(out)
}
pub fn table_items_collection(table: &MailMergeTable) -> Vec<SemanticRecord> {
    table
        .rows
        .iter()
        .map(|r| {
            table
                .canonical_headers
                .iter()
                .zip(r)
                .map(|(k, v)| (k.clone(), SemanticAtom::Text(v.clone())))
                .collect()
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_csv() {
        let t = parse_delimited_table("ФИО;contract.number\n\"Иванов; Иван\";Д-1").unwrap();
        assert_eq!(t.rows[0][0], "Иванов; Иван");
    }
    #[test]
    fn extra_cells_are_preserved_in_synthetic_columns() {
        let table = parse_delimited_table("ФИО;contract.number\nИванов;Д-1;Не терять").unwrap();
        assert_eq!(table.rows[0].len(), 3);
        assert_eq!(table.rows[0][2], "Не терять");
        assert_eq!(table.canonical_headers[2], "mailmerge.extra_1");
        let case = case_for_mail_merge_row(&SemanticCase::default(), &table, 0).unwrap();
        assert_eq!(case.get("mailmerge.extra_1"), Some("Не терять"));
        assert!(table
            .warnings
            .iter()
            .any(|warning| warning.contains("данные сохранены")));
    }

    #[test]
    fn duplicate_headers_do_not_overwrite_each_other() {
        let table = parse_delimited_table("Комментарий;Комментарий\nПервый;Второй").unwrap();
        assert_ne!(table.canonical_headers[0], table.canonical_headers[1]);
        let case = case_for_mail_merge_row(&SemanticCase::default(), &table, 0).unwrap();
        assert_eq!(case.get(&table.canonical_headers[0]), Some("Первый"));
        assert_eq!(case.get(&table.canonical_headers[1]), Some("Второй"));
    }

    #[test]
    fn row_to_case() {
        let t = parse_delimited_table("ФИО;contract.number\nИванов;Д-1").unwrap();
        let c = case_for_mail_merge_row(&SemanticCase::default(), &t, 0).unwrap();
        assert_eq!(c.get("subject.name"), Some("Иванов"));
    }
}
