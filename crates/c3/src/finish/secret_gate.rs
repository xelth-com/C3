//! The secret gate: before `c3 finish` commits, every staged text file's STAGED content
//! (`git show :<path>`, not the working-tree copy) passes the pack redactor
//! ([`crate::pack::redact`]). A secret-shaped value refuses the commit; the file and the
//! KIND are reported, never the value, and nothing is unstaged so the coordinator can look.
//! `--allow-secret <path>` (repeatable) lets one reviewed file through.

use std::path::Path;
use std::process::Command;

use crate::pack::redact;

/// A refused file: its repo-relative path and the secret kinds found in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHit {
    pub path: String,
    /// The redactor kinds, comma-joined (`openai-key`, `assignment`, ...).
    pub kinds: String,
}

impl SecretHit {
    /// The one-line report: `path: KIND` (never the value).
    pub fn line(&self) -> String {
        format!("{}: {}", self.path, self.kinds)
    }
}

/// Scan one staged file's content. Returns `Some(kinds)` when the redactor finds a secret,
/// `None` when the content is clean or binary (a binary blob is skipped, as the redactor is
/// a line scanner). The kinds are comma-joined in a stable order.
pub fn scan_staged_text(content: &[u8]) -> Option<String> {
    if is_binary(content) {
        return None;
    }
    let text = String::from_utf8_lossy(content);
    let report = redact::scan(&text);
    if report.by_kind.is_empty() {
        return None;
    }
    // `by_kind` is a BTreeMap, so the join is already in a stable (alphabetical) order.
    let kinds: Vec<&str> = report.by_kind.keys().map(String::as_str).collect();
    Some(kinds.join(", "))
}

/// A null byte in the first 8 KiB marks a binary blob (the same heuristic as discovery).
fn is_binary(content: &[u8]) -> bool {
    let n = content.len().min(8192);
    content[..n].contains(&0)
}

/// Run the gate over every staged file in `repo_root`, skipping any path in `allow`.
/// Returns the refused files (empty = clean). Reads the STAGED blob of each added/copied/
/// modified/renamed file; deletions carry no content and are not scanned.
pub fn scan_staged(repo_root: &Path, allow: &[String]) -> Vec<SecretHit> {
    let allow: std::collections::BTreeSet<String> =
        allow.iter().map(|a| a.replace('\\', "/")).collect();
    let mut hits = Vec::new();
    for path in staged_text_paths(repo_root) {
        if allow.contains(&path) {
            continue;
        }
        let Some(content) = show_staged(repo_root, &path) else {
            continue;
        };
        if let Some(kinds) = scan_staged_text(&content) {
            hits.push(SecretHit { path, kinds });
        }
    }
    hits
}

/// The staged, non-deleted paths (`git diff --cached --name-only --diff-filter=ACMR -z`).
fn staged_text_paths(repo_root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('\\', "/"))
            .collect(),
        _ => Vec::new(),
    }
}

/// The staged blob of `path` (`git show :<path>`), or `None` if it cannot be read.
fn show_staged(repo_root: &Path, path: &str) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("show")
        .arg(format!(":{path}"))
        .output()
        .ok()?;
    if out.status.success() {
        Some(out.stdout)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_an_sk_key() {
        let hit = scan_staged_text(b"let key = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\";\n");
        assert_eq!(hit.as_deref(), Some("openai-key"));
    }

    #[test]
    fn detects_an_api_key_assignment() {
        // An `ANTHROPIC_API_KEY = "..."` style line the assignment rule recognises.
        let hit = scan_staged_text(b"ANTHROPIC_API_KEY = \"supersecretvalue12345\"\n").unwrap();
        assert!(hit.contains("assignment"), "got {hit}");
    }

    #[test]
    fn clean_text_passes() {
        assert_eq!(scan_staged_text(b"fn main() { println!(\"hi\"); }\n"), None);
    }

    #[test]
    fn binary_blob_is_skipped() {
        // Contains a secret-shaped token but also a null byte → treated as binary, skipped.
        let mut buf = b"sk-abcdefghijklmnopqrstuvwxyz0123456789".to_vec();
        buf.push(0);
        assert_eq!(scan_staged_text(&buf), None);
    }

    #[test]
    fn hit_line_names_path_and_kind_only() {
        let hit = SecretHit {
            path: "src/cfg.rs".into(),
            kinds: "openai-key".into(),
        };
        assert_eq!(hit.line(), "src/cfg.rs: openai-key");
    }
}
