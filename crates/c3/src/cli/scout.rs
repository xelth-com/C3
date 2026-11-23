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

    // The scout file goes into C3's folder: make sure it is in place first.
    let state_dir = match crate::state_dir::ensure(&repo_root, None) {
        Ok(e) => {
            crate::state_dir::print_notes("c3 scout", &e.notes(), true);
            e.dir
        }
        Err(e) => {
            eprintln!("c3 scout: {e}");
            return 1;
        }
    };

    let opts = ScoutOpts {
        repo_root,
        state_dir,
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
