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

/// Scan some staged bytes with the redactor. Returns `Some(kinds)` when it finds a secret, `None`
/// when clean. The bytes are read as lossy UTF-8 first (the redactor is a line scanner), so a
/// binary blob is scanned too — a NUL byte no longer short-circuits to "clean" (Muse,
/// review-2026-10-30: a key tucked into a binary or NUL-bearing blob was slipping through). The
/// kinds are comma-joined in a stable order.
pub fn scan_staged_text(content: &[u8]) -> Option<String> {
    scan_text_for_kinds(&String::from_utf8_lossy(content))
}

/// The redactor's kinds for `text`, `None` when clean. `by_kind` is a `BTreeMap`, so the join is
/// already in a stable (alphabetical) order.
fn scan_text_for_kinds(text: &str) -> Option<String> {
    let report = redact::scan(text);
    if report.by_kind.is_empty() {
        return None;
    }
    let kinds: Vec<&str> = report.by_kind.keys().map(String::as_str).collect();
    Some(kinds.join(", "))
}

/// Scan a commit message with the redactor (`None` when clean). Unlike a staged file, a secret in
/// the message cannot be waved through with `--allow-secret` — the coordinator must rewrite the
/// message (Muse, review-2026-10-30).
pub fn scan_message(message: &str) -> Option<String> {
    scan_text_for_kinds(message)
}

/// Whether a staged diff is a binary diff (git emits `Binary files ... differ` under `-U0`, or a
/// `GIT binary patch` block, instead of `@@` hunks). Such a diff carries no added text lines, so
/// the gate scans the staged blob itself instead.
fn diff_is_binary(diff: &str) -> bool {
    diff.lines()
        .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch")
}

/// Run the gate over every staged file in `repo_root`, skipping any path in `allow`.
/// Returns the refused files (empty = clean). Scans only the ADDED lines of each
/// added/copied/modified/renamed file (`git diff --cached -U0`); deletions carry no added
/// content and are not scanned.
pub fn scan_staged(repo_root: &Path, allow: &[String]) -> Vec<SecretHit> {
    let allow: std::collections::BTreeSet<String> =
        allow.iter().map(|a| a.replace('\\', "/")).collect();
    let mut hits = Vec::new();
    // Fail closed: what cannot be read cannot be shown clean (Astra, review-2026-10-30).
    let Some(paths) = staged_text_paths(repo_root) else {
        hits.push(SecretHit {
            path: "<the staged file list>".into(),
            kinds: UNREADABLE.into(),
        });
        return hits;
    };
    for path in paths {
        if allow.contains(&path) {
            continue;
        }
        // A staged key-container file is refused by its extension, regardless of content: these are
        // usually binary (so the line scanner skips them) yet a `.key`/`.p8`/`.gpg` may be text and
        // still carry private key material. `--allow-secret <path>` is the only way through (Muse,
        // review-2026-10-30).
        if is_key_container(&path) {
            hits.push(SecretHit {
                path,
                kinds: KEY_CONTAINER.into(),
            });
            continue;
        }
        let Some(diff) = staged_diff(repo_root, &path) else {
            hits.push(SecretHit {
                path,
                kinds: UNREADABLE.into(),
            });
            continue;
        };
        // A binary (or NUL-bearing) staged change carries no added text lines, so scan the whole
        // staged blob (`git show :<path>`, lossy); a text change scans only its added lines. Either
        // read failing fails closed (Muse, review-2026-10-30).
        let kinds = if diff_is_binary(&diff) {
            let Some(blob) = staged_blob(repo_root, &path) else {
                hits.push(SecretHit {
                    path,
                    kinds: UNREADABLE.into(),
                });
                continue;
            };
            scan_staged_text(&blob)
        } else {
            scan_staged_text(added_from_diff(&diff).as_bytes())
        };
        if let Some(kinds) = kinds {
            hits.push(SecretHit { path, kinds });
        }
    }
    hits
}

/// Extract the added lines of a unified diff: the `+`-prefixed lines, with the `+` stripped,
/// excluding the `+++ b/<path>` file header. Pure (no git). A brand-new file's diff adds
/// every line, so all of it is returned. (The gate scans this, not the whole staged blob.)
pub fn added_from_diff(diff: &str) -> String {
    // The file header (`diff --git`, `index`, `---`, `+++`) precedes the first `@@` of each file
    // section; inside a hunk every `+` line is content - including one whose content itself
    // starts with `++` (Astra, review-2026-10-30: a `+++` prefix test dropped those).
    let mut out = String::new();
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            in_hunk = false;
            continue;
        }
        if line.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            out.push_str(rest);
            out.push('\n');
        }
    }
    out
}

/// The `kinds` of a staged file whose diff (or the staged list) could not be read: the gate
/// refuses it like a secret, so a git failure never lets content through unscanned.
pub const UNREADABLE: &str = "unreadable (git could not show the staged change)";

