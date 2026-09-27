//! `c3 scoreboard` — the per-reviewer usefulness scoreboard (milestone 3). Mirrors
//! `codex-scoreboard.ps1`; the logic lives in [`crate::scoreboard`].

use clap::Args;

/// Arguments of `c3 scoreboard` (see `docs/port/cli-surface.md` §3).
#[derive(Args, Debug, Default)]
pub struct ScoreboardArgs {
    /// Where consultations are stored. Relative paths resolve against the git repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,

    /// One task only (its directory under `<collab-dir>`); empty = every task.
    #[arg(long, default_value = "")]
    pub task: String,

    /// An array of row objects instead of the printed table.
    #[arg(long)]
    pub json: bool,
}

/// Print the scoreboard and return the process exit code.
pub fn run(args: ScoreboardArgs) -> i32 {
    crate::scoreboard::run(&args.collab_dir, &args.task, args.json)
}
