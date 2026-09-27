//! The pack pipeline (`PackBuilder`, milestone 7).
//!
//! One discovery/redaction/budget pipeline produces three outbound artifacts:
//! * [`snapshot`] — a whole-repository Markdown snapshot with an optional git delta;
//! * [`explain`] — an explainer pack for one claim (DESIGN §6, D6), asked once before it
//!   leaves the machine;
//! * [`reviewer`] — the reviewer pack for the `http` engine (milestone 7b) with a
//!   `.pack.json` sidecar.
//!
//! Every file body written by any of them passes through the single sanitizer
//! [`redact::redact`] (DESIGN §3 invariant 5, D8), and every path is repo-relative POSIX
//! (invariant 9). C3 writes only under `.collab/` or an explicit `--out` path and never
//! commits (D13).

pub mod budget;
pub mod discover;
pub mod explain;
pub mod redact;
pub mod reviewer;
pub mod snapshot;

use std::collections::BTreeSet;
use std::path::Path;

pub use discover::{DiscoverOpts, FileEntry};

/// The `--- File: <posix path> ---` marker eckSnapshot uses (kept for parity so a C3 pack
/// and an eckSnapshot snapshot read the same to a chat model).
pub fn file_marker(rel: &str) -> String {
    format!("--- File: {rel} ---")
}

