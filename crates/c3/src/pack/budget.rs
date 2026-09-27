//! Token estimation, depth levels, the skeletonizer and the budget cut (ported from
//! eckSnapshot's `depthConfig.js`, `snapshotBuilder.js` metrics and `skeletonizer.js`).
//!
//! The token estimate is eckSnapshot's `chars / 4` with a small per-extension factor. The
//! depth scale is 0–9: tree only, line-truncation, skeleton (signatures only), skeleton +
//! docs, then full with increasing line caps. The skeletonizer here is deliberately
//! lexical — it keeps lines that *look* like declarations for Rust/JS/TS/Python and drops
//! bodies with an explicit `[... N lines omitted ...]` marker. Its limits are documented on
//! [`skeletonize`]: it is not a parser, so a body line that happens to look like a
//! declaration is kept, and a declaration split across lines may be cut.

/// The rendered depth configuration for one level (eckSnapshot `getDepthConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Depth {
    pub level: u8,
    /// Tree only, no file bodies.
    pub tree_only: bool,
    /// Keep only signature-like lines.
    pub skeleton: bool,
    /// In skeleton mode, keep doc comments too.
    pub preserve_docs: bool,
    /// Truncate a body to this many lines (`0` = no cap).
    pub max_lines: usize,
}

/// Resolve a depth level 0–9 (clamped) to its configuration.
pub fn depth(level: u8) -> Depth {
    let d = level.min(9);
    match d {
        0 => Depth {
            level: 0,
            tree_only: true,
            skeleton: false,
            preserve_docs: false,
            max_lines: 0,
        },
        1..=4 => Depth {
            level: d,
            tree_only: false,
            skeleton: false,
            preserve_docs: false,
            max_lines: match d {
                1 => 10,
                2 => 30,
                3 => 60,
                _ => 100,
            },
        },
        5 => Depth {
            level: 5,
            tree_only: false,
            skeleton: true,
            preserve_docs: false,
            max_lines: 0,
        },
        6 => Depth {
            level: 6,
            tree_only: false,
            skeleton: true,
            preserve_docs: true,
            max_lines: 0,
        },
        7 => Depth {
            level: 7,
            tree_only: false,
            skeleton: false,
            preserve_docs: false,
            max_lines: 500,
        },
        8 => Depth {
            level: 8,
            tree_only: false,
            skeleton: false,
            preserve_docs: false,
            max_lines: 1000,
        },
        _ => Depth {
            level: 9,
            tree_only: false,
            skeleton: false,
            preserve_docs: false,
            max_lines: 0,
        },
    }
}

/// A per-extension multiplier on the base `chars / 4` estimate.
fn ext_factor(ext: &str) -> f64 {
    match ext.to_ascii_lowercase().as_str() {
        // Prose tokenizes a little denser than the 1/4 rule; markup/JSON a little coarser.
        "md" | "markdown" | "txt" | "rst" => 1.05,
        "json" | "yaml" | "yml" | "toml" | "lock" => 0.9,
        _ => 1.0,
    }
}

/// Approximate token count for `text` of the given file extension (empty for none).
/// eckSnapshot's `Math.round(content.length / 4)` with the per-extension factor.
pub fn estimate_tokens(text: &str, ext: &str) -> usize {
    let base = text.chars().count() as f64 / 4.0;
    (base * ext_factor(ext)).round() as usize
}

/// The extension of a repo-relative path (`""` for none), lowercased without the dot.
pub fn ext_of(rel: &str) -> String {
    rel.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()))
        .unwrap_or_default()
}

/// Truncate `content` to `max_lines`, appending an explicit omission marker (eckSnapshot's
/// `// ... truncated (N more lines)`), C3 uses the `[... N lines omitted ...]` marker the
/// design names). `0` means no cap.
pub fn truncate_lines(content: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return content.to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        return content.to_string();
    }
    let kept = &lines[..max_lines];
    format!(
        "{}\n[... {} lines omitted ...]",
        kept.join("\n"),
        lines.len() - max_lines
    )
}

