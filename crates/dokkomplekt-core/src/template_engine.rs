//! Strict universal template language: values, conditions, collections, formulas, blocks and counters.
use crate::{
    canonical_field_candidates, escape_xml, is_valid_field_id, RenderResult, SemanticAtom,
    SemanticCase, SemanticRecord,
};
use chrono::{Duration, NaiveDate};
use dokkomplekt_morph::{
    date_to_words_ru, decline_person_name, decline_position, format_money_ru, format_phone_ru,
    money_to_words_ru, number_to_words_ru, GrammaticalCase,
};
use dokkomplekt_refdata::add_working_days_ru;
use std::collections::{BTreeMap, BTreeSet};
const MAX_DEPTH: usize = 16;
// Literal template delimiters are represented internally with private-use
// sentinels. This lets users write `\{{` and `\}}` in a template when the
// finished document must contain real double braces (source code, LaTeX, a
// nested template, etc.) without those braces being interpreted as fields.
const ESCAPED_OPEN_SENTINEL: &str = "\u{e000}\u{e001}";
const ESCAPED_CLOSE_SENTINEL: &str = "\u{e002}\u{e003}";
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Text(String),
    Value(String),
    If {
        cond: String,
        yes: Vec<Node>,
        no: Vec<Node>,
        unless: bool,
    },
    Each {
        collection: String,
        body: Vec<Node>,
    },
    Expr(String),
    Sum(String),
    Count(String),
    Block(String),
    Counter {
        key: String,
        format: String,
    },
    Image(String),
}
#[derive(Debug, Clone)]
struct Parsed {
    nodes: Vec<Node>,
    errors: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CounterRequest {
    pub key: String,
    pub format: String,
}
pub fn inspect_template_syntax(t: &str) -> Vec<String> {
    parse(t).errors
}
pub fn template_uses_advanced_syntax(t: &str) -> bool {
    [
        "{{#if",
        "{{#unless",
        "{{#each",
        "{{=",
        "{{sum ",
        "{{count ",
        "{{block ",
        "{{counter ",
        "{{image ",
        "|",
    ]
    .iter()
    .any(|x| t.contains(x))
}
pub fn template_counter_requests(t: &str) -> Vec<CounterRequest> {
    let mut m = BTreeMap::new();
    collect_counters(&parse(t).nodes, &mut m);
    m.into_values().collect()
}

pub fn template_image_requests(t: &str) -> Vec<String> {
    let mut fields = BTreeSet::new();
    collect_images(&parse(t).nodes, &mut fields);
    fields.into_iter().collect()
}

fn collect_images(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Image(field_id) => {
                out.insert(field_id.clone());
            }
            Node::If { yes, no, .. } => {
                collect_images(yes, out);
                collect_images(no, out);
            }
            Node::Each { body, .. } => collect_images(body, out),
            _ => {}
        }
    }
}
fn collect_counters(nodes: &[Node], out: &mut BTreeMap<String, CounterRequest>) {
    for n in nodes {
        match n {
            Node::Counter { key, format } => {
                out.entry(key.clone()).or_insert(CounterRequest {
                    key: key.clone(),
                    format: format.clone(),
                });
            }
            Node::If { yes, no, .. } => {
                collect_counters(yes, out);
                collect_counters(no, out)
            }
            Node::Each { body, .. } => collect_counters(body, out),
            _ => {}
        }
    }
}
pub fn format_counter_value(format: &str, seq: u64, year: i32) -> String {
    let mut out = format.replace("{YYYY}", &year.to_string());
    for w in (1..=12).rev() {
        let token = format!("{{{}}}", "N".repeat(w));
        out = out.replace(&token, &format!("{seq:0w$}"));
    }
    out.replace("{N}", &seq.to_string())
}
pub fn template_field_references(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect_refs(&parse(t).nodes, &mut out);
    out
}

/// Collections whose contents can affect the rendered template.
///
/// This is used by the granular resume engine so an unrelated collection change
/// does not invalidate every document in a package.
pub fn template_collection_references(t: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    collect_collection_refs(&parse(t).nodes, &mut out);
    out.into_iter().collect()
}

/// Named clause blocks whose contents can affect the rendered template.
pub fn template_block_references(t: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    collect_block_refs(&parse(t).nodes, &mut out);
    out.into_iter().collect()
}

fn collect_collection_refs(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Each { collection, body } => {
                if !collection.trim().is_empty() {
                    out.insert(collection.trim().to_string());
                }
                collect_collection_refs(body, out);
            }
            Node::Sum(path) => {
                if let Some((collection, _)) = path.trim().split_once('.') {
                    if !collection.trim().is_empty() {
                        out.insert(collection.trim().to_string());
                    }
                }
            }
            Node::Count(collection) => {
                if !collection.trim().is_empty() {
                    out.insert(collection.trim().to_string());
                }
            }
            Node::If { yes, no, .. } => {
                collect_collection_refs(yes, out);
                collect_collection_refs(no, out);
            }
            _ => {}
        }
    }
}

