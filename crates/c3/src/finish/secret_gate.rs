//! The secret gate: before `c3 finish` commits, the ADDED lines of each staged text file
//! (`git diff --cached -U0 -- <path>`, the `+` lines of this commit's diff — a brand-new
//! file contributes all its lines) pass the pack redactor ([`crate::pack::redact`]). A
//! secret-shaped value refuses the commit; the file and the KIND are reported, never the
//! value, and nothing is unstaged so the coordinator can look. `--allow-secret <path>`
//! (repeatable) lets one reviewed file through.
//!
//! Only the added lines are scanned (not the whole staged blob): a committed test fixture
//! that carries a secret-shaped line must not re-block every later commit that merely
//! touches the same file. A line already in HEAD is not re-flagged; only what this commit
//! introduces is.

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
/// Returns the refused files (empty = clean). Scans only the ADDED lines of each
/// added/copied/modified/renamed file (`git diff --cached -U0`); deletions carry no added
/// content and are not scanned.
pub fn scan_staged(repo_root: &Path, allow: &[String]) -> Vec<SecretHit> {
    let allow: std::collections::BTreeSet<String> =
        allow.iter().map(|a| a.replace('\\', "/")).collect();
    let mut hits = Vec::new();
    for path in staged_text_paths(repo_root) {
        if allow.contains(&path) {
            continue;
        }
        let Some(diff) = staged_diff(repo_root, &path) else {
            continue;
        };
        let added = added_from_diff(&diff);
        if let Some(kinds) = scan_staged_text(added.as_bytes()) {
            hits.push(SecretHit { path, kinds });
        }
    }
    hits
}

/// Extract the added lines of a unified diff: the `+`-prefixed lines, with the `+` stripped,
/// excluding the `+++ b/<path>` file header. Pure (no git). A brand-new file's diff adds
/// every line, so all of it is returned. (The gate scans this, not the whole staged blob.)
pub fn added_from_diff(diff: &str) -> String {
    let mut out = String::new();
    for line in diff.lines() {
        if line.starts_with("+++") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            out.push_str(rest);
            out.push('\n');
        }
    }
    out
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

/// The staged, zero-context unified diff of `path` against HEAD
/// (`git diff --cached -U0 -- <path>`), or `None` if it cannot be read. A binary file's diff
/// carries no `+` lines ("Binary files ... differ"), so it contributes nothing to the scan.
fn staged_diff(repo_root: &Path, path: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--cached", "-U0", "--"])
        .arg(path)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
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
    fn added_from_diff_keeps_plus_lines_only() {
        // A real `git diff --cached -U0` shape: metadata, a `+++` header, a hunk header,
        // one removed line and two added lines.
        let diff = "diff --git a/f.rs b/f.rs\n\
index 111..222 100644\n\
--- a/f.rs\n\
+++ b/f.rs\n\
@@ -1 +1,2 @@\n\
-let old = 1;\n\
+let clean = 2;\n\
+let other = 3;\n";
        let added = added_from_diff(diff);
        assert_eq!(added, "let clean = 2;\nlet other = 3;\n");
        // The `+++` header and the removed `-` line never appear.
        assert!(!added.contains("f.rs"));
        assert!(!added.contains("old"));
    }

    #[test]
    fn gate_ignores_an_unchanged_fixture_line_but_catches_a_new_secret() {
        // The OLD line (a committed fixture secret) shows as a `-`/context line, never `+`,
        // so a diff that only adds a clean line passes...
        let clean_diff = "--- a/fixtures.rs\n\
+++ b/fixtures.rs\n\
@@ -1,1 +1,2 @@\n\
 const FIXTURE: &str = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\";\n\
+const NOTE: &str = \"this is fine\";\n";
        assert_eq!(
            scan_staged_text(added_from_diff(clean_diff).as_bytes()),
            None
        );

        // ...but a diff that ADDS a secret line is caught.
        let secret_diff = "--- a/fixtures.rs\n\
+++ b/fixtures.rs\n\
@@ -1,1 +1,2 @@\n\
 const NOTE: &str = \"this is fine\";\n\
+const KEY: &str = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\";\n";
        assert_eq!(
            scan_staged_text(added_from_diff(secret_diff).as_bytes()).as_deref(),
            Some("openai-key")
        );
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