/// A lexical skeletonizer for Rust / JS / TS / Python.
///
/// It keeps lines that look like declarations (functions, types, imports, module/impl
/// headers, top-level constants, decorators/attributes) and doc/comment lines when
/// `preserve_docs`; every run of dropped lines becomes one `[... N lines omitted ...]`
/// marker. Files of other languages are returned unchanged (only line-truncated by the
/// caller).
///
/// Limits (by construction, not a bug): this is regex-per-line, not an AST. A body line
/// that begins like a declaration (`let handler = |x| {`) is kept; a declaration whose
/// signature wraps across lines keeps only its first line; brace matching is not tracked,
/// so a kept declaration does not pull in its closing brace. For a faithful skeleton use a
/// language server; this is the offline, dependency-free approximation.
pub fn skeletonize(content: &str, rel: &str, preserve_docs: bool) -> String {
    let ext = ext_of(rel);
    let lang = match ext.as_str() {
        "rs" => Lang::Rust,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => Lang::Js,
        "py" => Lang::Py,
        _ => return content.to_string(),
    };

    let mut out: Vec<String> = Vec::new();
    let mut omitted = 0usize;
    let flush = |out: &mut Vec<String>, omitted: &mut usize| {
        if *omitted > 0 {
            out.push(format!("[... {} lines omitted ...]", *omitted));
            *omitted = 0;
        }
    };

    for line in content.lines() {
        if keep_line(line, lang, preserve_docs) {
            flush(&mut out, &mut omitted);
            out.push(line.to_string());
        } else {
            omitted += 1;
        }
    }
    flush(&mut out, &mut omitted);
    out.join("\n")
}

#[derive(Clone, Copy)]
enum Lang {
    Rust,
    Js,
    Py,
}

fn keep_line(line: &str, lang: Lang, preserve_docs: bool) -> bool {
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    // Doc / comment lines.
    let is_doc = matches!(lang, Lang::Rust)
        && (t.starts_with("///") || t.starts_with("//!") || t.starts_with("/**"))
        || matches!(lang, Lang::Js) && (t.starts_with("/**") || t.starts_with("* ") || t == "*/")
        || matches!(lang, Lang::Py) && (t.starts_with("\"\"\"") || t.starts_with("'''"));
    if is_doc {
        return preserve_docs;
    }

    match lang {
        Lang::Rust => {
            t.starts_with("use ")
                || t.starts_with("pub use ")
                || t.starts_with("mod ")
                || t.starts_with("pub mod ")
                || t.starts_with("#[")
                || t.starts_with("#![")
                || t.starts_with("extern crate")
                || starts_with_any(
                    t,
                    &[
                        "fn ",
                        "pub fn ",
                        "pub(crate) fn ",
                        "async fn ",
                        "pub async fn ",
                        "unsafe fn ",
                        "const fn ",
                        "pub const fn ",
                    ],
                )
                || starts_with_any(
                    t,
                    &[
                        "struct ",
                        "pub struct ",
                        "enum ",
                        "pub enum ",
                        "trait ",
                        "pub trait ",
                        "impl ",
                        "type ",
                        "pub type ",
                        "const ",
                        "pub const ",
                        "static ",
                        "pub static ",
                        "macro_rules!",
                    ],
                )
        }
        Lang::Js => {
            starts_with_any(
                t,
                &[
                    "import ",
                    "export ",
                    "function ",
                    "async function ",
                    "class ",
                    "export default",
                    "export class",
                    "export function",
                    "export async",
                    "export const",
                    "export interface",
                    "export type",
                    "export enum",
                    "interface ",
                    "type ",
                    "const ",
                    "let ",
                    "var ",
                    "public ",
                    "private ",
                    "protected ",
                    "static ",
                    "constructor(",
                    "async ",
                    "get ",
                    "set ",
                ],
            ) || t.starts_with("@")
        }
        Lang::Py => {
            starts_with_any(
                t,
                &["def ", "async def ", "class ", "import ", "from ", "@"],
            ) || (!t.starts_with(' ') && t.contains('=') && !t.contains("=="))
        }
    }
}

fn starts_with_any(t: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| t.starts_with(p))
}

/// One file offered to the budget cut.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub rel: String,
    /// The already-rendered (redaction happens later) body of the file.
    pub body: String,
    /// Focus files are kept whole; periphery is trimmed first.
    pub focus: bool,
    /// Estimated tokens of the full body.
    pub tokens: usize,
}

