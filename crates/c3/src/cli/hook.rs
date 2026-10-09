//! `c3 hook` — the one-line SessionStart summary (milestone 3). Mirrors
//! `codex-consult-hook.ps1`: always exits 0, never touches the network; the logic
//! lives in [`crate::hook`].

use clap::Args;

/// Arguments of `c3 hook` (see `docs/port/cli-surface.md` §4). The plugin hook has one
/// parameter, `-CollabDir`.
#[derive(Args, Debug, Default)]
pub struct HookArgs {
    /// Where consultations are stored. Relative paths resolve against the git repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// (wave 27c, D13) The command the pointer line names for a host without skills (default:
    /// this binary's `consult --explain coordinate`). The harness shim passes the plugin's form
    /// (`powershell -NoProfile -ExecutionPolicy Bypass -File "<scripts>\codex-consult.ps1"
    /// -Explain coordinate`).
    #[arg(long, default_value = "", hide = true)]
    pub explain_command: String,
}

/// Print the SessionStart line and return the process exit code (always 0, as the
/// plugin hook is: a session must never fail because a reviewer check did).
pub fn run(args: HookArgs) -> i32 {
    crate::hook::run(&args.collab_dir, &args.explain_command)
}
