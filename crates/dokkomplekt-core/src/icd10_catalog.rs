use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Offline ICD-10 catalog row. The universal core treats ICD as a domain plug-in catalog,
/// not as a hard dependency of the document constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Icd10Row {
    pub code: String,
    pub title: String,
}

#[derive(Debug)]
struct Icd10Catalog {
    rows: Vec<Icd10Row>,
    normalized_codes: Vec<String>,
    normalized_titles: Vec<String>,
}

const NON_F_TSV: &str = include_str!("../../../resources/icd10_full_non_f.tsv");
const F_TSV: &str = include_str!("../../../resources/icd10_f.tsv");

static ICD10_CATALOG: OnceLock<Icd10Catalog> = OnceLock::new();

/// Returns the process-wide immutable catalog.
///
/// Parsing, sorting, de-duplication and normalization are performed exactly once,
/// on first access. Autocomplete calls therefore scan cached strings instead of
/// reparsing roughly fourteen thousand TSV rows for every keystroke.
pub fn load_icd10_rows() -> &'static [Icd10Row] {
    &catalog().rows
}

pub fn search_icd10(query: &str, limit: usize) -> Vec<Icd10Row> {
    let needle = normalize_query(query);
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }

    let catalog = catalog();
    let mut scored: Vec<(usize, usize)> = catalog
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, _row)| {
            let code = &catalog.normalized_codes[index];
            let title = &catalog.normalized_titles[index];
            let score = if code == &needle {
                0
            } else if code.starts_with(&needle) {
                1
            } else if title.starts_with(&needle) {
                2
            } else if title.contains(&needle) {
                3
            } else {
                return None;
            };
            Some((score, index))
        })
        .collect();

    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(catalog.rows[a.1].code.cmp(&catalog.rows[b.1].code))
    });
    scored
        .into_iter()
        .take(limit)
        .map(|(_, index)| catalog.rows[index].clone())
        .collect()
}

fn catalog() -> &'static Icd10Catalog {
    ICD10_CATALOG.get_or_init(build_catalog)
}

fn build_catalog() -> Icd10Catalog {
    let mut rows = Vec::new();
    append_rows(&mut rows, NON_F_TSV, false);
    append_rows(&mut rows, F_TSV, true);
    rows.sort_by(|a, b| a.code.cmp(&b.code));
    rows.dedup_by(|a, b| a.code == b.code);

    let normalized_codes = rows.iter().map(|row| normalize_query(&row.code)).collect();
    let normalized_titles = rows.iter().map(|row| normalize_query(&row.title)).collect();

    Icd10Catalog {
        rows,
        normalized_codes,
        normalized_titles,
    }
}

fn append_rows(rows: &mut Vec<Icd10Row>, source: &str, allow_extra_columns: bool) {
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let pair = if allow_extra_columns {
            let mut columns = line.split('\t');
            columns.next().zip(columns.next())
        } else {
            line.split_once('\t')
        };
        let Some((code, title)) = pair else {
            continue;
        };
        let code = code.trim();
        let title = title.trim();
        if !code.is_empty() && !title.is_empty() {
            rows.push(Icd10Row {
                code: code.to_string(),
                title: title.to_string(),
            });
        }
    }
}

fn normalize_query(text: &str) -> String {
    text.trim().to_lowercase().replace('ё', "е")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_non_f_catalog_is_bundled() {
        let rows = load_icd10_rows();
        assert!(
            rows.len() > 10_000,
            "ICD-10 non-F TSV must be included, not replaced by a tiny stub"
        );
        assert!(rows.iter().any(|r| r.code.starts_with("A00")));
        assert!(rows.iter().any(|r| r.code.starts_with('Z')));
    }

    #[test]
    fn catalog_is_initialized_once_and_reused() {
        let first = load_icd10_rows();
        let second = load_icd10_rows();
        assert!(std::ptr::eq(first, second));
        assert_eq!(first.as_ptr(), second.as_ptr());
    }

    #[test]
    fn f_catalog_has_domain_rows() {
        let rows = search_icd10("F20", 5);
        assert!(rows.iter().any(|r| r.code == "F20"));
        assert!(rows.iter().any(|r| r.code == "F20.0"));
    }

    #[test]
    fn search_uses_normalized_cached_titles() {
        let rows = search_icd10("холера", 3);
        assert!(rows.iter().any(|r| r.code.starts_with("A00")));
        assert!(search_icd10("ignored", 0).is_empty());
    }
}