/// The outcome of a budget cut.
#[derive(Debug, Clone, Default)]
pub struct BudgetPlan {
    /// Files kept in full, in the given order.
    pub full: Vec<String>,
    /// Files reduced to a skeleton because the full body did not fit.
    pub skeletonized: Vec<String>,
    /// Files dropped entirely (with a periphery omission note upstream).
    pub dropped: Vec<String>,
}

/// Decide how each candidate is rendered under a token `budget`.
///
/// Focus files are always kept whole (they are the point of the pack; the budget can be
/// exceeded by them, which the caller reports). Periphery files are added in the given
/// order while budget remains; a periphery file that does not fit whole is offered as a
/// skeleton if the skeleton fits, otherwise dropped. `budget == 0` keeps everything whole.
pub fn plan(candidates: &[Candidate], budget: usize) -> BudgetPlan {
    let mut plan = BudgetPlan::default();
    if budget == 0 {
        plan.full = candidates.iter().map(|c| c.rel.clone()).collect();
        return plan;
    }

    let mut used = 0usize;
    // Focus first, whole, always.
    for c in candidates.iter().filter(|c| c.focus) {
        plan.full.push(c.rel.clone());
        used += c.tokens;
    }
    // Periphery, in order, trimming to fit.
    for c in candidates.iter().filter(|c| !c.focus) {
        if used + c.tokens <= budget {
            plan.full.push(c.rel.clone());
            used += c.tokens;
            continue;
        }
        let skel = skeletonize(&c.body, &c.rel, false);
        let skel_tokens = estimate_tokens(&skel, &ext_of(&c.rel));
        if used + skel_tokens <= budget {
            plan.skeletonized.push(c.rel.clone());
            used += skel_tokens;
        } else {
            plan.dropped.push(c.rel.clone());
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_scale() {
        assert!(depth(0).tree_only);
        assert_eq!(depth(2).max_lines, 30);
        assert!(depth(5).skeleton && !depth(5).preserve_docs);
        assert!(depth(6).skeleton && depth(6).preserve_docs);
        assert_eq!(depth(9).max_lines, 0);
        assert_eq!(depth(200).level, 9); // clamp
    }

    #[test]
    fn token_estimate_is_chars_over_four() {
        let s = "a".repeat(400);
        assert_eq!(estimate_tokens(&s, "rs"), 100);
    }

    #[test]
    fn truncate_marks_omissions() {
        let body = (0..10)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_lines(&body, 3);
        assert!(out.contains("[... 7 lines omitted ...]"));
        assert!(out.starts_with("0\n1\n2"));
    }

    #[test]
    fn skeleton_keeps_rust_signatures_drops_bodies() {
        let src = "pub fn add(a: i32, b: i32) -> i32 {\n    let x = a + b;\n    x\n}\n\nstruct S {\n    field: u8,\n}\n";
        let out = skeletonize(src, "x.rs", false);
        assert!(out.contains("pub fn add"));
        assert!(out.contains("struct S"));
        assert!(out.contains("[... "));
        assert!(!out.contains("let x = a + b"));
    }

    #[test]
    fn skeleton_leaves_unknown_lang_alone() {
        let src = "some prose\nmore prose\n";
        assert_eq!(skeletonize(src, "x.txt", false), src);
    }

    #[test]
    fn budget_keeps_focus_whole_trims_periphery() {
        let big = "fn a() {\n".to_string() + &"    x();\n".repeat(200) + "}\n";
        let cands = vec![
            Candidate {
                rel: "focus.rs".into(),
                body: big.clone(),
                focus: true,
                tokens: estimate_tokens(&big, "rs"),
            },
            Candidate {
                rel: "peri.rs".into(),
                body: big.clone(),
                focus: false,
                tokens: estimate_tokens(&big, "rs"),
            },
        ];
        // Budget only large enough for the focus file.
        let budget = estimate_tokens(&big, "rs") + 10;
        let p = plan(&cands, budget);
        assert!(p.full.contains(&"focus.rs".to_string()));
        // periphery did not fit whole → skeletonized or dropped, never full.
        assert!(!p.full.contains(&"peri.rs".to_string()));
    }
}