fn collect_block_refs(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Block(id) => {
                if !id.trim().is_empty() {
                    out.insert(id.trim().to_string());
                }
            }
            Node::If { yes, no, .. } => {
                collect_block_refs(yes, out);
                collect_block_refs(no, out);
            }
            Node::Each { body, .. } => collect_block_refs(body, out),
            _ => {}
        }
    }
}
fn collect_refs(nodes: &[Node], out: &mut Vec<String>) {
    for n in nodes {
        match n {
            Node::Value(v) => insert_ref(
                split_pipeline(v).first().map(String::as_str).unwrap_or(""),
                out,
            ),
            Node::If { cond, yes, no, .. } => {
                for token in tokens(cond) {
                    insert_ref(&token, out)
                }
                collect_refs(yes, out);
                collect_refs(no, out)
            }
            Node::Expr(e) => {
                for token in tokens(e) {
                    insert_ref(&token, out)
                }
            }
            Node::Each { body, .. } => collect_refs(body, out),
            Node::Image(field_id) => insert_ref(field_id, out),
            _ => {}
        }
    }
}
fn insert_ref(raw: &str, out: &mut Vec<String>) {
    let x = raw.trim();
    let l = x.to_lowercase();
    if x.is_empty()
        || x.starts_with('@')
        || x.starts_with("item.")
        || x.starts_with("this.")
        || parse_number(x).is_some()
        || parse_date(x).is_some()
        || matches!(
            l.as_str(),
            "true"
                | "false"
                | "да"
                | "нет"
                | "and"
                | "or"
                | "not"
                | "и"
                | "или"
                | "не"
                | "days"
                | "дней"
                | "working_days"
                | "рабочих_дней"
                | "workdays"
        )
    {
        return;
    }
    if !out.iter().any(|v| v == x) {
        out.push(x.to_string())
    }
}
pub fn render_advanced_text_template(t: &str, c: &SemanticCase, strict: bool) -> RenderResult {
    render(t, c, strict, |v| v.to_string())
}
pub fn render_advanced_xml_template(t: &str, c: &SemanticCase, strict: bool) -> RenderResult {
    render(t, c, strict, escape_xml)
}
fn render(t: &str, c: &SemanticCase, strict: bool, esc: fn(&str) -> String) -> RenderResult {
    let p = parse(t);
    let mut st = State {
        case: c,
        strict,
        esc,
        scopes: Vec::new(),
        missing: BTreeSet::new(),
        unknown: BTreeSet::new(),
        warnings: BTreeSet::new(),
        errors: p.errors.into_iter().collect(),
        block_stack: Vec::new(),
    };
    let output_text = restore_escaped_delimiters(&st.render_nodes(&p.nodes, 0));
    let references = template_field_references(t);
    let order = |field: &String| {
        references
            .iter()
            .position(|reference| {
                reference == field
                    || canonical_field_candidates(reference)
                        .iter()
                        .any(|candidate| candidate == field)
            })
            .unwrap_or(usize::MAX)
    };
    let mut missing_fields = st.missing.into_iter().collect::<Vec<_>>();
    missing_fields.sort_by_key(&order);
    let mut unknown_fields = st.unknown.into_iter().collect::<Vec<_>>();
    unknown_fields.sort_by_key(&order);
    RenderResult {
        output_text,
        missing_fields,
        unknown_fields,
        warnings: st.warnings.into_iter().collect(),
        template_errors: st.errors.into_iter().collect(),
    }
}
struct State<'a> {
    case: &'a SemanticCase,
    strict: bool,
    esc: fn(&str) -> String,
    scopes: Vec<BTreeMap<String, SemanticAtom>>,
    missing: BTreeSet<String>,
    unknown: BTreeSet<String>,
    warnings: BTreeSet<String>,
    errors: BTreeSet<String>,
    block_stack: Vec<String>,
}
impl State<'_> {
    fn render_nodes(&mut self, nodes: &[Node], depth: usize) -> String {
        if depth > MAX_DEPTH {
            self.errors
                .insert("Превышена максимальная глубина шаблона".into());
            return String::new();
        }
        let mut out = String::new();
        for n in nodes {
            match n {
                Node::Text(x) => out.push_str(x),
                Node::Value(x) => out.push_str(&self.render_value(x)),
                Node::If {
                    cond,
                    yes,
                    no,
                    unless,
                } => {
                    let ok = match self.condition(cond) {
                        Ok(v) => v,
                        Err(e) => {
                            self.errors.insert(e);
                            false
                        }
                    };
                    out.push_str(
                        &self.render_nodes(if ok ^ *unless { yes } else { no }, depth + 1),
                    );
                }
                Node::Each { collection, body } => {
                    let Some(rows) = self
                        .case
                        .collection(collection)
                        .map(<[SemanticRecord]>::to_vec)
                    else {
                        self.missing.insert(format!("collection.{collection}"));
                        continue;
                    };
                    let alias = singular(collection);
                    for (i, row) in rows.iter().enumerate() {
                        let mut scope = BTreeMap::new();
                        scope.insert("@index".into(), SemanticAtom::Integer(i as i64));
                        scope.insert("@number".into(), SemanticAtom::Integer(i as i64 + 1));
                        for (k, v) in row {
                            scope.insert(k.clone(), v.clone());
                            scope.insert(format!("item.{k}"), v.clone());
                            scope.insert(format!("this.{k}"), v.clone());
                            scope.insert(format!("{alias}.{k}"), v.clone());
                        }
                        self.scopes.push(scope);
                        out.push_str(&self.render_nodes(body, depth + 1));
                        self.scopes.pop();
                    }
                }
                Node::Expr(e) => match self.eval_expr(e) {
                    Ok(v) => out.push_str(&(self.esc)(&v)),
                    Err(e) => {
                        self.errors.insert(e);
                    }
                },
                Node::Sum(path) => match self.sum(path) {
                    Ok(v) => out.push_str(&(self.esc)(&v)),
                    Err(e) => {
                        self.errors.insert(e);
                    }
                },
                Node::Count(col) => {
                    if let Some(rows) = self.case.collection(col) {
                        out.push_str(&rows.len().to_string())
                    } else {
                        self.missing.insert(format!("collection.{col}"));
                    }
                }
                Node::Block(id) => out.push_str(&self.render_block(id, depth)),
                Node::Counter { key, .. } => {
                    let id = format!("counter.{key}");
                    if let Some(v) = self.lookup(&id).or_else(|| self.lookup(key)) {
                        out.push_str(&(self.esc)(&v.as_text()))
                    } else {
                        self.missing.insert(id);
                    }
                }
                Node::Image(field_id) => {
                    if self.case.is_skipped(field_id) {
                        continue;
                    }
                    if self
                        .lookup(field_id)
                        .map(|value| !value.as_text().trim().is_empty())
                        .unwrap_or(false)
                    {
                        out.push_str(&format!("[[DOKKOMPLEKT_IMAGE:{field_id}]]"));
                    } else {
                        self.missing.insert(field_id.clone());
                    }
                }
            }
        }
        out
    }
    fn render_value(&mut self, raw: &str) -> String {
        let parts = split_pipeline(raw);
        let id = parts.first().map(String::as_str).unwrap_or("").trim();
        let mut value = if let Some(value) = self.lookup(id).map(|v| v.as_text()) {
            value
        } else {
            let candidates = canonical_field_candidates(id);
            let vals = candidates
                .iter()
                .filter_map(|candidate| self.lookup(candidate))
                .map(|value| value.as_text())
                .collect::<BTreeSet<_>>();
            if vals.len() == 1 {
                vals.into_iter().next().unwrap_or_default()
            } else {
                if candidates.is_empty() && !is_valid_field_id(id) {
                    self.unknown.insert(id.to_string());
                } else {
                    self.missing.insert(
                        candidates
                            .first()
                            .cloned()
                            .unwrap_or_else(|| id.to_string()),
                    );
                }
                return if self.strict {
                    format!("{{{{{raw}}}}}")
                } else {
                    String::new()
                };
            }
        };
        for modifier in parts.iter().skip(1) {
            match apply_modifier(&value, modifier) {
                Ok(v) => value = v,
                Err(e) => {
                    self.errors.insert(format!("Модификатор «{modifier}»: {e}"));
                }
            }
        }
        (self.esc)(&value)
    }
    fn lookup(&self, id: &str) -> Option<SemanticAtom> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(id) {
                return Some(v.clone());
            }
        }
        if self.case.is_skipped(id) {
            return Some(SemanticAtom::Text(String::new()));
        }
        self.case.get(id).map(|v| SemanticAtom::Text(v.to_string()))
    }
    fn condition(&mut self, s: &str) -> Result<bool, String> {
        self.track_expression_references(s);
        eval_condition(s, |id| self.lookup(id))
    }
    fn eval_expr(&mut self, s: &str) -> Result<String, String> {
        self.track_formula_references(s);
        eval_expression(s, |id| self.lookup_expression_value(id))
    }
    fn lookup_expression_value(&self, id: &str) -> Option<SemanticAtom> {
        if let Some(value) = self.lookup(id) {
            return Some(value);
        }
        let values = canonical_field_candidates(id)
            .into_iter()
            .filter_map(|candidate| self.lookup(&candidate))
            .collect::<Vec<_>>();
        let first = values.first()?.clone();
        values
            .iter()
            .all(|value| value.as_text() == first.as_text())
            .then_some(first)
    }
    fn track_formula_references(&mut self, expression: &str) {
        let Ok(expression_tokens) = lex_expression(expression) else {
            // The evaluator reports the exact lexical error. Do not manufacture
            // additional missing-field errors from a malformed token stream.
            return;
        };
        for token in expression_tokens {
            let ExprToken::Identifier(id) = token else {
                continue;
            };
            if parse_date_unit(&id).is_some()
                || id.starts_with('@')
                || id.starts_with("item.")
                || id.starts_with("this.")
                || self.lookup_expression_value(&id).is_some()
            {
                continue;
            }
            let candidates = canonical_field_candidates(&id);
            if candidates.is_empty() && !is_valid_field_id(&id) {
                self.unknown.insert(id);
            } else {
                self.missing.insert(id);
            }
        }
    }
    fn track_expression_references(&mut self, expression: &str) {
        for id in tokens(expression) {
            if is_expression_literal(&id) || self.lookup(&id).is_some() {
                continue;
            }
            let candidates = canonical_field_candidates(&id);
            if candidates
                .iter()
                .any(|candidate| self.lookup(candidate).is_some())
            {
                continue;
            }
            if candidates.is_empty() && !is_valid_field_id(&id) {
                self.unknown.insert(id);
            } else {
                // Keep the identifier exactly as the template author wrote it. This makes
                // strict-mode diagnostics actionable and prevents a missing field in a
                // condition or formula from silently passing under a different alias.
                self.missing.insert(id);
            }
        }
    }
    fn sum(&self, path: &str) -> Result<String, String> {
        let (col, field) = path
            .trim()
            .split_once('.')
            .ok_or_else(|| format!("Некорректная сумма: {path}"))?;
        let rows = self
            .case
            .collection(col)
            .ok_or_else(|| format!("Коллекция {col} отсутствует"))?;
        let mut sum = 0i128;
        for row in rows {
            if let Some(v) = row.get(field) {
                sum += parse_decimal_scaled(&v.as_text())?;
            }
        }
        Ok(format_scaled(sum))
    }
    fn render_block(&mut self, id: &str, depth: usize) -> String {
        if self.block_stack.iter().any(|x| x == id) {
            self.errors
                .insert(format!("Циклическая ссылка блока: {id}"));
            return String::new();
        }
        let Some(content) = self.case.blocks.get(id).cloned() else {
            self.missing.insert(format!("block.{id}"));
            return String::new();
        };
        self.block_stack.push(id.to_string());
        let p = parse(&content);
        for e in p.errors {
            self.errors.insert(format!("Блок {id}: {e}"));
        }
        let out = self.render_nodes(&p.nodes, depth + 1);
        self.block_stack.pop();
        out
    }
}
fn parse(t: &str) -> Parsed {
    let (tokens, mut errors) = tokenize(t);
    let (mut nodes, idx, stop, mut parser_errors) = parse_nodes(&tokens, 0, &[]);
    errors.append(&mut parser_errors);
    if idx < tokens.len() {
        errors.push(format!(
            "Неожиданный закрывающий тег: {}",
            stop.unwrap_or_default()
        ))
    }
    Parsed {
        nodes: std::mem::take(&mut nodes),
        errors,
    }
}
fn tokenize(t: &str) -> (Vec<(bool, String)>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut literal = String::new();
    let mut cursor = 0usize;

    while cursor < t.len() {
        let rest = &t[cursor..];
        if rest.starts_with("\\{{") {
            literal.push_str(ESCAPED_OPEN_SENTINEL);
            cursor += 3;
            continue;
        }
        if rest.starts_with("\\}}") {
            literal.push_str(ESCAPED_CLOSE_SENTINEL);
            cursor += 3;
            continue;
        }
        if let Some(after) = rest.strip_prefix("{{") {
            if !literal.is_empty() {
                out.push((false, std::mem::take(&mut literal)));
            }
            if let Some(end) = after.find("}}") {
                out.push((true, after[..end].trim().to_string()));
                cursor += 2 + end + 2;
                continue;
            }
            literal.push_str(rest);
            errors.push("Незакрытый тег шаблона «{{»".into());
            break;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        literal.push(ch);
        cursor += ch.len_utf8();
    }

    if !literal.is_empty() {
        out.push((false, literal));
    }
    (out, errors)
}

fn restore_escaped_delimiters(text: &str) -> String {
    text.replace(ESCAPED_OPEN_SENTINEL, "{{")
        .replace(ESCAPED_CLOSE_SENTINEL, "}}")
}
fn parse_nodes(
    tokens: &[(bool, String)],
    mut i: usize,
    stops: &[&str],
) -> (Vec<Node>, usize, Option<String>, Vec<String>) {
    let mut nodes = Vec::new();
    let mut errors = Vec::new();
    while i < tokens.len() {
        let (is_tag, val) = &tokens[i];
        if !is_tag {
            nodes.push(Node::Text(val.clone()));
            i += 1;
            continue;
        }
        let tag = val.trim();
        if stops.contains(&tag) {
            return (nodes, i + 1, Some(tag.to_string()), errors);
        }
        if let Some(cond) = tag.strip_prefix("#if ") {
            let (yes, next, stop, mut e) = parse_nodes(tokens, i + 1, &["else", "/if"]);
            errors.append(&mut e);
            let (mut no, mut end) = (Vec::new(), next);
            if stop.as_deref() == Some("else") {
                let (n, nx, st, mut e2) = parse_nodes(tokens, next, &["/if"]);
                errors.append(&mut e2);
                no = n;
                end = nx;
                if st.as_deref() != Some("/if") {
                    errors.push("Незакрытый {{#if}}".into())
                }
            } else if stop.as_deref() != Some("/if") {
                errors.push("Незакрытый {{#if}}".into())
            }
            nodes.push(Node::If {
                cond: cond.trim().to_string(),
                yes,
                no,
                unless: false,
            });
            i = end;
            continue;
        }
        if let Some(cond) = tag.strip_prefix("#unless ") {
            let (yes, next, stop, mut e) = parse_nodes(tokens, i + 1, &["else", "/unless"]);
            errors.append(&mut e);
            let (mut no, mut end) = (Vec::new(), next);
            if stop.as_deref() == Some("else") {
                let (n, nx, st, mut e2) = parse_nodes(tokens, next, &["/unless"]);
                errors.append(&mut e2);
                no = n;
                end = nx;
                if st.as_deref() != Some("/unless") {
                    errors.push("Незакрытый {{#unless}}".into())
                }
            } else if stop.as_deref() != Some("/unless") {
                errors.push("Незакрытый {{#unless}}".into())
            }
            nodes.push(Node::If {
                cond: cond.trim().to_string(),
                yes,
                no,
                unless: true,
            });
            i = end;
            continue;
        }
        if let Some(col) = tag.strip_prefix("#each ") {
            let (body, next, stop, mut e) = parse_nodes(tokens, i + 1, &["/each"]);
            errors.append(&mut e);
            if stop.as_deref() != Some("/each") {
                errors.push("Незакрытый {{#each}}".into())
            }
            nodes.push(Node::Each {
                collection: col.trim().to_string(),
                body,
            });
            i = next;
            continue;
        }
        if matches!(tag, "else" | "/if" | "/unless" | "/each") {
            errors.push(format!("Лишний тег {{{{{tag}}}}}"));
            i += 1;
            continue;
        }
        let node = if let Some(x) = tag.strip_prefix('=') {
            Node::Expr(x.trim().to_string())
        } else if let Some(x) = tag.strip_prefix("sum ") {
            Node::Sum(x.trim().to_string())
        } else if let Some(x) = tag.strip_prefix("count ") {
            Node::Count(x.trim().to_string())
        } else if let Some(x) = tag.strip_prefix("block ") {
            Node::Block(x.trim().to_string())
        } else if let Some(x) = tag.strip_prefix("counter ") {
            let (key, format) = parse_counter(x);
            Node::Counter { key, format }
        } else if let Some(x) = tag.strip_prefix("image ") {
            let field_id = x.trim();
            if field_id.is_empty() || !is_valid_field_id(field_id) {
                errors.push(format!("Некорректное поле изображения: {field_id}"));
            }
            Node::Image(field_id.to_string())
        } else {
            Node::Value(tag.to_string())
        };
        nodes.push(node);
        i += 1
    }
    (nodes, i, None, errors)
}
fn parse_counter(s: &str) -> (String, String) {
    let key = s
        .split_whitespace()
        .next()
        .unwrap_or("document.number")
        .to_string();
    let format = extract_quoted_arg(s, "format").unwrap_or_else(|| "{YYYY}/{NNN}".into());
    (key, format)
}
fn extract_quoted_arg(s: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=");
    let p = s.find(&marker)? + marker.len();
    let q = s.as_bytes().get(p).copied()? as char;
    if q != '"' && q != '\'' {
        return None;
    }
    let tail = &s[p + 1..];
    let end = tail.find(q)?;
    Some(tail[..end].to_string())
}
fn split_pipeline(s: &str) -> Vec<String> {
    s.split('|').map(|x| x.trim().to_string()).collect()
}
fn singular(c: &str) -> String {
    if let Some(x) = c.strip_suffix("ies") {
        format!("{x}y")
    } else if let Some(x) = c.strip_suffix('s') {
        x.to_string()
    } else {
        c.to_string()
    }
}
fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in s.chars() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            quote = Some(ch);
        } else if ch.is_alphanumeric() || matches!(ch, '_' | '.' | '@' | '-') {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn is_expression_literal(raw: &str) -> bool {
    let lower = raw.trim().to_lowercase();
    raw.is_empty()
        || raw.starts_with('@')
        || raw.starts_with("item.")
        || raw.starts_with("this.")
        || parse_number(raw).is_some()
        || parse_date(raw).is_some()
        || matches!(
            lower.as_str(),
            "true"
                | "false"
                | "да"
                | "нет"
                | "and"
                | "or"
                | "not"
                | "и"
                | "или"
                | "не"
                | "days"
                | "дней"
                | "working_days"
                | "рабочих_дней"
                | "workdays"
        )
}
fn truthy(v: Option<SemanticAtom>) -> bool {
    match v {
        None => false,
        Some(SemanticAtom::Boolean(v)) => v,
        Some(SemanticAtom::Integer(v)) => v != 0,
        Some(v) => {
            let s = v.as_text();
            !s.trim().is_empty()
                && !matches!(
                    s.trim().to_lowercase().as_str(),
                    "false" | "нет" | "0" | "null"
                )
        }
    }
}
fn split_once_outside_quotes<'a, 'b>(
    s: &'a str,
    operators: &[&'b str],
) -> Option<(&'a str, &'b str, &'a str)> {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in s.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        for operator in operators {
            if s[index..].starts_with(operator) {
                return Some((&s[..index], *operator, &s[index + operator.len()..]));
            }
        }
    }
    None
}

