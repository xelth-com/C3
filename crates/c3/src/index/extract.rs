//! Lexical entity and relation extraction for the derived index (DESIGN §7).
//!
//! This is the offline, dependency-free approximation the pack pipeline already uses
//! (see [`crate::pack::budget::skeletonize`] and [`crate::pack::identifiers`]): a
//! regex/brace scanner, not an AST. It walks a discovered file and emits one `entity` per
//! declaration it recognises (functions, types, impls, modules, classes) plus one `file`
//! entity, capturing the item's line range, first doc line, the item text and the
//! identifiers it references. Relations are then derived from those entities:
//! `belongs_to` (item → file → directory), `calls` (an identifier in an item that names
//! another indexed entity) and `relates_to` (declared `@relates: a, b` tags in doc
//! comments). Everything here is pure and unit-tested without the `index-surreal` feature.
//!
//! Limits (by construction): a body line that begins like a declaration is treated as one,
//! a signature that wraps across lines keeps only its first line's name, and brace matching
//! is line-based. A faithful graph needs a language server; this is the rebuildable,
//! never-required derived index the design calls for (invariant 6).

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::pack::{identifiers, redact};

/// The kinds the design names: `file|module|fn|struct|enum|trait|impl|class|function|other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    File,
    Module,
    Fn,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Function,
    Other,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::File => "file",
            EntityKind::Module => "module",
            EntityKind::Fn => "fn",
            EntityKind::Struct => "struct",
            EntityKind::Enum => "enum",
            EntityKind::Trait => "trait",
            EntityKind::Impl => "impl",
            EntityKind::Class => "class",
            EntityKind::Function => "function",
            EntityKind::Other => "other",
        }
    }
}

/// A single indexed entity. `id` is the deterministic `path::name` (deduped on collision).
#[derive(Debug, Clone)]
pub struct Entity {
    pub id: String,
    pub kind: EntityKind,
    /// Repo-relative POSIX path.
    pub path: String,
    pub name: String,
    pub lang: String,
    pub line_start: usize,
    pub line_end: usize,
    /// First doc-comment line (empty if none).
    pub summary: String,
    /// The item text, already passed through [`redact`].
    pub code: String,
    pub content_hash: String,
    /// `@relates:` targets declared in the item's doc comment (names, not ids).
    pub relates: Vec<String>,
    /// Identifiers the item references (for `calls` derivation); not stored on the row.
    pub refs: BTreeSet<String>,
}

/// A derived relation edge between two entity ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub from: String,
    pub kind: RelationKind,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    BelongsTo,
    Calls,
    RelatesTo,
}

impl RelationKind {
    pub fn table(self) -> &'static str {
        match self {
            RelationKind::BelongsTo => "belongs_to",
            RelationKind::Calls => "calls",
            RelationKind::RelatesTo => "relates_to",
        }
    }
    /// The label a retrieval hit carries ("derived: calls").
    pub fn label(self) -> &'static str {
        match self {
            RelationKind::BelongsTo => "derived: belongs_to",
            RelationKind::Calls => "derived: calls",
            RelationKind::RelatesTo => "derived: relates_to",
        }
    }
}

/// Language of a repo-relative path, or `None` for a file we index whole (no item scan).
fn lang_of(rel: &str) -> Option<Lang> {
    let ext = rel
        .rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()))
        .unwrap_or_default();
    match ext.as_str() {
        "rs" => Some(Lang::Rust),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => Some(Lang::Js),
        "py" => Some(Lang::Py),
        _ => None,
    }
}

fn lang_name(rel: &str) -> String {
    match lang_of(rel) {
        Some(Lang::Rust) => "rust",
        Some(Lang::Js) => {
            let ext = rel.rsplit('.').next().unwrap_or("");
            if ext.starts_with("ts") {
                "typescript"
            } else {
                "javascript"
            }
        }
        Some(Lang::Py) => "python",
        None => "text",
    }
    .to_string()
}

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Rust,
    Js,
    Py,
}

