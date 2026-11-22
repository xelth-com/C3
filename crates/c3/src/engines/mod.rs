//! Engine adapters (`codex`, `agy`, `muse`, `http`): one attempt per call, the CLIs
//! wrapped exactly as the plugin drives them (argv from `c3_core::engine`, prompt
//! delivery per engine, events capture, timeout kill and continuation, denial
//! retry, format repair, partial salvage), the `http` engine sending one
//! OpenAI-compatible request.
//!
//! Milestone 2c implements the codex adapter ([`codex`]) end to end on top of the shared
//! subprocess launcher ([`subprocess`]); agy/muse reuse the same launcher and land next,
//! http at milestone 7.

pub mod agy;
pub mod claude;
pub mod claude_auth;
pub mod codex;
pub mod grok;
pub mod kimi;
pub mod muse;
pub mod subprocess;
pub mod tree_check;
pub mod worktree;

pub use agy::AgyEngine;
pub use claude::ClaudeEngine;
pub use codex::CodexEngine;
pub use grok::GrokEngine;
pub use kimi::KimiEngine;
pub use muse::MuseEngine;

/// (wave 27 / 27b) Remove the host markers from a child process's environment before it is
/// launched (`Hide-HostMarkers` / `Remove-HostMarkersFromStartInfo`): every variable of THIS
/// process whose name is a host marker (`c3_core::host::is_host_marker`) is removed from the
/// child's inherited environment, so a reviewer CLI or a launcher probe never inherits the
/// coordinator's session identity or its messaging socket and token. Exact names only — the
/// operator's own `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_PLUGIN_ROOT` and provider keys survive.
///
/// The NAMES removed are recorded in the ledger (`child_env_scrubbed`); no value is ever logged.
///
/// (wave 28b, D10) The test-mode variables go too (`Hide-HostMarkers -TestVars`,
/// `Remove-HostMarkersFromStartInfo`): `CODEX_CONSULT_TEST_MODE` and every `CODEX_CONSULT_TEST_*`
/// are the BRIDGE's switches - an engine child (codex/agy/muse) and a launcher probe (`--version`,
/// `login status`, `models`) never inherit them. They are not listed in `child_env_scrubbed`. The
/// bridge's own processes (a panel member re-exec, the detached background) keep them.
///
/// The child is also MARKED: [`C3_CHILD_ENV`] is set to `1`, so a `c3` command that runs inside the
/// child - the SessionStart hook of a reviewer or worker on the `claude` engine - can tell it was
/// started by C3 and keep its hands off the repository (see `setup::repo_models::hook_line`).
pub fn scrub_host_markers(cmd: &mut std::process::Command) {
    for (k, _) in std::env::vars_os() {
        if let Some(name) = k.to_str() {
            if c3_core::host::is_host_marker(name) || c3_core::test_hooks::is_test_var(name) {
                cmd.env_remove(name);
            }
        }
    }
    cmd.env(C3_CHILD_ENV, "1");
}

/// The variable C3 sets (to `1`) in the environment of every reviewer CLI, worker CLI and launcher
/// probe it starts - the claude engine's allow-listed environment
/// ([`c3_core::claude::child_environment`]) and its `auth status` / `--version` probes included -
/// and of its own detached maintenance pass. A `c3` process that sees it is running INSIDE such a
/// child: a reviewer is read-only and a worker's change is measured against its scope, so nothing
/// there may edit the repository on C3's own initiative.
pub use c3_core::host::C3_CHILD_ENV;

/// Whether this process runs inside a child that C3 started ([`C3_CHILD_ENV`] set to a value that
/// does not switch it off - see [`c3_child_value_marks`]).
pub fn in_c3_child() -> bool {
    std::env::var_os(C3_CHILD_ENV).is_some_and(|v| c3_child_value_marks(&v))
}

/// Whether a [`C3_CHILD_ENV`] value marks a C3 child. Fail-safe: any non-empty value does (the mark
/// only makes C3 read-only), except an explicit off - `0`, `false`, `off` or `no`, any case, spaces
/// around ignored.
pub fn c3_child_value_marks(v: &std::ffi::OsStr) -> bool {
    let s = v.to_string_lossy();
    let s = s.trim();
    !s.is_empty()
        && !["0", "false", "off", "no"]
            .iter()
            .any(|off| s.eq_ignore_ascii_case(off))
}

/// (wave 27c, D3) The transactional host-marker hide is fail-closed: a removal that cannot happen
/// aborts the whole start (a child never launches with only part of the markers hidden). C3 removes
/// the markers from the CHILD command, which cannot fail, so the only failure mode is the test hook
/// `CODEX_CONSULT_TEST_HIDE_FAIL=<name>` (test mode only): when it names a host marker present in
/// this process's environment, the start is refused. Returns the message body
/// `host markers could not be hidden (<name>: <why>)`, or `None` when nothing forces a failure.
pub fn host_marker_hide_failure() -> Option<String> {
    let fail_on = c3_core::test_hooks::hook("CODEX_CONSULT_TEST_HIDE_FAIL")?;
    let fail_on = fail_on.trim();
    if fail_on.is_empty() {
        return None;
    }
    for (k, _) in std::env::vars_os() {
        if let Some(name) = k.to_str() {
            if name.eq_ignore_ascii_case(fail_on) && c3_core::host::is_host_marker(name) {
                return Some(format!(
                    "host markers could not be hidden ({name}: the removal was refused (test hook CODEX_CONSULT_TEST_HIDE_FAIL))"
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod c3_child_value_tests {
    use super::c3_child_value_marks;
    use std::ffi::OsStr;

    #[test]
    fn an_explicit_off_value_is_not_a_c3_child() {
        for v in ["1", "yes", "true", "anything"] {
            assert!(c3_child_value_marks(OsStr::new(v)), "{v} marks a child");
        }
        for v in ["", "  ", "0", "false", "OFF", " no "] {
            assert!(!c3_child_value_marks(OsStr::new(v)), "{v:?} does not");
        }
    }
}