/// Read a file as UTF-8, replacing invalid sequences (a pack is text for a chat model, not
/// a byte-exact copy). Returns `None` when the file cannot be opened.
pub fn read_text(abs: &Path) -> Option<String> {
    std::fs::read(abs)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Prefix every line with a right-aligned 1-based line number (`   12\tcode`) — the
/// explainer pack renders focus files this way so a citation can name `file:line`.
pub fn with_line_numbers(text: &str) -> String {
    let total = text.lines().count().max(1);
    let width = total.to_string().len();
    text.lines()
        .enumerate()
        .map(|(i, l)| format!("{:>width$}\t{}", i + 1, l, width = width))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render an ASCII directory tree from a set of repo-relative POSIX paths.
pub fn directory_tree(paths: &[String]) -> String {
    // Build a nested map of path components.
    #[derive(Default)]
    struct Node {
        children: std::collections::BTreeMap<String, Node>,
        is_file: bool,
    }
    let mut root = Node::default();
    for p in paths {
        let mut cur = &mut root;
        let parts: Vec<&str> = p.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            cur = cur.children.entry(part.to_string()).or_default();
            if i == parts.len() - 1 {
                cur.is_file = true;
            }
        }
    }
    fn render(node: &Node, prefix: &str, out: &mut String) {
        let entries: Vec<(&String, &Node)> = node.children.iter().collect();
        for (i, (name, child)) in entries.iter().enumerate() {
            let last = i == entries.len() - 1;
            let branch = if last { "└── " } else { "├── " };
            let suffix = if child.is_file { "" } else { "/" };
            out.push_str(&format!("{prefix}{branch}{name}{suffix}\n"));
            let next_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            render(child, &next_prefix, out);
        }
    }
    let mut out = String::new();
    render(&root, "", &mut out);
    out
}

/// The `.eck/` manifest digest: the three narrative manifests, read as input only, in a
/// fixed order. `JOURNAL.md` is never read (project rule and DESIGN D6). Returns the
/// rendered (redacted) Markdown section and its redaction count, or `None` when no manifest
/// exists.
pub fn eck_manifest_digest(repo_root: &Path) -> Option<(String, usize)> {
    let eck = repo_root.join(".eck");
    if !eck.is_dir() {
        return None;
    }
    let mut section = String::new();
    let mut redactions = 0usize;
    let mut any = false;
    for name in ["CONTEXT.md", "ARCHITECTURE.md", "ROADMAP.md"] {
        let p = eck.join(name);
        if let Some(text) = read_text(&p) {
            any = true;
            let (red, n) = redact::redact(&text);
            redactions += n;
            section.push_str(&format!("### .eck/{name}\n\n{}\n\n", red.trim_end()));
        }
    }
    if any {
        Some((section, redactions))
    } else {
        None
    }
}

/// Extract candidate identifiers from focus text: word-boundary tokens of length ≥ 4 that
/// are not common language keywords. Used to build the lexical neighbourhood (files that
/// share identifiers with the focus) for the explainer and reviewer packs when
/// `Index: none`.
pub fn identifiers(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut BTreeSet<String>| {
        if cur.len() >= 4 && !is_keyword(cur) && cur.chars().any(|c| c.is_alphabetic()) {
            out.insert(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn is_keyword(w: &str) -> bool {
    const KW: &[&str] = &[
        "self",
        "true",
        "false",
        "null",
        "none",
        "some",
        "return",
        "const",
        "static",
        "async",
        "await",
        "match",
        "impl",
        "trait",
        "struct",
        "enum",
        "type",
        "where",
        "pub",
        "crate",
        "super",
        "mod",
        "use",
        "let",
        "mut",
        "for",
        "while",
        "loop",
        "break",
        "continue",
        "then",
        "this",
        "function",
        "class",
        "import",
        "export",
        "from",
        "default",
        "public",
        "private",
        "protected",
        "void",
        "string",
        "number",
        "boolean",
        "def",
        "elif",
        "else",
        "with",
        "pass",
        "raise",
        "yield",
        "lambda",
        "print",
        "println",
        "value",
        "index",
        "count",
        "result",
        "error",
        "unwrap",
    ];
    let lw = w.to_ascii_lowercase();
    KW.contains(&lw.as_str())
}

/// A file's relation to the focus in a lexical neighbourhood.
#[derive(Debug, Clone)]
pub struct Neighbour {
    pub rel: String,
    /// The identifiers this file shares with the focus (sorted, capped for the header).
    pub shared: Vec<String>,
}

/// Build the lexical neighbourhood: for each non-focus file, the identifiers it shares with
/// the union of focus identifiers, sorted by overlap descending. Files sharing nothing are
/// excluded. This is the `Index: none` fallback the design names ("derived relations").
pub fn lexical_neighbourhood(
    focus_ids: &BTreeSet<String>,
    periphery: &[FileEntry],
) -> Vec<Neighbour> {
    let mut out: Vec<Neighbour> = Vec::new();
    for f in periphery {
        let Some(text) = read_text(&f.abs) else {
            continue;
        };
        let ids = identifiers(&text);
        let shared: Vec<String> = focus_ids.intersection(&ids).cloned().collect();
        if !shared.is_empty() {
            out.push(Neighbour {
                rel: f.rel.clone(),
                shared,
            });
        }
    }
    out.sort_by(|a, b| {
        b.shared
            .len()
            .cmp(&a.shared.len())
            .then_with(|| a.rel.cmp(&b.rel))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_renders_nested_paths() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/sub/b.rs".to_string(),
            "README.md".to_string(),
        ];
        let t = directory_tree(&paths);
        assert!(t.contains("README.md"));
        assert!(t.contains("src/"));
        assert!(t.contains("b.rs"));
    }

    #[test]
    fn line_numbers_are_one_based_and_aligned() {
        let out = with_line_numbers("a\nb\nc");
        assert!(out.starts_with("1\ta"));
        assert!(out.contains("3\tc"));
    }

    #[test]
    fn identifiers_skip_keywords_and_short_tokens() {
        let ids = identifiers("pub fn compute_ledger(store: Store) -> Verdict { let x = 1; }");
        assert!(ids.contains("compute_ledger"));
        assert!(ids.contains("Store"));
        assert!(ids.contains("Verdict"));
        assert!(!ids.contains("pub"));
        assert!(!ids.contains("fn"));
        assert!(!ids.contains("let"));
    }

    #[test]
    fn neighbourhood_ranks_by_overlap() {
        let d = std::env::temp_dir().join(format!("c3-pack-nbr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.rs"), "fn ledger_entry() { compute_verdict(); }").unwrap();
        std::fs::write(d.join("b.rs"), "fn unrelated_thing() {}").unwrap();
        let focus_ids = identifiers("ledger_entry compute_verdict");
        let periphery = vec![
            FileEntry {
                rel: "a.rs".into(),
                abs: d.join("a.rs"),
                size: 0,
            },
            FileEntry {
                rel: "b.rs".into(),
                abs: d.join("b.rs"),
                size: 0,
            },
        ];
        let n = lexical_neighbourhood(&focus_ids, &periphery);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].rel, "a.rs");
        let _ = std::fs::remove_dir_all(&d);
    }
}