fn hash_str(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

/// The `content_hash` of a whole file, used by the `file_hash` skip table.
pub fn file_hash(content: &str) -> String {
    hash_str(content)
}

/// Extract every entity from one file (the `file` entity plus each recognised item).
///
/// `rel` is the repo-relative POSIX path; `content` is the raw file text. Item `code` is
/// redacted here so a stored row never carries a secret (invariant 5). Ids are `path::name`,
/// deduped with a `~N` suffix when a name repeats in the file.
pub fn extract_file(rel: &str, content: &str, generation: &str) -> Vec<Entity> {
    let lang = lang_name(rel);
    let (red_file, _) = redact::redact(content);
    let line_count = content.lines().count().max(1);

    let mut out: Vec<Entity> = Vec::new();
    let mut used_ids: BTreeMap<String, usize> = BTreeMap::new();

    let mk_id = |name: &str, used: &mut BTreeMap<String, usize>| -> String {
        let base = format!("{rel}::{name}");
        let n = used.entry(base.clone()).or_insert(0);
        let id = if *n == 0 {
            base.clone()
        } else {
            format!("{base}~{n}")
        };
        *n += 1;
        id
    };

    // The file entity: name is the basename, code is the whole redacted file.
    let basename = rel.rsplit('/').next().unwrap_or(rel).to_string();
    let file_id = mk_id(&basename, &mut used_ids);
    out.push(Entity {
        id: file_id.clone(),
        kind: EntityKind::File,
        path: rel.to_string(),
        name: basename,
        lang: lang.clone(),
        line_start: 1,
        line_end: line_count,
        summary: String::new(),
        code: red_file,
        content_hash: hash_str(content),
        relates: Vec::new(),
        refs: BTreeSet::new(),
    });

    let Some(l) = lang_of(rel) else {
        return out;
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some((kind, name)) = decl_at(line, l) {
            let (end, body) = capture_item(&lines, i, l);
            let (doc_summary, relates) = doc_above(&lines, i, l);
            let (red_body, _) = redact::redact(&body);
            let refs = identifiers(&body);
            let id = mk_id(&name, &mut used_ids);
            out.push(Entity {
                id,
                kind,
                path: rel.to_string(),
                name,
                lang: lang.clone(),
                line_start: i + 1,
                line_end: end + 1,
                summary: doc_summary,
                content_hash: hash_str(&format!("{generation}\n{red_body}")),
                code: red_body,
                relates,
                refs,
            });
            // Continue scanning inside the item too (nested fns / impl methods): step one
            // line, not past the body, so methods inside an `impl` are still found.
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Recognise a declaration on `line`; returns its kind and name.
fn decl_at(line: &str, lang: Lang) -> Option<(EntityKind, String)> {
    let t = line.trim_start();
    match lang {
        Lang::Rust => {
            let t = strip_prefixes(t, &["pub ", "pub(crate) ", "pub(super) ", "default "]);
            if let Some(r) = t
                .strip_prefix("async fn ")
                .or_else(|| t.strip_prefix("fn "))
            {
                return ident(r).map(|n| (EntityKind::Fn, n));
            }
            for (kw, kind) in [
                ("const fn ", EntityKind::Fn),
                ("unsafe fn ", EntityKind::Fn),
                ("struct ", EntityKind::Struct),
                ("enum ", EntityKind::Enum),
                ("trait ", EntityKind::Trait),
                ("mod ", EntityKind::Module),
                ("type ", EntityKind::Other),
                ("const ", EntityKind::Other),
                ("static ", EntityKind::Other),
            ] {
                if let Some(r) = t.strip_prefix(kw) {
                    return ident(r).map(|n| (kind, n));
                }
            }
            if let Some(r) = t.strip_prefix("impl ") {
                // Name: the text up to `{` / `where`, generics dropped — e.g. "Foo",
                // "Trait for Foo".
                let head = r.split(['{']).next().unwrap_or(r);
                let head = head.split(" where").next().unwrap_or(head).trim();
                let name = strip_generics(head);
                if !name.is_empty() {
                    return Some((EntityKind::Impl, name));
                }
            }
            None
        }
        Lang::Js => {
            let t = strip_prefixes(t, &["export default ", "export ", "default "]);
            if let Some(r) = t
                .strip_prefix("async function ")
                .or_else(|| t.strip_prefix("function "))
            {
                return ident(r).map(|n| (EntityKind::Function, n));
            }
            if let Some(r) = t.strip_prefix("class ") {
                return ident(r).map(|n| (EntityKind::Class, n));
            }
            if let Some(r) = t
                .strip_prefix("interface ")
                .or_else(|| t.strip_prefix("type "))
            {
                return ident(r).map(|n| (EntityKind::Other, n));
            }
            None
        }
        Lang::Py => {
            if let Some(r) = t
                .strip_prefix("async def ")
                .or_else(|| t.strip_prefix("def "))
            {
                return ident(r).map(|n| (EntityKind::Function, n));
            }
            if let Some(r) = t.strip_prefix("class ") {
                return ident(r).map(|n| (EntityKind::Class, n));
            }
            None
        }
    }
}

fn strip_prefixes<'a>(mut s: &'a str, prefixes: &[&str]) -> &'a str {
    loop {
        let mut stripped = false;
        for p in prefixes {
            if let Some(r) = s.strip_prefix(p) {
                s = r.trim_start();
                stripped = true;
            }
        }
        if !stripped {
            return s;
        }
    }
}

/// The leading identifier of `s` (stops at the first non `[A-Za-z0-9_]`).
fn ident(s: &str) -> Option<String> {
    let name: String = s
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Drop a trailing generic parameter list and any tail from an impl head ("Foo<T>" → "Foo").
fn strip_generics(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Capture the item body starting at `start`, returning the (0-based) last line and text.
///
/// Brace-delimited langs (Rust/JS): read to the first `{`, then to its matching `}`. A
/// declaration that ends in `;` before any `{` is a one-line item. Python: read until a
/// line indented no deeper than the declaration (blank lines excepted).
fn capture_item(lines: &[&str], start: usize, lang: Lang) -> (usize, String) {
    match lang {
        Lang::Rust | Lang::Js => {
            let mut depth = 0i32;
            let mut seen_brace = false;
            let mut end = start;
            for (idx, line) in lines.iter().enumerate().skip(start) {
                for c in line.chars() {
                    match c {
                        '{' => {
                            depth += 1;
                            seen_brace = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                end = idx;
                if seen_brace && depth <= 0 {
                    break;
                }
                if !seen_brace && line.trim_end().ends_with(';') {
                    break;
                }
                // Guard: a signature line with no brace and no `;` (e.g. a wrapped trait fn
                // decl) — stop at a blank line so we do not swallow the rest of the file.
                if !seen_brace && idx > start && line.trim().is_empty() {
                    break;
                }
            }
            (end, lines[start..=end].join("\n"))
        }
        Lang::Py => {
            let base_indent = indent_of(lines[start]);
            let mut end = start;
            for (idx, line) in lines.iter().enumerate().skip(start + 1) {
                if line.trim().is_empty() {
                    continue;
                }
                if indent_of(line) <= base_indent {
                    break;
                }
                end = idx;
            }
            (end, lines[start..=end].join("\n"))
        }
    }
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// The doc comment immediately above line `start`: its first line as a summary, plus any
/// `@relates:` targets.
fn doc_above(lines: &[&str], start: usize, lang: Lang) -> (String, Vec<String>) {
    let mut doc: Vec<String> = Vec::new();
    let mut i = start;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim_start();
        let is_doc = match lang {
            Lang::Rust => t.starts_with("///") || t.starts_with("//!") || t.starts_with("//"),
            Lang::Js => {
                t.starts_with("///")
                    || t.starts_with("//")
                    || t.starts_with("*")
                    || t.starts_with("/*")
            }
            Lang::Py => t.starts_with('#'),
        };
        // Rust/JS attribute or decorator lines sit between the doc and the item; skip them.
        let is_attr = matches!(lang, Lang::Rust) && t.starts_with("#[")
            || matches!(lang, Lang::Js | Lang::Py) && t.starts_with('@');
        if is_attr {
            continue;
        }
        if is_doc {
            doc.push(t.to_string());
        } else if t.is_empty() && doc.is_empty() {
            continue;
        } else {
            break;
        }
    }
    doc.reverse();

    let mut relates: Vec<String> = Vec::new();
    let mut summary = String::new();
    for raw in &doc {
        let clean = raw
            .trim_start_matches(['/', '!', '*', '#', ' '])
            .trim()
            .to_string();
        if let Some(rest) = clean.strip_prefix("@relates:") {
            for r in rest.split(',') {
                let r = r.trim();
                if !r.is_empty() {
                    relates.push(r.to_string());
                }
            }
        } else if summary.is_empty() && !clean.is_empty() && !clean.starts_with('@') {
            summary = clean;
        }
    }
    (summary, relates)
}

/// The full extraction of a file set: every entity and every derived relation.
#[derive(Debug, Default)]
pub struct Extraction {
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
}

/// Derive relations across a set of already-extracted entities from one or more files.
///
/// - `belongs_to`: each item → its file entity, and each file → its directory entity.
/// - `calls`: an identifier an item references that names another indexed entity (self
///   excluded, file entities excluded as targets).
/// - `relates_to`: a `@relates:` name that matches another entity's name.
///
/// Directory entities are synthesised here (one per distinct parent directory) so the
/// structural chain item → file → directory the design names exists as records.
pub fn derive_relations(entities: &[Entity]) -> (Vec<Entity>, Vec<Relation>) {
    let mut relations: Vec<Relation> = Vec::new();

    // name → ids (for calls / relates_to; a common name like `new` maps to several).
    let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    // path → file entity id.
    let mut file_of_path: BTreeMap<&str, &str> = BTreeMap::new();
    for e in entities {
        by_name.entry(e.name.as_str()).or_default().push(&e.id);
        if e.kind == EntityKind::File {
            file_of_path.insert(e.path.as_str(), &e.id);
        }
    }

    // Directory entities, one per parent dir of an indexed file.
    let mut dir_entities: Vec<Entity> = Vec::new();
    let mut dir_id_of: BTreeMap<String, String> = BTreeMap::new();
    for e in entities {
        if e.kind != EntityKind::File {
            continue;
        }
        let dir = e.path.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        let dir_key = dir.to_string();
        let dir_id = dir_id_of.entry(dir_key.clone()).or_insert_with(|| {
            let name = dir.rsplit('/').next().unwrap_or(dir).to_string();
            let id = format!("{dir}::<dir>");
            dir_entities.push(Entity {
                id: id.clone(),
                kind: EntityKind::Other,
                path: dir.to_string(),
                name,
                lang: "dir".to_string(),
                line_start: 0,
                line_end: 0,
                summary: String::new(),
                code: String::new(),
                content_hash: hash_str(dir),
                relates: Vec::new(),
                refs: BTreeSet::new(),
            });
            id
        });
        relations.push(Relation {
            from: e.id.clone(),
            kind: RelationKind::BelongsTo,
            to: dir_id.clone(),
        });
    }

    for e in entities {
        if e.kind == EntityKind::File {
            continue;
        }
        // item → file
        if let Some(fid) = file_of_path.get(e.path.as_str()) {
            if *fid != e.id {
                relations.push(Relation {
                    from: e.id.clone(),
                    kind: RelationKind::BelongsTo,
                    to: (*fid).to_string(),
                });
            }
        }
        // calls: referenced identifiers that name another (non-file) entity
        for r in &e.refs {
            if r == &e.name {
                continue;
            }
            if let Some(ids) = by_name.get(r.as_str()) {
                for tid in ids {
                    if *tid == e.id {
                        continue;
                    }
                    // Skip file entities as call targets.
                    let is_file_target = entities
                        .iter()
                        .any(|x| x.id == **tid && x.kind == EntityKind::File);
                    if is_file_target {
                        continue;
                    }
                    relations.push(Relation {
                        from: e.id.clone(),
                        kind: RelationKind::Calls,
                        to: (*tid).to_string(),
                    });
                }
            }
        }
        // relates_to: declared @relates names
        for r in &e.relates {
            if let Some(ids) = by_name.get(r.as_str()) {
                for tid in ids {
                    if *tid == e.id {
                        continue;
                    }
                    relations.push(Relation {
                        from: e.id.clone(),
                        kind: RelationKind::RelatesTo,
                        to: (*tid).to_string(),
                    });
                }
            }
        }
    }

    // Deterministic order (rebuild-identical) and dedup.
    relations.sort_by(|a, b| {
        (a.from.as_str(), a.kind.table(), a.to.as_str()).cmp(&(
            b.from.as_str(),
            b.kind.table(),
            b.to.as_str(),
        ))
    });
    relations.dedup_by(|a, b| a.from == b.from && a.kind == b.kind && a.to == b.to);
    (dir_entities, relations)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEN: &str = "test-gen";

    #[test]
    fn extracts_rust_fn_struct_impl() {
        let src = "\
/// Adds two numbers.
/// @relates: Point
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub struct Point {
    x: i32,
}

impl Point {
    pub fn origin() -> Point {
        Point { x: 0 }
    }
}
";
        let ents = extract_file("src/lib.rs", src, GEN);
        let kinds: Vec<_> = ents
            .iter()
            .map(|e| (e.kind.as_str(), e.name.as_str()))
            .collect();
        assert!(kinds.contains(&("file", "lib.rs")));
        assert!(kinds.contains(&("fn", "add")));
        assert!(kinds.contains(&("struct", "Point")));
        assert!(kinds.contains(&("impl", "Point")));
        assert!(kinds.contains(&("fn", "origin")));

        let add = ents.iter().find(|e| e.name == "add").unwrap();
        assert_eq!(add.summary, "Adds two numbers.");
        assert_eq!(add.relates, vec!["Point".to_string()]);
        assert_eq!(add.line_start, 3);
        assert!(add.code.contains("a + b"));
    }

    #[test]
    fn deterministic_ids_and_hashes() {
        let src = "pub fn a() {}\npub fn a() {}\n";
        let ents = extract_file("m.rs", src, GEN);
        let ids: Vec<_> = ents.iter().map(|e| e.id.clone()).collect();
        assert!(ids.contains(&"m.rs::a".to_string()));
        assert!(ids.contains(&"m.rs::a~1".to_string()));
        // Re-run yields identical ids and hashes.
        let ents2 = extract_file("m.rs", src, GEN);
        for (x, y) in ents.iter().zip(ents2.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.content_hash, y.content_hash);
        }
    }

    #[test]
    fn derives_calls_and_belongs_and_relates() {
        let src = "\
/// @relates: helper
pub fn caller() {
    helper();
}

pub fn helper() {}
";
        let mut ents = extract_file("src/x.rs", src, GEN);
        let (dirs, rels) = derive_relations(&ents);
        ents.extend(dirs);
        let caller = ents.iter().find(|e| e.name == "caller").unwrap();
        let helper = ents.iter().find(|e| e.name == "helper").unwrap();
        let file = ents
            .iter()
            .find(|e| e.kind == EntityKind::File && e.name == "x.rs")
            .unwrap();

        assert!(rels
            .iter()
            .any(|r| r.from == caller.id && r.kind == RelationKind::Calls && r.to == helper.id));
        assert!(rels
            .iter()
            .any(|r| r.from == caller.id && r.kind == RelationKind::BelongsTo && r.to == file.id));
        assert!(rels
            .iter()
            .any(|r| r.from == file.id && r.kind == RelationKind::BelongsTo));
        assert!(rels.iter().any(|r| r.from == caller.id
            && r.kind == RelationKind::RelatesTo
            && r.to == helper.id));
    }

    #[test]
    fn python_and_js() {
        let py = "class Foo:\n    def bar(self):\n        return 1\n";
        let ents = extract_file("a.py", py, GEN);
        assert!(ents
            .iter()
            .any(|e| e.kind == EntityKind::Class && e.name == "Foo"));
        assert!(ents
            .iter()
            .any(|e| e.kind == EntityKind::Function && e.name == "bar"));

        let js = "export function greet(name) {\n  return name;\n}\n";
        let ents = extract_file("a.js", js, GEN);
        assert!(ents
            .iter()
            .any(|e| e.kind == EntityKind::Function && e.name == "greet"));
    }

    #[test]
    fn non_code_file_is_one_entity() {
        let ents = extract_file("README.md", "# Title\n\nbody\n", GEN);
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].kind, EntityKind::File);
    }
}
