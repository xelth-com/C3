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

/// The one sentence of the operator's waiting rule (the SessionStart scripts in `plugin/hooks/`
/// print the whole rule; the `coordinate` skill states it in its first lines) that `c3 consult
/// --detach` and `c3 work --detach` print on stderr when they start - where the waiting begins.
pub const WAITING_NOTICE: &str = "waiting rule (every session, with or without c3): before you end a turn while this runs, call ScheduleWakeup with delaySeconds 1800 and a one-line prompt saying what to check - a wake-up inside the cache lifetime keeps your context cached, a notice after a long idle gap makes you re-read it at full price";

/// The exact message the plugin prints when `codex` is not on PATH.
const CODEX_MISSING: &str =
    "codex-consult: codex CLI not found on PATH - follow the setup-providers skill before consulting a reviewer";

/// Compute the SessionStart lines, print them, and return 0. `collab_dir` is the value of
/// `--collab-dir` (endpoint health is read from every task ledger under it; no network,
/// no lock, nothing written); `explain_command` the pointer line's command (`""`: this binary's
/// own `consult --explain coordinate`). `source` is the SessionStart source (`startup`/`resume`/
/// `compact`); only `compact` adds the (R30) re-hydration line, so startup/resume stay unchanged.
pub fn run(collab_dir: &str, explain_command: &str, source: &str) -> i32 {
    println!("{}", line(collab_dir));
    println!("{}", pointer_line(explain_command));
    // (R30 wave-boundary compaction) after a compaction the coordinator has lost the in-context
    // memory of who it is coordinating; restore it from the record it would itself write
    // (CODEX_CONSULT_COORDINATOR, resolved against the roster) so it can pick its detached runs
    // (the first line already names them) back up. Printed ONLY for the `compact` source.
    if source.eq_ignore_ascii_case("compact") {
        if let Some(l) = compaction_line() {
            println!("{l}");
        }
    }
    // (wave 11) one extra line when C3 has a newer release on record: the cache is refreshed if it
    // is stale (within the 3 s budget) and the check is on; any error, a disabled check or an
    // up-to-date install prints nothing, so startup/resume/compact output is otherwise unchanged.
    if let Some(l) = crate::update::hook_line(chrono::Utc::now()) {
        println!("{l}");
    }
    // (c3 maintain) one extra line when a finished maintenance pass has not been reported yet; the
    // hook also starts a detached pass when one is due and the feature is on. Silent on any error,
    // nothing printed when there is nothing to say, so startup/resume/compact output is otherwise
    // unchanged and never blocked.
    if let Some(l) = crate::maintain::hook_line(chrono::Utc::now()) {
        println!("{l}");
    }
    // (repo model settings) C3 repairs this repository's stale agent model pins and lets AGENTS.md
    // follow the roster's successor, printing one line only when it changed something or could not
    // make a repair. Default ON (`C3_REPO_MODELS=off` / `"repo_models": false`); nothing inside a C3
    // child. No process at all is started (the root is found by walking up to `.git`), nothing is
    // written outside the repo root, a file is replaced by rename, never written through.
    if let Some(l) = crate::setup::repo_models::hook_line() {
        println!("{l}");
    }
    // (branches) one line when a branch or worktree has no recorded state, the default branch is
    // unknown, or the start folder is not on the default branch. Default ON (`C3_BRANCH_CHECK=off`
    // / `"branch_check": false`). Read-only (git's optional locks off) and cheap (ancestry only, no
    // `git cherry`, no `git status`), every error swallowed, nothing inside a C3 child or outside a
    // git repository - and (0.2.5, F02-6) all of its git calls share ONE deadline: past it the line
    // is dropped, its git children are killed (R2, F13-5) and the hook goes on.
    if let Some(l) = branch_line_within(BRANCH_LINE_DEADLINE) {
        println!("{l}");
    }
    // (0.2.5, red CI) one line when the latest completed CI run of the default branch failed, read
    // through `gh`: at most one call per repository and day (the attempt is cached in C3's state
    // directory), silent without `gh` / a GitHub remote / a login, with `C3_CI_CHECK=off` /
    // `"ci_check": false`, and inside a C3 child. (R4, F16-3) Every git and `gh` process of the check
    // runs under ONE deadline (`ci_check::HOOK_DEADLINE`, 3 s) and is killed when it fires.
    if let Some(l) = crate::ci_check::hook_line() {
        println!("{l}");
    }
    0
}

