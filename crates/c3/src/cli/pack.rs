//! `c3 pack` — build a reviewer pack (and its `.pack.json` sidecar) for the `http` engine
//! (milestone 7; milestone 7b consumes it). The pipeline lives in
//! [`crate::pack::reviewer`].

use std::path::PathBuf;

use clap::Args;

use crate::pack::reviewer::{self, PackOpts};
use crate::providers;

/// Arguments of `c3 pack`.
#[derive(Args, Debug, Default)]
pub struct PackArgs {
    /// The one-page brief file (path relative to the repo root, or absolute).
    #[arg(long)]
    pub brief: String,
    /// Focus file(s): path or glob, included in full (repeatable).
    #[arg(long)]
    pub focus: Vec<String>,
    /// Token budget for the periphery (0 = none).
    #[arg(long, default_value_t = 0)]
    pub budget: usize,
    /// Task slug whose `findings.json` supplies the open-findings snapshot.
    #[arg(long)]
    pub task: Option<String>,
    /// Output path for the pack; the `.pack.json` sidecar is written beside it.
    #[arg(long)]
    pub out: String,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
}

/// Build the reviewer pack and its sidecar. Returns the process exit code.
pub fn run(args: PackArgs) -> i32 {
    if args.out.trim().is_empty() {
        eprintln!("c3 pack: --out is required");
        return 1;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &args.collab_dir);

    let out = PathBuf::from(&args.out);
    let opts = PackOpts {
        repo_root,
        collab_root,
        brief: PathBuf::from(&args.brief),
        focus: args.focus,
        budget: args.budget,
        task: args.task,
        out: out.clone(),
        max_file_size: 2 * 1024 * 1024,
    };

    let pack = match reviewer::build(&opts) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("c3 pack: {e}");
            return 1;
        }
    };

    match reviewer::write(&pack, &out) {
        Ok(side) => {
            let kb = (pack.size_bytes / 1024).max(1);
            println!(
                "Reviewer pack: {} ({kb} KB | {} focus | {} periphery | {} redaction(s))",
                out.display(),
                pack.focus_files.len(),
                pack.periphery_shown,
                pack.redactions
            );
            println!("Sidecar: {}", side.display());
            0
        }
        Err(e) => {
            eprintln!("c3 pack: {e}");
            1
        }
    }
}
