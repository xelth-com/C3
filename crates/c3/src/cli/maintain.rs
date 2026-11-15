//! `c3 maintain` — C3 keeps the engine CLIs and the seated model ids fresh by itself. The logic
//! lives in [`crate::maintain`]; this is the clap surface and the dispatch.

use clap::Args;

use crate::maintain::{self, PassOpts};

/// Arguments of `c3 maintain`.
#[derive(Args, Debug, Default)]
pub struct MaintainArgs {
    /// Run one maintenance pass now, in the foreground (otherwise report from the stored state).
    #[arg(long)]
    pub run: bool,
    /// Run ONLY the two-turn smoke on each selected engine's chosen target (no check/update/state).
    #[arg(long)]
    pub smoke: bool,
    /// Touch only this engine (repeatable); without it, every engine.
    #[arg(long = "engine")]
    pub engines: Vec<String>,
    /// Probe versions and model lists and say what a real pass would do; install/edit nothing. With
    /// `--smoke`, print the chosen target per engine without a model turn.
    #[arg(long)]
    pub dry_run: bool,
    /// Ignore the interval and the cool-down.
    #[arg(long)]
    pub force: bool,
}

/// Run `c3 maintain`. `--run` runs a pass; otherwise it reports the stored state. Exit 0, except 1
/// when a pass cannot start (no codex home, or another pass is already running).
pub fn run(args: MaintainArgs) -> i32 {
    if args.smoke {
        maintain::run_smoke_only(args.engines, args.dry_run)
    } else if args.run {
        maintain::run_pass(PassOpts {
            engines: args.engines,
            dry_run: args.dry_run,
            force: args.force,
        })
    } else {
        maintain::run_report()
    }
}
