//! `c3 fail` — mark a task BLOCKED/FAILED: write the report and journal (and, with `--snapshot`,
//! an emergency `--fail` snapshot), with no staging and no commit (port spec, Part A).

use std::path::PathBuf;

use clap::Args;

use crate::cli::finish::resolve_status;
use crate::finish::{self, FailOpts};
use crate::providers;

/// Arguments of `c3 fail`.
#[derive(Args, Debug)]
pub struct FailArgs {
    /// The agent report written to `.c3/last-report.md` and the journal.
    #[arg(long, allow_hyphen_values = true)]
    pub status: Option<String>,
    /// Read the agent report from a file instead of `--status`.
    #[arg(long)]
    pub status_file: Option<String>,
    /// How many snapshots to keep in `.c3/snapshots/` (with `--snapshot`).
    #[arg(long, default_value_t = 10)]
    pub keep: usize,
    /// Also write an emergency snapshot into `.c3/snapshots/` (and `.c3/lastsnapshot/`) - for a
    /// web reviewer that has no bridge. Off by default.
    #[arg(long)]
    pub snapshot: bool,
    /// Accepted for older callers; does nothing (no snapshot is the default).
    #[arg(long, hide = true)]
    pub no_snapshot: bool,
    /// Where consultations are stored; a relative path resolves against the repo root.
    #[arg(long, default_value = ".collab")]
    pub collab_dir: String,
    /// Emit a JSON object instead of the text summary.
    #[arg(long)]
    pub json: bool,
}

/// Run `c3 fail`. Returns the process exit code.
pub fn run(args: FailArgs) -> i32 {
    let status = match resolve_status(&args.status, &args.status_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("c3 fail: {e}");
            return 1;
        }
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = providers::resolve_repo_root(&cwd);
    let collab_root = providers::resolve_collab_root(&repo_root, &args.collab_dir);

    let opts = FailOpts {
        repo_root,
        collab_root,
        status,
        keep: args.keep,
        snapshot: args.snapshot,
    };

    match finish::run_fail(&opts) {
        Ok(r) => {
            crate::state_dir::print_notes("c3 fail", &r.folder_notes, args.json);
            if args.json {
                let obj = serde_json::json!({
                    "snapshot": r.snapshot_path.as_ref().map(|p| p.display().to_string()),
                    "snapshot_note": r.snapshot_note,
                    "pruned": r.pruned,
                });
                println!("{obj}");
            } else {
                println!(
                    "Task marked BLOCKED/FAILED; {}/{} and the journal updated.",
                    crate::state_dir::STATE_DIR,
                    crate::state_dir::LAST_REPORT
                );
                match (&r.snapshot_path, &r.snapshot_note) {
                    (Some(p), _) => println!("Snapshot: {}", p.display()),
                    (None, Some(note)) => println!("{note}"),
                    (None, None) => {}
                }
            }
            0
        }
        Err(e) => {
            eprintln!("c3 fail: {e}");
            1
        }
    }
}
