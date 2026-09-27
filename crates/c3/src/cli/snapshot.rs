//! `c3 snapshot` — take a repository snapshot, optionally as a git delta (milestone 7).
//! The pipeline lives in [`crate::pack::snapshot`]; this owns the clap surface and the
//! one-line summary. C3 writes files only and never commits (D13).

use std::path::PathBuf;

use clap::Args;

use crate::pack::snapshot::{self, SnapshotOpts};
use crate::providers;

/// Arguments of `c3 snapshot`.
#[derive(Args, Debug, Default)]
pub struct SnapshotArgs {
    /// Output path. Default: `<collab>/.c3/snapshots/<repo>_<ts>_<rev7>[_upN]_<sizeKB>kb.md`.
    #[arg(long)]
    pub out: Option<String>,
    /// Snapshot only what changed since C3's anchor (the last full snapshot's commit).
    #[arg(long)]
    pub delta: bool,
    /// Depth 0-9: 0 tree only, 1-4 truncate, 5-6 skeleton, 7-9 full (default 9).
    #[arg(long, default_value_t = 9)]
    pub depth: u8,
    /// Token budget; periphery is trimmed to fit, focus files are kept whole (0 = none).
    #[arg(long, default_value_t = 0)]
    pub budget: usize,
    /// Keep matching files whole regardless of depth/budget (repeatable glob).
    #[arg(long)]
    pub focus: Vec<String>,
    /// Omit the directory tree.
    #[arg(long)]
    pub no_tree: bool,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
}

/// Build the snapshot and print a one-line summary. Returns the process exit code.
pub fn run(args: SnapshotArgs) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &args.collab_dir);

    let opts = SnapshotOpts {
        repo_root,
        collab_root,
        out: args.out.map(PathBuf::from),
        delta: args.delta,
        depth: args.depth,
        budget: args.budget,
        focus: args.focus,
        tree: !args.no_tree,
        max_file_size: 2 * 1024 * 1024,
    };

    match snapshot::run(&opts) {
        Ok(r) => {
            let kb = (r.size_bytes / 1024).max(1);
            let toks = if r.tokens < 1000 {
                r.tokens.to_string()
            } else {
                format!("{:.1}k", r.tokens as f64 / 1000.0)
            };
            println!(
                "Snapshot: {} ({kb} KB | ~{toks} tokens | {} file(s) | {} redaction(s))",
                r.path.display(),
                r.files,
                r.redactions
            );
            0
        }
        Err(e) => {
            eprintln!("c3 snapshot: {e}");
            1
        }
    }
}
