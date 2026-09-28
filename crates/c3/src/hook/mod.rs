//! The SessionStart hook line behind `c3 hook` (milestone 3): the availability
//! summary of `providers --short` without any network call, always exit 0.
//!
//! Mirrors `codex-consult-hook.ps1`. The plugin hook shells out to
//! `codex-providers.ps1 -Short -Json -NoNetwork -CollabDir <dir>` and prints the `line`
//! field of the JSON it emits; C3 calls [`crate::providers::short_line`] in-process
//! instead (the cli-surface doc's sanctioned alternative), so the same one line reaches
//! stdout with no subprocess. The contract, line for line:
//!
//! * Codex CLI not on PATH -> the fixed `codex CLI not found on PATH` line.
//! * Otherwise the availability line, but only if it is well-formed (`^codex-consult: `
//!   and single-line, exactly the plugin's guard); anything else, or any failure inside
//!   the check, becomes `codex-consult: reviewer check failed - <reason>`.
//! * Always exactly one line to stdout, always exit 0.

use c3_core::one_line;

use crate::providers::{resolve_codex_launcher, short_line, Options};

/// The exact message the plugin prints when `codex` is not on PATH.
const CODEX_MISSING: &str =
    "codex-consult: codex CLI not found on PATH - follow the setup-providers skill before consulting a reviewer";

/// Compute the SessionStart line, print it, and return 0. `collab_dir` is the value of
/// `--collab-dir` (endpoint health is read from every task ledger under it; no network,
/// no lock, nothing written).
pub fn run(collab_dir: &str) -> i32 {
    println!("{}", line(collab_dir));
    0
}

/// The line, without printing it (so the behaviour is unit-testable).
pub fn line(collab_dir: &str) -> String {
    format!(
        "{}{}",
        availability_line(collab_dir),
        detached_phrase(collab_dir)
    )
}

/// The availability half of the hook line (the provider check).
fn availability_line(collab_dir: &str) -> String {
    // `Get-Command codex`: the plugin refuses to run the provider check at all when the
    // launcher is not on PATH. `resolve_codex_launcher("")` returns an empty string in
    // exactly that case (an explicit override would be an error, but the hook passes
    // none).
    match resolve_codex_launcher("") {
        Ok(l) if l.is_empty() => return CODEX_MISSING.to_string(),
        Ok(_) => {}
        Err(e) => return reviewer_check_failed(&e),
    }
    let opts = Options {
        collab_dir: collab_dir.to_string(),
        no_network: true,
        short: true,
        ..Default::default()
    };
    match short_line(&opts) {
        Ok(text) => {
            // The plugin's own guard on the extracted `line`: it must be the availability
            // line and it must be a single line.
            if text.starts_with("codex-consult: ") && !text.contains(['\r', '\n']) {
                text
            } else {
                reviewer_check_failed(&text)
            }
        }
        Err(e) => reviewer_check_failed(&e),
    }
}

/// The SessionStart phrase over every detached run of the collab root (`Get-DetachedPhrase`).
fn detached_phrase(collab_dir: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo_root = crate::providers::resolve_repo_root(&cwd);
    let collab_root = crate::providers::resolve_collab_root(&repo_root, collab_dir);
    crate::consult::detach::detached_phrase(&collab_root, chrono::Utc::now(), 24)
}

/// The plugin's catch-branch wording: collapse whitespace, truncate to 120 characters
/// (117 + an ellipsis), prefix `codex-consult: reviewer check failed - `.
fn reviewer_check_failed(reason: &str) -> String {
    let mut msg = one_line(reason);
    if msg.chars().count() > 120 {
        let truncated: String = msg.chars().take(117).collect();
        msg = format!("{truncated}...");
    }
    format!("codex-consult: reviewer check failed - {msg}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_missing_message() {
        assert!(CODEX_MISSING.starts_with("codex-consult: codex CLI not found on PATH"));
    }

    #[test]
    fn reviewer_check_failed_collapses_and_truncates() {
        let msg = reviewer_check_failed("a  b\n c");
        assert_eq!(msg, "codex-consult: reviewer check failed - a b c");
        let long = "x".repeat(200);
        let out = reviewer_check_failed(&long);
        let body = out
            .strip_prefix("codex-consult: reviewer check failed - ")
            .unwrap();
        assert_eq!(body.chars().count(), 120);
        assert!(body.ends_with("..."));
    }
}