fn eval_condition(
    s: &str,
    lookup: impl Fn(&str) -> Option<SemanticAtom> + Copy,
) -> Result<bool, String> {
    let s = s.trim();
    if let Some((left, _, right)) = split_once_outside_quotes(s, &[" or ", " или ", " || "]) {
        return Ok(eval_condition(left, lookup)? || eval_condition(right, lookup)?);
    }
    if let Some((left, _, right)) = split_once_outside_quotes(s, &[" and ", " и ", " && "]) {
        return Ok(eval_condition(left, lookup)? && eval_condition(right, lookup)?);
    }
    if let Some(rest) = s
        .strip_prefix("not ")
        .or_else(|| s.strip_prefix("не "))
        .or_else(|| s.strip_prefix('!'))
    {
        return Ok(!eval_condition(rest, lookup)?);
    }
    if let Some((left, operator, right)) =
        split_once_outside_quotes(s, &["==", "!=", ">=", "<=", ">", "<"])
    {
        let left_value = operand(left, &lookup);
        let right_value = operand(right, &lookup);
        let ordering = compare(&left_value, &right_value);
        return Ok(match operator {
            "==" => left_value == right_value,
            "!=" => left_value != right_value,
            ">" => ordering.is_gt(),
            "<" => ordering.is_lt(),
            ">=" => !ordering.is_lt(),
            "<=" => !ordering.is_gt(),
            _ => false,
        });
    }
    Ok(truthy(lookup(s)))
}
fn operand<F>(s: &str, lookup: &F) -> String
where
    F: Fn(&str) -> Option<SemanticAtom>,
{
    let x = s.trim();
    if x.len() >= 2
        && ((x.starts_with('"') && x.ends_with('"')) || (x.starts_with('\'') && x.ends_with('\'')))
    {
        x[1..x.len() - 1].to_string()
    } else {
        lookup(x)
            .map(|v| v.as_text())
            .unwrap_or_else(|| x.to_string())
    }
}
fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    if let (Ok(x), Ok(y)) = (parse_decimal_scaled(a), parse_decimal_scaled(b)) {
        x.cmp(&y)
    } else if let (Some(x), Some(y)) = (parse_date(a), parse_date(b)) {
        x.cmp(&y)
    } else {
        a.cmp(b)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateUnit {
    CalendarDays,
    WorkingDays,
}

#[derive(Debug, Clone, PartialEq)]
enum ExprToken {
    Number(String),
    Date(String),
    Identifier(String),
    StringLiteral(String),
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
}

#[derive(Debug, Clone, PartialEq)]
enum ExprValue {
    Number(i128),
    Date(NaiveDate),
    Text(String),
    Duration { amount: i32, unit: DateUnit },
}

fn eval_expression<F>(s: &str, lookup: F) -> Result<String, String>
where
    F: Fn(&str) -> Option<SemanticAtom>,
{
    let tokens = lex_expression(s)?;
    if tokens.is_empty() {
        return Err("Пустая формула".into());
    }
    let mut parser = ExpressionParser {
        tokens: &tokens,
        position: 0,
        lookup: &lookup,
    };
    let value = parser.parse_add_sub()?;
    if let Some(token) = parser.peek() {
        return Err(format!(
            "Лишняя часть формулы после позиции {}: {}",
            parser.position + 1,
            expression_token_label(token)
        ));
    }
    expression_value_to_text(value)
}

struct ExpressionParser<'a, F> {
    tokens: &'a [ExprToken],
    position: usize,
    lookup: &'a F,
}

