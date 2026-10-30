//! `c3 scout` — snapshot an EXTERNAL directory for context (port spec, Part C).
//! The logic lives in [`crate::pack::scout`].

use std::path::PathBuf;

use clap::Args;

use crate::pack::scout::{self, ScoutOpts};
use crate::providers;

/// Arguments of `c3 scout`.
#[derive(Args, Debug)]
pub struct ScoutArgs {
    /// The directory to scout (external; a path inside this repository is refused).
    pub path: String,
    /// Depth 0-9: 0 tree only, 5 skeleton, 9 full source (default 0).
    #[arg(long, default_value_t = 0)]
    pub depth: u8,
}

/// Run `c3 scout`. Returns the process exit code.
pub fn run(args: ScoutArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let target = PathBuf::from(&args.path);
    let target = if target.is_absolute() {
        target
    } else {
        cwd.join(target)
    };

    let opts = ScoutOpts {
        repo_root,
        target,
        depth: args.depth,
    };
    match scout::run_scout(&opts) {
        Ok(path) => {
            println!("Scout: {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("c3 scout: {e}");
            1
        }
    }
}
