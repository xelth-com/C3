//! `c3 fetch` — write the full contents of matching files of THIS repository (port spec,
//! Part C). The logic lives in [`crate::pack::scout`].

use std::path::PathBuf;

use clap::Args;

use crate::pack::scout::{self, FetchOpts};
use crate::providers;

/// Arguments of `c3 fetch`.
#[derive(Args, Debug)]
pub struct FetchArgs {
    /// One or more repo-relative globs selecting the files to include (e.g. `**/api.rs`).
    #[arg(required = true)]
    pub glob: Vec<String>,
}

/// Run `c3 fetch`. Returns the process exit code.
pub fn run(args: FetchArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);

    let opts = FetchOpts {
        repo_root,
        globs: args.glob.iter().map(|g| g.replace('\\', "/")).collect(),
    };
    match scout::run_fetch(&opts) {
        Ok(r) => {
            println!(
                "Fetch: {} ({} file(s) | {} redaction(s))",
                r.path.display(),
                r.files,
                r.redactions
            );
            0
        }
        Err(e) => {
            eprintln!("c3 fetch: {e}");
            1
        }
    }
}