impl<F> ExpressionParser<'_, F>
where
    F: Fn(&str) -> Option<SemanticAtom>,
{
    fn peek(&self) -> Option<&ExprToken> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<ExprToken> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn parse_add_sub(&mut self) -> Result<ExprValue, String> {
        let mut left = self.parse_mul_div()?;
        loop {
            let operator = match self.peek() {
                Some(ExprToken::Plus) => '+',
                Some(ExprToken::Minus) => '-',
                _ => break,
            };
            self.position += 1;
            let right = self.parse_mul_div()?;
            left = apply_expression_binary(left, operator, right)?;
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<ExprValue, String> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = match self.peek() {
                Some(ExprToken::Star) => '*',
                Some(ExprToken::Slash) => '/',
                _ => break,
            };
            self.position += 1;
            let right = self.parse_unary()?;
            left = apply_expression_binary(left, operator, right)?;
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<ExprValue, String> {
        let sign = match self.peek() {
            Some(ExprToken::Plus) => {
                self.position += 1;
                1
            }
            Some(ExprToken::Minus) => {
                self.position += 1;
                -1
            }
            _ => 1,
        };
        let mut value = self.parse_primary()?;
        if sign < 0 {
            value = match value {
                ExprValue::Number(number) => ExprValue::Number(
                    number
                        .checked_neg()
                        .ok_or_else(|| "Переполнение числа".to_string())?,
                ),
                ExprValue::Duration { amount, unit } => ExprValue::Duration {
                    amount: amount
                        .checked_neg()
                        .ok_or_else(|| "Переполнение количества дней".to_string())?,
                    unit,
                },
                _ => return Err("Унарный минус допустим только для числа".into()),
            };
        }
        self.parse_optional_date_unit(value)
    }

    fn parse_primary(&mut self) -> Result<ExprValue, String> {
        match self.next() {
            Some(ExprToken::Number(value)) => parse_decimal_scaled(&value).map(ExprValue::Number),
            Some(ExprToken::Date(value)) => parse_date(&value)
                .map(ExprValue::Date)
                .ok_or_else(|| format!("Некорректная дата в формуле: {value}")),
            Some(ExprToken::StringLiteral(value)) => Ok(ExprValue::Text(value)),
            Some(ExprToken::Identifier(id)) => {
                let value = (self.lookup)(&id)
                    .ok_or_else(|| format!("Не найдено поле формулы: {id}"))?
                    .as_text();
                if let Some(date) = parse_date(&value) {
                    Ok(ExprValue::Date(date))
                } else if let Ok(number) = parse_decimal_scaled(&value) {
                    Ok(ExprValue::Number(number))
                } else {
                    Ok(ExprValue::Text(value))
                }
            }
            Some(ExprToken::LeftParen) => {
                let value = self.parse_add_sub()?;
                match self.next() {
                    Some(ExprToken::RightParen) => Ok(value),
                    Some(other) => Err(format!(
                        "Ожидалась закрывающая скобка, найдено: {}",
                        expression_token_label(&other)
                    )),
                    None => Err("Незакрытая скобка в формуле".into()),
                }
            }
            Some(token) => Err(format!(
                "Ожидалось значение формулы, найдено: {}",
                expression_token_label(&token)
            )),
            None => Err("Формула неожиданно закончилась".into()),
        }
    }

    fn parse_optional_date_unit(&mut self, value: ExprValue) -> Result<ExprValue, String> {
        let Some(ExprToken::Identifier(unit)) = self.peek() else {
            return Ok(value);
        };
        let unit = unit.clone();
        let Some(unit_kind) = parse_date_unit(&unit) else {
            return Ok(value);
        };
        self.position += 1;
        let ExprValue::Number(number) = value else {
            return Err(format!(
                "Единица «{unit}» допустима только после целого количества дней"
            ));
        };
        if number % 10_000 != 0 {
            return Err("Количество дней должно быть целым числом".into());
        }
        let days = i32::try_from(number / 10_000)
            .map_err(|_| "Количество дней выходит за допустимый диапазон".to_string())?;
        Ok(ExprValue::Duration {
            amount: days,
            unit: unit_kind,
        })
    }
}

fn parse_date_unit(value: &str) -> Option<DateUnit> {
    match value.trim().to_lowercase().as_str() {
        "days" | "day" | "дней" | "день" | "дня" => Some(DateUnit::CalendarDays),
        "working_days" | "workdays" | "рабочих_дней" | "рабочий_день" | "рабочихдней" => {
            Some(DateUnit::WorkingDays)
        }
        _ => None,
    }
}

fn apply_expression_binary(
    left: ExprValue,
    operator: char,
    right: ExprValue,
) -> Result<ExprValue, String> {
    match (left, operator, right) {
        (ExprValue::Number(a), '+', ExprValue::Number(b)) => a
            .checked_add(b)
            .map(ExprValue::Number)
            .ok_or_else(|| "Переполнение числа".to_string()),
        (ExprValue::Number(a), '-', ExprValue::Number(b)) => a
            .checked_sub(b)
            .map(ExprValue::Number)
            .ok_or_else(|| "Переполнение числа".to_string()),
        (ExprValue::Number(a), '*', ExprValue::Number(b)) => {
            let product = a
                .checked_mul(b)
                .ok_or_else(|| "Переполнение числа".to_string())?;
            Ok(ExprValue::Number(div_round_half_away(product, 10_000)?))
        }
        (ExprValue::Number(_), '/', ExprValue::Number(0)) => Err("Деление на ноль".into()),
        (ExprValue::Number(a), '/', ExprValue::Number(b)) => {
            let numerator = a
                .checked_mul(10_000)
                .ok_or_else(|| "Переполнение числа".to_string())?;
            Ok(ExprValue::Number(div_round_half_away(numerator, b)?))
        }
        (ExprValue::Date(date), '+', ExprValue::Duration { amount, unit }) => {
            add_expression_days(date, amount, unit).map(ExprValue::Date)
        }
        (ExprValue::Date(date), '-', ExprValue::Duration { amount, unit }) => {
            let amount = amount
                .checked_neg()
                .ok_or_else(|| "Переполнение количества дней".to_string())?;
            add_expression_days(date, amount, unit).map(ExprValue::Date)
        }
        (ExprValue::Date(date), '+', ExprValue::Number(number)) => {
            let amount = scaled_integer_days(number)?;
            add_expression_days(date, amount, DateUnit::CalendarDays).map(ExprValue::Date)
        }
        (ExprValue::Date(date), '-', ExprValue::Number(number)) => {
            let amount = scaled_integer_days(number)?
                .checked_neg()
                .ok_or_else(|| "Переполнение количества дней".to_string())?;
            add_expression_days(date, amount, DateUnit::CalendarDays).map(ExprValue::Date)
        }
        (ExprValue::Date(left), '-', ExprValue::Date(right)) => {
            let days = left.signed_duration_since(right).num_days();
            let scaled = i128::from(days)
                .checked_mul(10_000)
                .ok_or_else(|| "Переполнение числа".to_string())?;
            Ok(ExprValue::Number(scaled))
        }
        (ExprValue::Duration { amount, unit }, '+', ExprValue::Date(date)) => {
            add_expression_days(date, amount, unit).map(ExprValue::Date)
        }
        (left, operator, right) => Err(format!(
            "Оператор «{operator}» неприменим к {} и {}",
            expression_value_kind(&left),
            expression_value_kind(&right)
        )),
    }
}

fn scaled_integer_days(number: i128) -> Result<i32, String> {
    if number % 10_000 != 0 {
        return Err("Количество дней должно быть целым числом".into());
    }
    i32::try_from(number / 10_000)
        .map_err(|_| "Количество дней выходит за допустимый диапазон".to_string())
}

fn add_expression_days(date: NaiveDate, amount: i32, unit: DateUnit) -> Result<NaiveDate, String> {
    match unit {
        DateUnit::CalendarDays => date
            .checked_add_signed(Duration::days(i64::from(amount)))
            .ok_or_else(|| "Переполнение даты".to_string()),
        DateUnit::WorkingDays => {
            add_working_days_ru(date, amount).map_err(|error| error.to_string())
        }
    }
}

fn expression_value_to_text(value: ExprValue) -> Result<String, String> {
    match value {
        ExprValue::Number(value) => Ok(format_scaled(value)),
        ExprValue::Date(value) => Ok(value.format("%d.%m.%Y").to_string()),
        ExprValue::Text(value) => Ok(value),
        ExprValue::Duration { .. } => {
            Err("Формула не может завершаться только количеством дней".into())
        }
    }
}

fn expression_value_kind(value: &ExprValue) -> &'static str {
    match value {
        ExprValue::Number(_) => "числу",
        ExprValue::Date(_) => "дате",
        ExprValue::Text(_) => "тексту",
        ExprValue::Duration { .. } => "количеству дней",
    }
}

fn expression_token_label(token: &ExprToken) -> String {
    match token {
        ExprToken::Number(value)
        | ExprToken::Date(value)
        | ExprToken::Identifier(value)
        | ExprToken::StringLiteral(value) => format!("«{value}»"),
        ExprToken::Plus => "«+»".into(),
        ExprToken::Minus => "«-»".into(),
        ExprToken::Star => "«*»".into(),
        ExprToken::Slash => "«/»".into(),
        ExprToken::LeftParen => "«(»".into(),
        ExprToken::RightParen => "«)»".into(),
    }
}

fn lex_expression(input: &str) -> Result<Vec<ExprToken>, String> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        let simple = match ch {
            '+' => Some(ExprToken::Plus),
            '-' => Some(ExprToken::Minus),
            '*' => Some(ExprToken::Star),
            '/' => Some(ExprToken::Slash),
            '(' => Some(ExprToken::LeftParen),
            ')' => Some(ExprToken::RightParen),
            _ => None,
        };
        if let Some(token) = simple {
            tokens.push(token);
            index += 1;
            continue;
        }
        if ch == '\'' || ch == '"' {
            let quote = ch;
            index += 1;
            let mut value = String::new();
            let mut escaped = false;
            let mut closed = false;
            while index < chars.len() {
                let current = chars[index];
                index += 1;
                if escaped {
                    value.push(current);
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    closed = true;
                    break;
                } else {
                    value.push(current);
                }
            }
            if !closed {
                return Err("Незакрытая строка в формуле".into());
            }
            tokens.push(ExprToken::StringLiteral(value));
            continue;
        }
        if ch.is_ascii_digit() {
            if let Some((literal, consumed)) = scan_date_literal(&chars[index..]) {
                tokens.push(ExprToken::Date(literal));
                index += consumed;
                continue;
            }
            let start = index;
            let mut separators = 0usize;
            while index < chars.len()
                && (chars[index].is_ascii_digit() || matches!(chars[index], '.' | ','))
            {
                if matches!(chars[index], '.' | ',') {
                    separators += 1;
                }
                index += 1;
            }
            let value = chars[start..index].iter().collect::<String>();
            if separators > 1 {
                return Err(format!("Некорректное число в формуле: {value}"));
            }
            tokens.push(ExprToken::Number(value));
            continue;
        }
        if ch.is_alphabetic() || matches!(ch, '_' | '@') {
            let start = index;
            while index < chars.len()
                && (chars[index].is_alphanumeric() || matches!(chars[index], '_' | '.' | '@'))
            {
                index += 1;
            }
            tokens.push(ExprToken::Identifier(
                chars[start..index].iter().collect::<String>(),
            ));
            continue;
        }
        return Err(format!("Недопустимый символ в формуле: «{ch}»"));
    }
    Ok(tokens)
}

