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
pub mod muse;
pub mod subprocess;
pub mod tree_check;

pub use agy::AgyEngine;
pub use claude::ClaudeEngine;
pub use codex::CodexEngine;
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
pub fn scrub_host_markers(cmd: &mut std::process::Command) {
    for (k, _) in std::env::vars_os() {
        if let Some(name) = k.to_str() {
            if c3_core::host::is_host_marker(name) || c3_core::test_hooks::is_test_var(name) {
                cmd.env_remove(name);
            }
        }
    }
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