/// The `kinds` of a staged key-container file, refused by extension regardless of content.
pub const KEY_CONTAINER: &str = "key-container";

/// File extensions (without the dot, case-insensitive) that name a private-key / keystore
/// container: PKCS#12 (`p12`/`pfx`), DER (`der`), Java keystore (`jks`/`keystore`), a bare private
/// key (`key`/`p8`), a GPG keyring/blob (`gpg`) and a KeePass database (`kdbx`). A staged file with
/// one of these is refused even when it is binary (the line scanner would skip it) or text.
const KEY_CONTAINER_EXTS: &[&str] = &[
    "p12", "pfx", "der", "jks", "keystore", "key", "p8", "gpg", "kdbx",
];

/// Whether `path`'s final extension is a key-container extension (case-insensitive).
fn is_key_container(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    match base.rsplit_once('.') {
        Some((_, ext)) => {
            let ext = ext.to_ascii_lowercase();
            KEY_CONTAINER_EXTS.contains(&ext.as_str())
        }
        None => false,
    }
}

/// The staged, non-deleted paths (`git diff --cached --name-only --diff-filter=ACMRT -z`);
/// `None` when git cannot list them. `T` (typechange, e.g. a file becoming a symlink or vice
/// versa) is included so such an entry is scanned too (Muse, review-2026-10-30).
fn staged_text_paths(repo_root: &Path) -> Option<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMRT",
            "-z",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => Some(
            String::from_utf8_lossy(&o.stdout)
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(|s| s.replace('\\', "/"))
                .collect(),
        ),
        _ => None,
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

/// The staged (index) blob of `path` (`git show :<path>`), raw bytes, or `None` if it cannot be
/// read. Used to scan a binary staged change the unified diff cannot show as text lines.
fn staged_blob(repo_root: &Path, path: &str) -> Option<Vec<u8>> {
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
    fn nul_bearing_blob_is_still_scanned() {
        // A secret-shaped token with a NUL byte is no longer skipped (Muse, review-2026-10-30):
        // lossy UTF-8 keeps the key on its line for the redactor.
        let mut buf = b"sk-abcdefghijklmnopqrstuvwxyz0123456789".to_vec();
        buf.push(0);
        assert_eq!(scan_staged_text(&buf).as_deref(), Some("openai-key"));
    }

    #[test]
    fn diff_is_binary_detects_binary_diffs() {
        assert!(diff_is_binary(
            "diff --git a/x.bin b/x.bin\nindex 1..2 100644\nBinary files a/x.bin and b/x.bin differ\n"
        ));
        assert!(diff_is_binary(
            "diff --git a/x b/x\nGIT binary patch\nliteral 0\n"
        ));
        // A normal text hunk is not binary.
        assert!(!diff_is_binary(
            "diff --git a/f.rs b/f.rs\n@@ -1 +1 @@\n-a\n+b\n"
        ));
    }

    #[test]
    fn scan_message_flags_a_secret_in_the_commit_message() {
        assert_eq!(scan_message("fix: routine change\n"), None);
        assert_eq!(
            scan_message("wip: key = sk-abcdefghijklmnopqrstuvwxyz0123456789\n").as_deref(),
            Some("openai-key")
        );
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
        // Inside a hunk a content line that itself starts with `++` is still an added line
        // (Astra, review-2026-10-30); the next file's header is skipped again.
        let tricky = "diff --git a/k.txt b/k.txt\n\
--- a/k.txt\n\
+++ b/k.txt\n\
@@ -0,0 +1,2 @@\n\
+++key = sk-abcdefghijklmnopqrstuvwxyz0123456789\n\
++ plus\n\
diff --git a/g.txt b/g.txt\n\
--- a/g.txt\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+g\n";
        let added = added_from_diff(tricky);
        assert_eq!(
            added,
            "++key = sk-abcdefghijklmnopqrstuvwxyz0123456789\n+ plus\ng\n"
        );
        assert!(scan_staged_text(added.as_bytes()).is_some());
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
    fn key_container_extensions_are_recognised_case_insensitively() {
        for p in [
            "secrets/server.p12",
            "a/b/keys.PFX",
            "cert.der",
            "store.jks",
            "release.keystore",
            "id_rsa.key",
            "svc.p8",
            "ring.GPG",
            "vault.kdbx",
        ] {
            assert!(is_key_container(p), "{p} should be a key container");
        }
        // Not key containers: a different extension, no extension, or the word only in the stem.
        assert!(!is_key_container("src/keyring.rs"));
        assert!(!is_key_container("notes.md"));
        assert!(!is_key_container("Makefile"));
        assert!(!is_key_container("my.key.txt"));
        // The constant the gate reports.
        assert_eq!(KEY_CONTAINER, "key-container");
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