fn scan_date_literal(chars: &[char]) -> Option<(String, usize)> {
    for (length, first_separator, second_separator) in
        [(10usize, 4usize, 7usize), (10usize, 2usize, 5usize)]
    {
        if chars.len() < length {
            continue;
        }
        let candidate = chars[..length].iter().collect::<String>();
        let first = chars[first_separator];
        let second = chars[second_separator];
        let separators_match = if first_separator == 4 {
            first == '-' && second == '-'
        } else {
            (first == '.' && second == '.') || (first == '/' && second == '/')
        };
        if separators_match
            && parse_date(&candidate).is_some()
            && chars.get(length).is_none_or(|next| {
                next.is_whitespace() || matches!(*next, '+' | '-' | '*' | '/' | '(' | ')')
            })
        {
            return Some((candidate, length));
        }
    }
    None
}
fn parse_date(s: &str) -> Option<NaiveDate> {
    ["%d.%m.%Y", "%Y-%m-%d", "%d/%m/%Y"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(s.trim(), f).ok())
}
fn parse_number(s: &str) -> Option<i128> {
    parse_decimal_scaled(s).ok()
}
fn parse_decimal_scaled(s: &str) -> Result<i128, String> {
    let cleaned = s.trim().replace([' ', '\u{00A0}'], "").replace(',', ".");
    let negative = cleaned.starts_with('-');
    let body = cleaned.trim_start_matches(['+', '-']);
    let mut parts = body.split('.');
    let whole_text = parts.next().unwrap_or("0");
    if whole_text.is_empty() && !body.starts_with('.') {
        return Err(format!("Не число: {s}"));
    }
    let whole = if whole_text.is_empty() {
        0
    } else {
        whole_text
            .parse::<i128>()
            .map_err(|_| format!("Не число: {s}"))?
    };
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!("Не число: {s}"));
    }
    let mut four = fraction.chars().take(4).collect::<String>();
    while four.len() < 4 {
        four.push('0');
    }
    let mut scaled_fraction = if four.is_empty() {
        0
    } else {
        four.parse::<i128>().map_err(|_| format!("Не число: {s}"))?
    };
    if fraction
        .as_bytes()
        .get(4)
        .is_some_and(|digit| *digit >= b'5')
    {
        scaled_fraction += 1;
    }
    let carry = scaled_fraction / 10_000;
    scaled_fraction %= 10_000;
    let absolute = whole
        .checked_add(carry)
        .and_then(|value| value.checked_mul(10_000))
        .and_then(|value| value.checked_add(scaled_fraction))
        .ok_or_else(|| "Переполнение числа".to_string())?;
    Ok(if negative { -absolute } else { absolute })
}

