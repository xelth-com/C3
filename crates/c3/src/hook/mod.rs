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
//! * Then (wave 27, R13 D5) ONE pointer line, always, whatever the first line says - (wave 27c,
//!   D13) with a command runnable as written on a host that substitutes nothing (wave 2e, F11-4:
//!   by default `& "<this c3>" consult --explain coordinate` on Windows - PowerShell -, `'<this
//!   c3>' consult --explain coordinate` elsewhere) - and (wave 28, R17) ending with the telemetry
//!   switch: `codex-consult: coordinator rules - skill codex-consult:coordinate (or <command>);
//!   telemetry: on|off`.
//! * Always exactly two lines to stdout, always exit 0.

use c3_core::one_line;

use crate::providers::{resolve_codex_launcher, short_line, Options};

/// The exact message the plugin prints when `codex` is not on PATH.
const CODEX_MISSING: &str =
    "codex-consult: codex CLI not found on PATH - follow the setup-providers skill before consulting a reviewer";

/// Compute the SessionStart lines, print them, and return 0. `collab_dir` is the value of
/// `--collab-dir` (endpoint health is read from every task ledger under it; no network,
/// no lock, nothing written); `explain_command` the pointer line's command (`""`: this binary's
/// own `consult --explain coordinate`).
pub fn run(collab_dir: &str, explain_command: &str) -> i32 {
    println!("{}", line(collab_dir));
    println!("{}", pointer_line(explain_command));
    0
}

/// (wave 27, R13 D5 / 27c D13 / 28 R17) The pointer to the coordinator's rules: the skill, else a
/// command that prints it, then the telemetry switch (`CODEX_CONSULT_TELEMETRY`, the environment
/// only).
pub fn pointer_line(explain_command: &str) -> String {
    let cmd = if explain_command.trim().is_empty() {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "c3".to_string());
        default_explain_command(&exe, cfg!(windows))
    } else {
        explain_command.trim().to_string()
    };
    format!(
        "codex-consult: coordinator rules - skill codex-consult:coordinate (or {cmd}); telemetry: {}",
        crate::telemetry::switch(None).text()
    )
}

/// (wave 2e, F11-4) The default pointer command - this binary's `consult --explain coordinate` -
/// runnable as written in the host's shell, as the plugin's `powershell -NoProfile -ExecutionPolicy
/// Bypass -File "<script>" -Explain coordinate` is: on Windows a PowerShell invocation (the call
/// operator `&` before the double-quoted path - a quoted path followed by arguments is a parse
/// error there - with `` ` ``, `$` and the double quotes PowerShell knows escaped by a backtick); on
/// Unix a POSIX shell's single-quoted path (`'` written `'\''`).
pub fn default_explain_command(exe: &str, windows: bool) -> String {
    if windows {
        let mut quoted = String::with_capacity(exe.len());
        for c in exe.chars() {
            if matches!(c, '`' | '$' | '"' | '\u{201C}' | '\u{201D}' | '\u{201E}') {
                quoted.push('`');
            }
            quoted.push(c);
        }
        format!("& \"{quoted}\" consult --explain coordinate")
    } else {
        format!(
            "'{}' consult --explain coordinate",
            exe.replace('\'', "'\\''")
        )
    }
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
    fn pointer_line_names_the_command_and_the_switch() {
        let l = pointer_line("powershell -NoProfile -ExecutionPolicy Bypass -File \"C:\\s\\codex-consult.ps1\" -Explain coordinate");
        assert!(l.starts_with("codex-consult: coordinator rules - skill codex-consult:coordinate (or powershell -NoProfile -ExecutionPolicy Bypass -File \"C:\\s\\codex-consult.ps1\" -Explain coordinate); telemetry: "), "{l}");
        assert!(
            l.ends_with("; telemetry: on") || l.ends_with("; telemetry: off"),
            "{l}"
        );
        let d = pointer_line("");
        assert!(
            d.contains("consult --explain coordinate); telemetry: "),
            "{d}"
        );
        if cfg!(windows) {
            assert!(d.contains("(or & \""), "{d}");
        }
    }

    // (wave 2e, F11-4) the default command: PowerShell's call operator and an escaped
    // double-quoted path on Windows, a single-quoted path on Unix.
    #[test]
    fn the_default_explain_command_is_runnable_in_the_host_shell() {
        assert_eq!(
            default_explain_command(r"C:\Program Files\c3\c3.exe", true),
            r#"& "C:\Program Files\c3\c3.exe" consult --explain coordinate"#
        );
        assert_eq!(
            default_explain_command(r"C:\a $b`c\c3.exe", true),
            r#"& "C:\a `$b``c\c3.exe" consult --explain coordinate"#
        );
        assert_eq!(
            default_explain_command("/opt/my c3/c3", false),
            "'/opt/my c3/c3' consult --explain coordinate"
        );
        assert_eq!(
            default_explain_command("/opt/it's/c3", false),
            r"'/opt/it'\''s/c3' consult --explain coordinate"
        );
    }

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