/// (0.2.5, F02-6) The one deadline of the branch line's git calls.
pub const BRANCH_LINE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);

/// The branch line, or `None` when it is not ready within `deadline` (a slow or stalled git, a slow
/// file system): the scan runs on a thread of its own ([`crate::bounded::within`]), and one that
/// misses the deadline is left behind - its result is never printed, and the process ends without
/// waiting for it. (0.2.5 R2, F13-5) Its git children are stopped first: killed, and no further one
/// started, before the hook goes on, so none outlives `c3 hook`.
fn branch_line_within(deadline: std::time::Duration) -> Option<String> {
    crate::bounded::within(deadline, crate::setup::branches::hook_line)
}

/// (R30) The re-hydration line for the `compact` SessionStart source: the coordinator identity the
/// bridge records, resolved from the environment against the roster exactly as a real run resolves
/// it (no network, nothing written). `None` when no coordinator identity is on record
/// (CODEX_CONSULT_COORDINATOR unset or host-only) or the value does not parse - nothing to restore.
fn compaction_line() -> Option<String> {
    let roster = crate::providers::read_reviewer_roster().ok();
    let entries = roster.as_ref().filter(|r| r.exists).map(|r| &r.entries[..]);
    let coord = crate::consult::orchestrate::resolve_coordinator(entries).ok()?;
    coord.provider.as_ref()?;
    let id = c3_core::host::format_coordinator_id(&coord);
    Some(format!(
        "codex-consult: after compaction - coordinator {id} recorded (CODEX_CONSULT_COORDINATOR); re-read the c3:coordinate skill, then `c3 consult --status` the detached runs above before the next wave"
    ))
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

    fn quick_line() -> Option<String> {
        Some("c3: quick".to_string())
    }

    fn stalled_line() -> Option<String> {
        std::thread::sleep(std::time::Duration::from_secs(30));
        Some("c3: too late".to_string())
    }

    // (0.2.5, F02-6) The branch line's work shares one deadline: a ready line is printed, a stalled
    // one is dropped when the deadline passes (the hook does not wait for it).
    #[test]
    fn a_hook_line_past_its_deadline_is_dropped() {
        let d = std::time::Duration::from_millis(300);
        assert_eq!(
            crate::bounded::within(std::time::Duration::from_secs(10), quick_line).as_deref(),
            Some("c3: quick")
        );
        let start = std::time::Instant::now();
        assert_eq!(crate::bounded::within(d, stalled_line), None);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(BRANCH_LINE_DEADLINE, std::time::Duration::from_secs(2));
        assert_eq!(
            crate::ci_check::HOOK_DEADLINE,
            std::time::Duration::from_secs(3)
        );
    }

    /// A line whose work is a stalled git-like child, started through the branch scan's runner.
    fn stalled_child_line() -> Option<String> {
        let cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = std::process::Command::new("sleep");
            c.arg("30");
            c
        };
        let _ = crate::setup::branches::run_registered(cmd);
        Some("c3: too late".to_string())
    }

    // (0.2.5 R2, F13-5) When the branch line's deadline fires, the scan's child is killed before
    // the hook goes on: nothing it started is still running, and the line is dropped.
    #[test]
    fn the_branch_deadline_stops_the_scans_children() {
        let children = std::sync::Arc::new(crate::setup::branches::ScanChildren::default());
        let start = std::time::Instant::now();
        let line = crate::bounded::within_stopping(
            std::time::Duration::from_millis(1500),
            stalled_child_line,
            std::sync::Arc::clone(&children),
        );
        assert_eq!(line, None);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "{:?}",
            start.elapsed()
        );
        let started = children.started_pids();
        assert_eq!(started.len(), 1, "the child never started: {started:?}");
        for pid in started {
            assert!(
                crate::liveness::proc::process_start_iso(pid).is_none(),
                "the scan's child {pid} outlived the deadline"
            );
        }
        assert!(children.running_pids().is_empty());
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