fn div_round_half_away(numerator: i128, denominator: i128) -> Result<i128, String> {
    if denominator == 0 {
        return Err("Деление на ноль".into());
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let twice_remainder = remainder
        .abs()
        .checked_mul(2)
        .ok_or_else(|| "Переполнение числа".to_string())?;
    if twice_remainder < denominator.abs() {
        return Ok(quotient);
    }
    let adjustment = if (numerator < 0) ^ (denominator < 0) {
        -1
    } else {
        1
    };
    quotient
        .checked_add(adjustment)
        .ok_or_else(|| "Переполнение числа".to_string())
}
fn format_scaled(v: i128) -> String {
    let neg = v < 0;
    let a = v.abs();
    let whole = a / 10_000;
    let frac = a % 10_000;
    if frac == 0 {
        format!("{}{}", if neg { "-" } else { "" }, whole)
    } else {
        let mut f = format!("{frac:04}");
        while f.ends_with('0') {
            f.pop();
        }
        format!("{}{}.{}", if neg { "-" } else { "" }, whole, f)
    }
}
fn parse_case(s: &str) -> Option<GrammaticalCase> {
    match s.trim().to_lowercase().as_str() {
        "genitive" | "родительный" => Some(GrammaticalCase::Genitive),
        "dative" | "дательный" => Some(GrammaticalCase::Dative),
        "accusative" | "винительный" => Some(GrammaticalCase::Accusative),
        "instrumental" | "творительный" => Some(GrammaticalCase::Instrumental),
        "prepositional" | "предложный" => Some(GrammaticalCase::Prepositional),
        _ => None,
    }
}
fn apply_modifier(v: &str, m: &str) -> Result<String, String> {
    let m = m.trim();
    if let Some(c) = parse_case(m) {
        return Ok(decline_person_name(v, c));
    }
    match m.to_lowercase().as_str() {
        "person_genitive" => Ok(decline_person_name(v, GrammaticalCase::Genitive)),
        "position_genitive" => Ok(decline_position(v, GrammaticalCase::Genitive)),
        "position_dative" => Ok(decline_position(v, GrammaticalCase::Dative)),
        "position_accusative" => Ok(decline_position(v, GrammaticalCase::Accusative)),
        "position_instrumental" => Ok(decline_position(v, GrammaticalCase::Instrumental)),
        "position_prepositional" => Ok(decline_position(v, GrammaticalCase::Prepositional)),
        "money" => Ok(format_money_ru((parse_decimal_scaled(v)? / 100) as i64)),
        "words" => {
            if let Some(d) = parse_date(v) {
                Ok(date_to_words_ru(d))
            } else {
                let scaled = parse_decimal_scaled(v)?;
                if scaled % 10_000 == 0 {
                    Ok(number_to_words_ru((scaled / 10_000) as i64))
                } else {
                    Ok(money_to_words_ru((scaled / 100) as i64))
                }
            }
        }
        "money_words" => Ok(money_to_words_ru((parse_decimal_scaled(v)? / 100) as i64)),
        "date_words" => parse_date(v)
            .map(date_to_words_ru)
            .ok_or_else(|| "Не дата".into()),
        "phone" => Ok(format_phone_ru(v)),
        "upper" => Ok(v.to_uppercase()),
        "lower" => Ok(v.to_lowercase()),
        _ if m.starts_with("date:") => {
            let f = m.trim_start_matches("date:");
            parse_date(v)
                .map(|d| d.format(f).to_string())
                .ok_or_else(|| "Не дата".into())
        }
        _ => Err("Неизвестный модификатор".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};
    fn c() -> SemanticCase {
        let mut c = SemanticCase::default();
        for (k, v) in [
            ("org.type", "ИП"),
            ("org.name", "Иванов Иван"),
            ("amount.total", "1000"),
            ("contract.date", "2026-05-15"),
        ] {
            c.values.insert(
                k.into(),
                SemanticValue::new(k, v, ValueSource::UserConfirmed, 1.0),
            );
        }
        let mut a = SemanticRecord::new();
        a.insert("name".into(), SemanticAtom::Text("Работа".into()));
        a.insert("price".into(), SemanticAtom::Decimal("100.50".into()));
        let mut b = SemanticRecord::new();
        b.insert("name".into(), SemanticAtom::Text("Материал".into()));
        b.insert("price".into(), SemanticAtom::Decimal("20".into()));
        c.collections.insert("items".into(), vec![a, b]);
        c.blocks.insert("requisites".into(), "{{org.name}}".into());
        c
    }
    #[test]
    fn conditions_loops_formula_blocks() {
        let r=render_advanced_text_template("{{#if org.type == \"ИП\"}}ИП{{else}}ООО{{/if}} {{#each items}}{{@number}} {{item.name}};{{/each}} {{sum items.price}} {{block requisites}} {{= amount.total * 0.20}}",&c(),true);
        assert!(r.template_errors.is_empty(), "{:?}", r.template_errors);
        assert!(r
            .output_text
            .contains("ИП 1 Работа;2 Материал; 120.5 Иванов Иван 200"));
    }
    #[test]
    fn strict_unclosed() {
        assert!(!inspect_template_syntax("{{#if x}}x").is_empty());
    }
    #[test]
    fn working_days() {
        let r = render_advanced_text_template("{{= contract.date + 1 working_days}}", &c(), true);
        assert_eq!(r.output_text, "18.05.2026");
    }
    #[test]
    fn escaped_delimiters_are_literal_text_not_template_fields() {
        let result = render_advanced_text_template(
            r"Пример: \{{customer.name\}}; настоящее поле: {{org.name}}",
            &c(),
            true,
        );
        assert_eq!(
            result.output_text,
            "Пример: {{customer.name}}; настоящее поле: Иванов Иван"
        );
        assert!(result.missing_fields.is_empty());
        assert!(result.unknown_fields.is_empty());
        assert!(result.template_errors.is_empty());
    }

    #[test]
    fn double_braces_inside_a_semantic_value_are_preserved() {
        let mut case = c();
        case.values.insert(
            "custom.code".into(),
            SemanticValue::new(
                "custom.code",
                "fn main() { println!(\"{{value}}\"); }",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let result = render_advanced_text_template("Код: {{custom.code}}", &case, true);
        assert!(result.output_text.contains("{{value}}"));
        assert!(result.missing_fields.is_empty());
        assert!(result.unknown_fields.is_empty());
        assert!(result.template_errors.is_empty());
    }

    #[test]
    fn unmatched_opening_tag_is_not_duplicated_and_is_reported() {
        let result = render_advanced_text_template("До {{org.name", &c(), true);
        assert_eq!(result.output_text, "До {{org.name");
        assert!(result
            .template_errors
            .iter()
            .any(|error| error.contains("Незакрытый тег")));
    }

    #[test]
    fn strict_mode_tracks_missing_fields_in_conditions_and_formulas() {
        let condition = render_advanced_text_template(
            "{{#if contract.status == \"активен\"}}Да{{/if}}",
            &c(),
            true,
        );
        assert!(condition
            .missing_fields
            .contains(&"contract.status".to_string()));
        let formula = render_advanced_text_template("{{= amount.total + amount.tax}}", &c(), true);
        assert!(formula.missing_fields.contains(&"amount.tax".to_string()));
    }

    #[test]
    fn quoted_logical_words_do_not_split_condition() {
        let mut case = c();
        case.values.insert(
            "contract.status".into(),
            SemanticValue::new(
                "contract.status",
                "А или Б",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let result = render_advanced_text_template(
            "{{#if contract.status == \"А или Б\"}}совпало{{else}}нет{{/if}}",
            &case,
            true,
        );
        assert_eq!(result.output_text, "совпало");
        assert!(
            result.template_errors.is_empty(),
            "{:?}",
            result.template_errors
        );
    }

    #[test]
    fn formula_resolves_right_operand_from_semantic_case() {
        let mut case = c();
        case.values.insert(
            "amount.tax".into(),
            SemanticValue::new("amount.tax", "200.25", ValueSource::UserConfirmed, 1.0),
        );
        let result = render_advanced_text_template("{{= amount.total + amount.tax}}", &case, true);
        assert_eq!(result.output_text, "1200.25");
        assert!(result.missing_fields.is_empty());
    }

    #[test]
    fn fixed_point_math_rounds_half_away_from_zero() {
        assert_eq!(parse_decimal_scaled("1.23455").unwrap(), 12_346);
        assert_eq!(parse_decimal_scaled("-1.23455").unwrap(), -12_346);
        let result = render_advanced_text_template("{{= amount.total / 3}}", &c(), true);
        assert_eq!(result.output_text, "333.3333");
        let result = render_advanced_text_template("{{= 0.0001 * 0.5000}}", &c(), true);
        assert_eq!(result.output_text, "0.0001");
    }

    #[test]
    fn formulas_support_chains_parentheses_and_precedence() {
        let mut case = c();
        for (field, value) in [("amount.vat", "200"), ("amount.fee", "50")] {
            case.values.insert(
                field.into(),
                SemanticValue::new(field, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        let chained = render_advanced_text_template(
            "{{= amount.total + amount.vat + amount.fee}}",
            &case,
            true,
        );
        assert_eq!(chained.output_text, "1250");
        assert!(
            chained.template_errors.is_empty(),
            "{:?}",
            chained.template_errors
        );

        let grouped = render_advanced_text_template("{{= (3 + 2) * 2}}", &case, true);
        assert_eq!(grouped.output_text, "10");
        assert!(
            grouped.template_errors.is_empty(),
            "{:?}",
            grouped.template_errors
        );

        let precedence = render_advanced_text_template("{{= 3 + 2 * 4}}", &case, true);
        assert_eq!(precedence.output_text, "11");

        let no_spaces = render_advanced_text_template("{{=amount.total-amount.vat}}", &case, true);
        assert_eq!(no_spaces.output_text, "800");
        assert!(
            no_spaces.missing_fields.is_empty(),
            "{:?}",
            no_spaces.missing_fields
        );
    }

    #[test]
    fn malformed_or_unsupported_formulas_fail_closed() {
        for template in [
            "{{= amount.total + }}",
            "{{= (amount.total + 1 }}",
            "{{= amount.total nonsense}}",
            "{{= amount.total + \"текст\"}}",
        ] {
            let result = render_advanced_text_template(template, &c(), true);
            assert!(
                result.output_text.is_empty(),
                "{template}: {}",
                result.output_text
            );
            assert!(
                !result.template_errors.is_empty(),
                "{template} unexpectedly passed"
            );
        }
    }

    #[test]
    fn missing_formula_identifier_is_both_tracked_and_blocking() {
        let result =
            render_advanced_text_template("{{= amount.total + amount.unknown + 1}}", &c(), true);
        assert!(result.output_text.is_empty());
        assert!(result
            .missing_fields
            .contains(&"amount.unknown".to_string()));
        assert!(result
            .template_errors
            .iter()
            .any(|error| error.contains("amount.unknown")));
    }

    #[test]
    fn working_day_formula_fails_closed_outside_complete_calendar() {
        let mut case = c();
        case.values.insert(
            "contract.date".into(),
            SemanticValue::new(
                "contract.date",
                "2027-12-30",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let result =
            render_advanced_text_template("{{= contract.date + 3 working_days}}", &case, true);
        assert!(result.output_text.is_empty());
        assert!(result
            .template_errors
            .iter()
            .any(|error| error.contains("2027")));
    }

    #[test]
    fn image_placeholder_is_structured_and_not_rendered_as_a_path() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "org.stamp".into(),
            crate::SemanticValue::new(
                "org.stamp",
                "C:/assets/stamp.png",
                crate::ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let rendered = render_advanced_text_template("Печать: {{image org.stamp}}", &case, true);
        assert!(rendered.missing_fields.is_empty());
        assert_eq!(
            rendered.output_text,
            "Печать: [[DOKKOMPLEKT_IMAGE:org.stamp]]"
        );
        assert_eq!(
            template_image_requests("{{image org.stamp}}"),
            vec!["org.stamp"]
        );
    }

    #[test]
    fn dependency_references_include_collections_and_blocks() {
        let template = "{{#each items}}{{this.name}}{{/each}} {{sum items.amount}} {{count approvals}} {{block requisites}}";
        assert_eq!(
            template_collection_references(template),
            vec!["approvals".to_string(), "items".to_string()]
        );
        assert_eq!(
            template_block_references(template),
            vec!["requisites".to_string()]
        );
    }

    #[test]
    fn counter() {
        assert_eq!(
            format_counter_value("Д-{YYYY}/{NNN}", 7, 2026),
            "Д-2026/007"
        );
    }
}
