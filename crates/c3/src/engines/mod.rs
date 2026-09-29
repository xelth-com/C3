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
pub mod codex;
pub mod muse;
pub mod subprocess;
pub mod tree_check;

pub use agy::AgyEngine;
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
pub fn scrub_host_markers(cmd: &mut std::process::Command) {
    for (k, _) in std::env::vars_os() {
        if let Some(name) = k.to_str() {
            if c3_core::host::is_host_marker(name) {
                cmd.env_remove(name);
            }
        }
    }
}
